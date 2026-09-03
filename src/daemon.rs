//! Infrastructure — D-Bus `org.freedesktop.Notifications`.
//! Toma posse do nome no session bus. Apps chamam `Notify` aqui.

use std::collections::HashMap;

use zbus::fdo;
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;

use crate::icons::resolve_notice_icon;
use crate::provider;
use crate::queue::{CloseReason, Queue};
use crate::state::Notice;
use crate::time;

pub const NOTIFICATION_PATH: &str = "/org/freedesktop/Notifications";

const MAX_SUMMARY_LEN: usize = 200;
const MAX_BODY_LEN: usize = 500;
const MAX_ACTIONS: usize = 20; // 10 pares key+label
const MAX_ACTION_LEN: usize = 64;
const MAX_HINTS: usize = 64;
const MAX_ICON_LEN: usize = 512;

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
        for notice in provider::expire(&self.queue, time::now_ms()) {
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
            expire_ms: provider::effective_expire_timeout(expire_timeout, is_crit),
            arrived_at_ms: time::now_ms(),
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

#[allow(clippy::collapsible_if)]
fn is_critical(hints: &HashMap<String, OwnedValue>) -> bool {
    // Spec: urgency = byte (0 low, 1 normal, 2 critical). Alguns clientes enviam i32.
    if let Some(v) = hints.get("urgency") {
        if let Ok(cloned) = v.try_clone() {
            if let Ok(b) = u8::try_from(cloned) {
                return b >= 2;
            }
        }
        // fallback: tenta como i32/u32 para clientes não-conformes
        if let Ok(cloned) = v.try_clone() {
            if let Ok(n) = i32::try_from(cloned) {
                return n >= 2;
            }
        }
    }
    false
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

/// Remove tags HTML e decodifica entidades básicas.
/// Só remove `<...>` que parecem tags reais; `<` sem `>` ou com espaço após `<`
/// é tratado como literal (evita `5 < 10` virar `5  10` e tags não-fechadas perderem resto).
fn strip_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            // sem fechamento → literal
            let Some(rel_end) = chars[i + 1..].iter().position(|&c| c == '>') else {
                out.push(chars[i]);
                i += 1;
                continue;
            };
            let end = i + 1 + rel_end;
            // conteúdo entre < e >
            let inner: String = chars[i + 1..end].iter().collect();
            // tag válida não começa com espaço e não contém '<' aninhado
            let starts_with_space = chars.get(i + 1).is_some_and(|c| c.is_whitespace());
            let has_nested_lt = inner.contains('<');
            let trimmed = inner.trim();
            let looks_like_tag = !starts_with_space
                && !has_nested_lt
                && !trimmed.is_empty()
                && trimmed
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!' || c == '?');
            if looks_like_tag {
                // pula tag inteira
                i = end + 1;
                continue;
            } else {
                // não é tag → '<' literal
                out.push('<');
                i += 1;
                continue;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    // Ordem importa: &amp; por último para não double-decode &lt;
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&#34;", "\"")
        .replace("&amp;", "&")
        .trim()
        .to_string()
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

    #[test]
    fn detects_critical_urgency() {
        let hints = HashMap::from([("urgency".into(), OwnedValue::from(2_u8))]);

        assert!(is_critical(&hints));
    }

    #[test]
    fn strip_markup_basic() {
        assert_eq!(strip_markup("<b>oi</b> &amp; ola"), "oi & ola");
    }

    #[test]
    fn truncate_limits() {
        assert_eq!(truncate("abcdef", 3), "abc");
    }
}
