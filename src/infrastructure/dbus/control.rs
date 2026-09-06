use crate::application::commands;
use crate::domain::queue::Queue;

pub const CONTROL_PATH: &str = "/com/nobody/Control";

pub struct ControlService {
    pub queue: Queue,
}

#[zbus::interface(name = "com.nobody.Control")]
impl ControlService {
    pub fn center_open(&self) {
        commands::set_center_open(&self.queue, true);
    }

    pub fn center_close(&self) {
        commands::set_center_open(&self.queue, false);
    }

    pub fn center_toggle(&self) {
        self.queue.toggle_center_open();
    }

    pub fn dismiss_all(&self) {
        commands::dismiss_all(&self.queue);
    }

    pub fn dnd_on(&self) {
        self.queue.set_manual_quiet(true);
    }

    pub fn dnd_off(&self) {
        self.queue.set_manual_quiet(false);
    }

    pub fn dnd_toggle(&self) {
        self.queue.toggle_manual_quiet();
    }

    pub fn dnd_status(&self) -> (bool, bool, bool) {
        let manual = self.queue.is_manual_quiet();
        let auto = self.queue.is_quiet();
        (manual, auto, manual || auto)
    }
}

#[zbus::proxy(
    interface = "com.nobody.Control",
    default_service = "org.freedesktop.Notifications",
    default_path = "/com/nobody/Control"
)]
trait Control {
    async fn center_open(&self) -> zbus::Result<()>;
    async fn center_close(&self) -> zbus::Result<()>;
    async fn center_toggle(&self) -> zbus::Result<()>;
    async fn dismiss_all(&self) -> zbus::Result<()>;
    async fn dnd_on(&self) -> zbus::Result<()>;
    async fn dnd_off(&self) -> zbus::Result<()>;
    async fn dnd_toggle(&self) -> zbus::Result<()>;
    async fn dnd_status(&self) -> zbus::Result<(bool, bool, bool)>;
}

fn call_err(op: &str, e: zbus::Error) -> String {
    if let zbus::Error::MethodError(name, _, _) = &e {
        let name = name.to_string();
        if ["UnknownObject", "UnknownMethod", "ServiceUnknown", "NameHasNoOwner"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
        {
            return "nobody: daemon não está rodando".to_string();
        }
    }
    format!("nobody: {op} falhou ({e})")
}

fn blocking_proxy() -> Result<ControlProxyBlocking<'static>, String> {
    let conn = zbus::blocking::Connection::session()
        .map_err(|e| format!("nobody: daemon não está rodando ({e})"))?;
    ControlProxyBlocking::new(&conn)
        .map_err(|e| format!("nobody: falha ao falar com o daemon ({e})"))
}

pub fn center_open() -> Result<(), String> {
    let proxy = blocking_proxy()?;
    proxy.center_open().map_err(|e| call_err("center open", e))
}

pub fn center_close() -> Result<(), String> {
    let proxy = blocking_proxy()?;
    proxy.center_close().map_err(|e| call_err("center close", e))
}

pub fn center_toggle() -> Result<(), String> {
    let proxy = blocking_proxy()?;
    proxy.center_toggle().map_err(|e| call_err("center toggle", e))
}

pub fn dismiss_all() -> Result<(), String> {
    let proxy = blocking_proxy()?;
    proxy.dismiss_all().map_err(|e| call_err("dismiss all", e))
}

pub fn dnd_on() -> Result<(), String> {
    let proxy = blocking_proxy()?;
    proxy.dnd_on().map_err(|e| call_err("dnd on", e))
}

pub fn dnd_off() -> Result<(), String> {
    let proxy = blocking_proxy()?;
    proxy.dnd_off().map_err(|e| call_err("dnd off", e))
}

pub fn dnd_toggle() -> Result<(), String> {
    let proxy = blocking_proxy()?;
    proxy.dnd_toggle().map_err(|e| call_err("dnd toggle", e))
}

pub fn dnd_status() -> Result<(bool, bool, bool), String> {
    let proxy = blocking_proxy()?;
    proxy.dnd_status().map_err(|e| call_err("dnd status", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_flips_center_without_touching_queue() {
        let queue = Queue::new();
        let svc = ControlService { queue: queue.clone() };
        svc.center_toggle();
        assert!(queue.is_center_open());
        assert!(queue.snapshot().is_empty());
        svc.center_toggle();
        assert!(!queue.is_center_open());
    }

    #[test]
    fn open_and_close_idempotent_and_preserve_state() {
        let queue = Queue::new();
        queue.push_with_outcome(
            0,
            crate::domain::notice::Notice {
                id: 0,
                app: "App".into(),
                summary: "sum".into(),
                body: "body".into(),
                icon: None,
                actions: vec![],
                expire_ms: 0,
                arrived_at_ms: 0,
            },
        );
        let notices_before = queue.snapshot();
        let history_before = queue.history_snapshot();
        let svc = ControlService { queue: queue.clone() };

        assert!(!queue.is_center_open());

        svc.center_open();
        assert!(queue.is_center_open());

        svc.center_open();
        assert!(queue.is_center_open());

        svc.center_close();
        assert!(!queue.is_center_open());

        svc.center_close();
        assert!(!queue.is_center_open());

        assert_eq!(queue.snapshot(), notices_before);
        assert_eq!(queue.history_snapshot(), history_before);
        assert!(queue.drain_close_requests().is_empty());
    }

    #[test]
    fn open_respects_fullscreen_and_manual_dnd() {
        let queue = Queue::new();
        let svc = ControlService { queue: queue.clone() };

        // Silêncio automático por tela cheia ativo; executar open -> Continua fechada
        queue.set_quiet(true);
        svc.center_open();
        assert!(!queue.is_center_open());

        // Sair da tela cheia após esse open -> Continua fechada; não agendar abertura
        queue.set_quiet(false);
        assert!(!queue.is_center_open());

        // Apenas Não Perturbe manual ativo; executar open -> Abre a central, preservando Não Perturbe
        svc.dnd_on();
        assert_eq!(svc.dnd_status(), (true, false, true));
        svc.center_open();
        assert!(queue.is_center_open());
        assert_eq!(svc.dnd_status(), (true, false, true));
    }

    #[test]
    fn dnd_methods_drive_only_manual_state() {
        let queue = Queue::new();
        let svc = ControlService { queue: queue.clone() };
        svc.dnd_on();
        assert_eq!(svc.dnd_status(), (true, false, true));
        svc.dnd_off();
        assert_eq!(svc.dnd_status(), (false, false, false));
        svc.dnd_toggle();
        assert_eq!(svc.dnd_status(), (true, false, true));
        // Detector automático nunca é sobrescrito pelo controle manual.
        queue.set_quiet(true);
        svc.dnd_off();
        assert_eq!(svc.dnd_status(), (false, true, true));
    }
}
