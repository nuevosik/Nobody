use zbus::fdo::RequestNameReply;

use crate::application::{clock, commands, config::Config};
use crate::domain::close::CloseReason;
use crate::domain::queue::Queue;
use crate::infrastructure::dbus::control::{CONTROL_PATH, ControlService};
use crate::infrastructure::dbus::daemon::{self, NOTIFICATION_PATH, NotificationDaemon};

pub async fn serve(queue: Queue, config: Config) -> Option<zbus::Connection> {
    let conn = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "nobody: não foi possível conectar ao session bus ({e}). Verifique DBUS_SESSION_BUS_ADDRESS."
            );
            return None;
        }
    };

    let daemon = NotificationDaemon { queue: queue.clone(), config };
    if let Err(e) = conn.object_server().at(NOTIFICATION_PATH, daemon).await {
        eprintln!("nobody: register interface: {e}");
        return None;
    }

    let control = ControlService { queue: queue.clone() };
    if let Err(e) = conn.object_server().at(CONTROL_PATH, control).await {
        eprintln!("nobody: register control interface: {e}");
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

pub async fn flush_lifecycle_events(connection: &zbus::Connection) {
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

    // Serialize validation, signals and removal with Notify/CloseNotification.
    // ponytail: one interface lock; use per-ID serialization if signal I/O becomes a bottleneck.
    let daemon = interface.get_mut().await;
    let queue = &daemon.queue;

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

    for request in queue.drain_action_requests() {
        if queue.has_pending_close(request.id) {
            continue;
        }
        let active_action = queue.snapshot().into_iter().find(|notice| {
            notice.id == request.id
                && notice.has_action(&request.key)
                && !notice.is_expired_at(clock::now_ms())
        });
        if active_action.is_none() {
            continue;
        }
        if let Err(error) =
            daemon::emit_action_invoked(interface.signal_emitter(), request.id, &request.key).await
        {
            eprintln!("nobody: falha ao sinalizar ação de {}: {error}", request.id);
            continue;
        }
        if queue.remove(request.id).is_none() {
            continue;
        }
        if let Err(error) = daemon::emit_notification_closed(
            interface.signal_emitter(),
            request.id,
            CloseReason::DismissedByUser,
        )
        .await
        {
            eprintln!("nobody: falha ao sinalizar fechamento de {}: {error}", request.id);
        }
    }
}
