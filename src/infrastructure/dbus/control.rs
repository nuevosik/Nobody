use zbus::fdo;

use crate::application::commands;
use crate::domain::history::HistoryEntry;
use crate::domain::notice::Notice;
use crate::domain::queue::Queue;

pub const CONTROL_PATH: &str = "/com/nobody/Control";

fn serialize_list(notices: &[Notice]) -> String {
    let items: Vec<serde_json::Value> = notices
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "app": n.app,
                "summary": n.summary,
                "body": n.body,
                "expire_ms": n.expire_ms,
                "progress": n.progress,
            })
        })
        .collect();
    serde_json::to_string(&serde_json::json!({ "notifications": items }))
        .unwrap_or_else(|_| r#"{"notifications":[]}"#.to_string())
}

fn serialize_history(entries: &[HistoryEntry]) -> String {
    let items: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "id": e.notice.id,
                "app": e.notice.app,
                "summary": e.notice.summary,
                "body": e.notice.body,
                "expire_ms": e.notice.expire_ms,
                "progress": e.notice.progress,
                "seq": e.seq,
            })
        })
        .collect();
    serde_json::to_string(&serde_json::json!({ "notifications": items }))
        .unwrap_or_else(|_| r#"{"notifications":[]}"#.to_string())
}

pub struct ControlService {
    pub queue: Queue,
}

#[zbus::interface(name = "com.nobody.Control")]
impl ControlService {
    pub fn list(&self) -> String {
        serialize_list(&commands::snapshot(&self.queue))
    }

    pub fn history(&self) -> String {
        serialize_history(&commands::history(&self.queue))
    }

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

    pub fn dismiss(&self, id: u32) -> fdo::Result<()> {
        if id == 0 {
            return Err(fdo::Error::InvalidArgs("id must be positive".into()));
        }
        commands::request_dismissal(&self.queue, id);
        Ok(())
    }

    pub fn dismiss_app(&self, app: &str) -> fdo::Result<()> {
        if app.trim().is_empty() {
            return Err(fdo::Error::InvalidArgs("app name must not be empty".into()));
        }
        commands::dismiss_app(&self.queue, app);
        Ok(())
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
    async fn list(&self) -> zbus::Result<String>;
    async fn history(&self) -> zbus::Result<String>;
    async fn center_open(&self) -> zbus::Result<()>;
    async fn center_close(&self) -> zbus::Result<()>;
    async fn center_toggle(&self) -> zbus::Result<()>;
    async fn dismiss_all(&self) -> zbus::Result<()>;
    async fn dismiss(&self, id: u32) -> zbus::Result<()>;
    async fn dismiss_app(&self, app: &str) -> zbus::Result<()>;
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

pub fn list_json() -> Result<String, String> {
    let proxy = blocking_proxy()?;
    proxy.list().map_err(|e| call_err("list", e))
}

pub fn history_json() -> Result<String, String> {
    let proxy = blocking_proxy()?;
    proxy.history().map_err(|e| call_err("history", e))
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

pub fn dismiss(id: u32) -> Result<(), String> {
    let proxy = blocking_proxy()?;
    proxy.dismiss(id).map_err(|e| call_err("dismiss", e))
}

pub fn dismiss_app(app: &str) -> Result<(), String> {
    let proxy = blocking_proxy()?;
    proxy.dismiss_app(app).map_err(|e| call_err("dismiss app", e))
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
    fn dismiss_validates_boundary_and_selects_exact_app() {
        let queue = Queue::new();
        let id = queue
            .push_with_outcome(
                0,
                Notice {
                    id: 0,
                    app: "Spotify".into(),
                    summary: "sum".into(),
                    body: String::new(),
                    icon: None,
                    actions: vec![],
                    expire_ms: 0,
                    arrived_at_ms: 0,
                    stack_tag: None,
                    progress: None,
                },
            )
            .id;
        let svc = ControlService { queue: queue.clone() };

        assert!(matches!(svc.dismiss(0), Err(fdo::Error::InvalidArgs(_))));
        assert!(matches!(svc.dismiss_app("   "), Err(fdo::Error::InvalidArgs(_))));
        svc.dismiss(id).unwrap();
        svc.dismiss_app("Spotify").unwrap();
        assert_eq!(
            queue.drain_close_requests(),
            vec![crate::domain::close::CloseRequest {
                id,
                reason: crate::domain::close::CloseReason::DismissedByUser,
            }]
        );
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
                stack_tag: None,
                progress: None,
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

        queue.set_quiet(true);
        svc.center_open();
        assert!(!queue.is_center_open());

        queue.set_quiet(false);
        assert!(!queue.is_center_open());

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
        queue.set_quiet(true);
        svc.dnd_off();
        assert_eq!(svc.dnd_status(), (false, true, true));
    }

    #[test]
    fn list_and_history_empty() {
        let queue = Queue::new();
        let svc = ControlService { queue };
        assert_eq!(svc.list(), r#"{"notifications":[]}"#);
        assert_eq!(svc.history(), r#"{"notifications":[]}"#);
    }

    #[test]
    fn list_and_history_formatting_and_escaping() {
        let queue = Queue::new();
        queue.push_with_outcome(
            0,
            Notice {
                id: 0,
                app: "App \"Quoted\" & /slash/ \\backslash\\".into(),
                summary: "Line 1\nLine 2".into(),
                body: "Unicode café ☕ 🎉".into(),
                icon: None,
                actions: vec![],
                expire_ms: 5000,
                arrived_at_ms: 100,
                stack_tag: None,
                progress: Some(50),
            },
        );
        let svc = ControlService { queue };
        let list_json = svc.list();
        let history_json = svc.history();

        assert!(!list_json.contains('\n'));
        assert!(!history_json.contains('\n'));

        let list_val: serde_json::Value = serde_json::from_str(&list_json).expect("valid json");
        let item = &list_val["notifications"][0];
        assert_eq!(item["app"], "App \"Quoted\" & /slash/ \\backslash\\");
        assert_eq!(item["summary"], "Line 1\nLine 2");
        assert_eq!(item["body"], "Unicode café ☕ 🎉");
        assert_eq!(item["expire_ms"], 5000);
        assert_eq!(item["progress"], 50);
        assert!(item.get("arrived_at_ms").is_none());

        let hist_val: serde_json::Value = serde_json::from_str(&history_json).expect("valid json");
        let hist_item = &hist_val["notifications"][0];
        assert_eq!(hist_item["app"], "App \"Quoted\" & /slash/ \\backslash\\");
        assert_eq!(hist_item["summary"], "Line 1\nLine 2");
        assert_eq!(hist_item["body"], "Unicode café ☕ 🎉");
        assert_eq!(hist_item["expire_ms"], 5000);
        assert_eq!(hist_item["progress"], 50);
        assert_eq!(hist_item["seq"], 1);
        assert!(hist_item.get("arrived_at_ms").is_none());
    }

    #[test]
    fn list_and_history_differences_and_ordering() {
        let queue = Queue::new();
        let o1 = queue.push_with_outcome(
            0,
            Notice {
                id: 0,
                app: "First".into(),
                summary: "s1".into(),
                body: "b1".into(),
                icon: None,
                actions: vec![],
                expire_ms: 1000,
                arrived_at_ms: 1,
                stack_tag: None,
                progress: None,
            },
        );
        let o2 = queue.push_with_outcome(
            0,
            Notice {
                id: 0,
                app: "Second".into(),
                summary: "s2".into(),
                body: "b2".into(),
                icon: None,
                actions: vec![],
                expire_ms: 2000,
                arrived_at_ms: 2,
                stack_tag: None,
                progress: None,
            },
        );

        queue.remove(o1.id);

        let svc = ControlService { queue: queue.clone() };

        let list_val: serde_json::Value = serde_json::from_str(&svc.list()).unwrap();
        let hist_val: serde_json::Value = serde_json::from_str(&svc.history()).unwrap();

        let list_arr = list_val["notifications"].as_array().unwrap();
        let hist_arr = hist_val["notifications"].as_array().unwrap();

        assert_eq!(list_arr.len(), 1);
        assert_eq!(list_arr[0]["id"], o2.id);
        assert_eq!(list_arr[0]["app"], "Second");

        assert_eq!(hist_arr.len(), 2);
        assert_eq!(hist_arr[0]["id"], o2.id);
        assert_eq!(hist_arr[0]["app"], "Second");
        assert_eq!(hist_arr[0]["seq"], 2);

        assert_eq!(hist_arr[1]["id"], o1.id);
        assert_eq!(hist_arr[1]["app"], "First");
        assert_eq!(hist_arr[1]["seq"], 1);
    }

    #[test]
    fn list_and_history_are_read_only_and_preserve_state() {
        let queue = Queue::new();
        queue.push_with_outcome(
            0,
            Notice {
                id: 0,
                app: "App".into(),
                summary: "sum".into(),
                body: "body".into(),
                icon: None,
                actions: vec![],
                expire_ms: 0,
                arrived_at_ms: 0,
                stack_tag: None,
                progress: None,
            },
        );
        let svc = ControlService { queue: queue.clone() };
        svc.dnd_on();
        svc.center_open();

        let snapshot_before = queue.snapshot();
        let history_before = queue.history_snapshot();
        let dnd_before = svc.dnd_status();
        let center_before = queue.is_center_open();

        let _ = svc.list();
        let _ = svc.history();

        assert_eq!(queue.snapshot(), snapshot_before);
        assert_eq!(queue.history_snapshot(), history_before);
        assert_eq!(svc.dnd_status(), dnd_before);
        assert_eq!(queue.is_center_open(), center_before);
        assert!(queue.drain_close_requests().is_empty());
    }
}
