use crate::application::commands;
use crate::domain::queue::Queue;

pub const CONTROL_PATH: &str = "/com/nobody/Control";

pub struct ControlService {
    pub queue: Queue,
}

#[zbus::interface(name = "com.nobody.Control")]
impl ControlService {
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
