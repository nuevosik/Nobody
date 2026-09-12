use crate::application::{clock, commands, config::Config};
use crate::domain::close::CloseReason;
use crate::domain::queue::Queue;
use crate::infrastructure::activation;
use crate::infrastructure::dbus::control::{CONTROL_PATH, ControlService};
use crate::infrastructure::dbus::daemon::{self, NOTIFICATION_PATH, NotificationDaemon};

pub const NOTIFICATION_SERVICE: &str = "org.freedesktop.Notifications";

pub fn is_retryable(error: &zbus::Error) -> bool {
    matches!(
        error,
        zbus::Error::Connection(_, _) | zbus::Error::InputOutput(_) | zbus::Error::Handshake(_)
    )
}

pub async fn serve(queue: Queue, config: Config) -> zbus::Result<zbus::Connection> {
    let daemon = NotificationDaemon { queue: queue.clone(), config };
    let control = ControlService { queue };
    zbus::connection::Builder::session()?
        .name(NOTIFICATION_SERVICE)?
        .allow_name_replacements(false)
        .replace_existing_names(false)
        .serve_at(NOTIFICATION_PATH, daemon)?
        .serve_at(CONTROL_PATH, control)?
        .build()
        .await
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
        if queue.remove_if_current(request.id, request.revision).is_none() {
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
        if queue.has_pending_close(request.id, request.revision) {
            continue;
        }
        let Some((desktop_entry, app)) = queue.action_target_if_current(
            request.id,
            request.revision,
            &request.key,
            clock::now_ms(),
        ) else {
            continue;
        };
        if let Err(error) =
            daemon::emit_action_invoked(interface.signal_emitter(), request.id, &request.key).await
        {
            eprintln!("nobody: falha ao sinalizar ação de {}: {error}", request.id);
            continue;
        }
        if request.key == "default" {
            blocking::unblock(move || activation::focus_application(desktop_entry, app)).detach();
        }
        if queue.remove_if_current(request.id, request.revision).is_none() {
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
