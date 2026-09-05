use zbus::fdo::RequestNameReply;

use crate::application::{clock, commands};
use crate::domain::close::CloseReason;
use crate::domain::queue::Queue;
use crate::infrastructure::dbus::daemon::{self, NOTIFICATION_PATH, NotificationDaemon};

pub async fn serve(queue: Queue) -> Option<zbus::Connection> {
    let conn = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "nobody: não foi possível conectar ao session bus ({e}). Verifique DBUS_SESSION_BUS_ADDRESS."
            );
            return None;
        }
    };

    let daemon = NotificationDaemon { queue: queue.clone() };
    if let Err(e) = conn.object_server().at(NOTIFICATION_PATH, daemon).await {
        eprintln!("nobody: register interface: {e}");
        return None;
    }

    let name = "org.freedesktop.Notifications";
    match conn.request_name_with_flags(name, zbus::fdo::RequestNameFlags::DoNotQueue.into()).await {
        Ok(RequestNameReply::PrimaryOwner) | Ok(RequestNameReply::AlreadyOwner) => {}
        Ok(_) => {
            eprintln!(
                "nobody: outro daemon ocupa {name}. Pare o mako: systemctl --user stop mako && pkill mako"
            );
            return None;
        }
        Err(e) => {
            eprintln!("nobody: request_name {name}: {e}");
            return None;
        }
    }

    Some(conn)
}

pub async fn flush_lifecycle_events(connection: &zbus::Connection, queue: &Queue) {
    let interface = match connection
        .object_server()
        .interface::<_, NotificationDaemon>(NOTIFICATION_PATH)
        .await
    {
        Ok(interface) => interface,
        Err(error) => {
            eprintln!("nobody: não foi possível obter a interface de notificações: {error}");
            return;
        }
    };

    for notice in commands::expire(queue, clock::now_ms()) {
        if let Err(error) = daemon::emit_notification_closed(
            interface.signal_emitter(),
            notice.id,
            CloseReason::Expired,
        )
        .await
        {
            eprintln!("nobody: falha ao sinalizar expiração de {}: {error}", notice.id);
        }
    }

    for request in queue.drain_close_requests() {
        if queue.remove(request.id).is_none() {
            continue;
        }
        if let Err(error) =
            daemon::emit_notification_closed(interface.signal_emitter(), request.id, request.reason)
                .await
        {
            eprintln!("nobody: falha ao sinalizar fechamento de {}: {error}", request.id);
        }
    }
}
