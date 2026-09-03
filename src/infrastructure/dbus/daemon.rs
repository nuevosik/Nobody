//! Infrastructure — D-Bus `org.freedesktop.Notifications`.
use std::collections::HashMap;
use zbus::fdo;
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;

use crate::application::clock;
use crate::application::{commands, policy};
use crate::domain::close::CloseReason;
use crate::domain::notice::Notice;
use crate::domain::queue::Queue;
use crate::infrastructure::dbus::markup::strip_markup;
use crate::infrastructure::dbus::validation::{
    MAX_ACTION_LEN, MAX_ACTIONS, MAX_BODY_LEN, MAX_HINTS, MAX_ICON_LEN, MAX_SUMMARY_LEN,
    is_critical, truncate,
};
use crate::infrastructure::icons::resolve_notice_icon;

pub const NOTIFICATION_PATH: &str = "/org/freedesktop/Notifications";

pub struct NotificationDaemon {
    pub queue: Queue,
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationDaemon {
    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &mut self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<u32> {
        for notice in commands::expire(&self.queue, clock::now_ms()) {
            emit_closed_for_call(&emitter, notice.id, CloseReason::Expired).await?;
        }

        let summary = truncate(&strip_markup(summary), MAX_SUMMARY_LEN);
        let body = truncate(&strip_markup(body), MAX_BODY_LEN);
        let app = if app_name.trim().is_empty() {
            String::from("App")
        } else {
            truncate(app_name.trim(), 64)
        };

        // Limita actions e hints para evitar DoS por payload gigante.
        let mut actions = actions;
        if actions.len() > MAX_ACTIONS {
            actions.truncate(MAX_ACTIONS);
        }
        // trunca cada action individualmente e garante UTF-8 seguro
        let actions: Vec<String> =
            actions.into_iter().map(|a| truncate(&a, MAX_ACTION_LEN)).collect();
        // limita hints: pega só até MAX_HINTS e trunca icon string
        let hints_limited: HashMap<String, OwnedValue> = if hints.len() > MAX_HINTS {
            hints.into_iter().take(MAX_HINTS).collect()
        } else {
            hints
        };
        let app_icon_trunc = truncate(app_icon, MAX_ICON_LEN);
        let is_crit = is_critical(&hints_limited);

        // I/O de disco (lookup de ícone) offloaded para thread blocking.
        let icon_app = app.clone();
        let icon_str = app_icon_trunc.clone();
        let hints_clone = hints_limited.clone();
        let icon =
            blocking::unblock(move || resolve_notice_icon(&icon_str, &icon_app, &hints_clone))
                .await;

        let notice = Notice {
            id: 0,
            app,
            summary,
            body,
            icon,
            actions,
            expire_ms: policy::effective_expire_timeout(expire_timeout, is_crit),
            arrived_at_ms: clock::now_ms(),
        };
        let outcome = self.queue.push_with_outcome(replaces_id, notice);

        for notice in outcome.evicted {
            emit_closed_for_call(&emitter, notice.id, CloseReason::Undefined).await?;
        }

        Ok(outcome.id)
    }

    async fn close_notification(
        &mut self,
        id: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        // Spec: CloseNotification deve ser silencioso se id não existe
        if self.queue.remove(id).is_none() {
            return Ok(());
        }

        emit_closed_for_call(&emitter, id, CloseReason::ClosedByCall).await
    }

    fn get_capabilities(&self) -> Vec<String> {
        // Suportado de verdade: "body" (texto puro, markup é removido via
        // strip_markup) + "icon-static" (resolve_notice_icon). "actions" e
        // "body-markup" NÃO são anunciados: actions são armazenadas/truncadas
        // mas nunca renderizadas e ActionInvoked nunca é emitido (sem path de
        // clique na UI); markup é sempre removido. Anunciar seria violar a spec.
        vec!["body".into(), "icon-static".into()]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        ("nobody".into(), "nobody".into(), env!("CARGO_PKG_VERSION").into(), "1.2".into())
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

pub(crate) async fn emit_notification_closed(
    emitter: &SignalEmitter<'_>,
    id: u32,
    reason: CloseReason,
) -> zbus::Result<()> {
    NotificationDaemon::notification_closed(emitter, id, reason.code()).await
}

async fn emit_closed_for_call(
    emitter: &SignalEmitter<'_>,
    id: u32,
    reason: CloseReason,
) -> fdo::Result<()> {
    emit_notification_closed(emitter, id, reason)
        .await
        .map_err(|error| fdo::Error::Failed(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_only_supported_capabilities() {
        let daemon = NotificationDaemon { queue: Queue::new() };
        let caps = daemon.get_capabilities();
        assert!(caps.contains(&"body".to_string()));
        assert!(caps.contains(&"icon-static".to_string()));
        assert!(!caps.contains(&"actions".to_string()));
        assert!(!caps.contains(&"body-markup".to_string()));
    }

    #[test]
    fn does_not_advertise_unsupported_capabilities() {
        // Contrato real: strip_markup() remove todo markup e ActionInvoked
        // nunca é emitido — logo "body-markup"/"actions" não podem ser anunciados.
        let daemon = NotificationDaemon { queue: Queue::new() };
        let caps = daemon.get_capabilities();
        assert!(caps.contains(&"body".to_string()));
        assert!(caps.contains(&"icon-static".to_string()));
        assert!(!caps.contains(&"body-markup".to_string()));
        assert!(!caps.contains(&"actions".to_string()));
    }
}
