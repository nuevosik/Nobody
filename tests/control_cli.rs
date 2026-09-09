use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use zbus::zvariant::{OwnedValue, Value};

use nobody::application::{cli, commands, config::Config};
use nobody::domain::close::{CloseReason, CloseRequest};
use nobody::domain::notice::Notice;
use nobody::domain::queue::Queue;
use nobody::infrastructure::dbus::control::{CONTROL_PATH, ControlService};
use nobody::infrastructure::dbus::daemon::{NOTIFICATION_PATH, NotificationDaemon};
use nobody::infrastructure::dbus::host;

const SERVICE: &str = "org.freedesktop.Notifications";
type ClosedEvents = (Receiver<Vec<(u32, u32)>>, Receiver<()>, std::thread::JoinHandle<()>);
type LifecycleEvents = (Receiver<Vec<LifecycleEvent>>, Receiver<()>, std::thread::JoinHandle<()>);

#[derive(Debug, PartialEq, Eq)]
enum LifecycleEvent {
    Action(u32, String),
    Closed(u32, u32),
}

struct IsolatedBus {
    address: String,
    daemon: Child,
}

impl IsolatedBus {
    fn start() -> Self {
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon must be available for the isolated-bus test");
        let stdout = daemon.stdout.take().expect("dbus-daemon stdout");
        let address = BufReader::new(stdout)
            .lines()
            .next()
            .expect("dbus-daemon must print an address")
            .expect("read isolated bus address");
        Self { address, daemon }
    }
}

impl Drop for IsolatedBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

fn server(bus: &IsolatedBus, queue: Queue) -> zbus::Connection {
    let builder = zbus::connection::Builder::address(bus.address.as_str())
        .expect("isolated bus address")
        .name(SERVICE)
        .expect("service name")
        .serve_at(
            NOTIFICATION_PATH,
            NotificationDaemon { queue: queue.clone(), config: Config::default() },
        )
        .expect("notification interface")
        .serve_at(CONTROL_PATH, ControlService { queue })
        .expect("control interface");
    zbus::block_on(builder.build()).expect("build isolated-bus service")
}

fn client(bus: &IsolatedBus) -> zbus::blocking::Connection {
    zbus::blocking::connection::Builder::address(bus.address.as_str())
        .expect("isolated bus address")
        .build()
        .expect("connect isolated-bus client")
}

fn notice(app: &str) -> Notice {
    Notice {
        id: 0,
        app: app.into(),
        summary: "summary".into(),
        body: String::new(),
        icon: None,
        actions: vec![],
        expire_ms: 0,
        arrived_at_ms: 0,
        stack_tag: None,
        progress: None,
    }
}

fn string_hint(value: &str) -> OwnedValue {
    OwnedValue::try_from(Value::from(value)).expect("string hint")
}

fn closed_events(connection: &zbus::blocking::Connection, sentinel_id: u32) -> ClosedEvents {
    let connection = connection.clone();
    let (events_tx, events_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let proxy = zbus::blocking::Proxy::new(
            &connection,
            SERVICE,
            NOTIFICATION_PATH,
            "org.freedesktop.Notifications",
        )
        .expect("notification proxy");
        let mut signals = proxy.receive_signal("NotificationClosed").expect("signal match");
        ready_tx.send(()).expect("signal listener ready");
        let mut events = Vec::new();
        loop {
            let Some(message) = signals.next() else { break };
            let event =
                message.body().deserialize::<(u32, u32)>().expect("NotificationClosed body");
            let is_sentinel = event.0 == sentinel_id;
            events.push(event);
            if is_sentinel {
                break;
            }
        }
        events_tx.send(events).expect("send captured signals");
    });
    (events_rx, ready_rx, handle)
}

fn lifecycle_events(connection: &zbus::blocking::Connection, sentinel_id: u32) -> LifecycleEvents {
    let connection = connection.clone();
    let (events_tx, events_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let proxy = zbus::blocking::Proxy::new(
            &connection,
            SERVICE,
            NOTIFICATION_PATH,
            "org.freedesktop.Notifications",
        )
        .expect("notification proxy");
        let mut signals = proxy.receive_all_signals().expect("signal match");
        ready_tx.send(()).expect("signal listener ready");
        let mut events = Vec::new();
        loop {
            let Some(message) = signals.next() else { break };
            let member = message.header().member().map(ToString::to_string);
            match member.as_deref() {
                Some("ActionInvoked") => {
                    let event =
                        message.body().deserialize::<(u32, String)>().expect("ActionInvoked body");
                    events.push(LifecycleEvent::Action(event.0, event.1));
                }
                Some("NotificationClosed") => {
                    let event = message
                        .body()
                        .deserialize::<(u32, u32)>()
                        .expect("NotificationClosed body");
                    let is_sentinel = event.0 == sentinel_id;
                    events.push(LifecycleEvent::Closed(event.0, event.1));
                    if is_sentinel {
                        break;
                    }
                }
                _ => {}
            }
        }
        events_tx.send(events).expect("send captured signals");
    });
    (events_rx, ready_rx, handle)
}

#[test]
fn cli_parse_covers_all_commands() {
    let args = |words: &[&str]| words.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(matches!(cli::parse(&args(&[])), cli::Cli::Daemon));
    assert!(matches!(cli::parse(&args(&["center", "open"])), cli::Cli::CenterOpen));
    assert!(matches!(cli::parse(&args(&["center", "close"])), cli::Cli::CenterClose));
    assert!(matches!(cli::parse(&args(&["center", "toggle"])), cli::Cli::CenterToggle));
    assert!(matches!(cli::parse(&args(&["list", "--json"])), cli::Cli::ListJson));
    assert!(matches!(cli::parse(&args(&["history", "--json"])), cli::Cli::HistoryJson));
    assert!(matches!(cli::parse(&args(&["dismiss", "all"])), cli::Cli::DismissAll));
    assert!(matches!(cli::parse(&args(&["dismiss", "1"])), cli::Cli::Dismiss(1)));
    assert!(matches!(cli::parse(&args(&["dismiss", "0001"])), cli::Cli::Dismiss(1)));
    assert!(matches!(cli::parse(&args(&["dismiss", "4294967295"])), cli::Cli::Dismiss(u32::MAX)));
    assert!(matches!(cli::parse(&args(&["dismiss", "42"])), cli::Cli::Dismiss(42)));
    assert!(matches!(
        cli::parse(&args(&["dismiss", "app", "My App"])),
        cli::Cli::DismissApp(app) if app == "My App"
    ));
    assert!(matches!(cli::parse(&args(&["dnd", "status"])), cli::Cli::DndStatus));
    assert!(matches!(cli::parse(&args(&["dnd", "nope"])), cli::Cli::Bad(_)));
    assert!(matches!(cli::parse(&args(&["list"])), cli::Cli::Bad(_)));
    assert!(matches!(cli::parse(&args(&["history"])), cli::Cli::Bad(_)));
    for words in [
        vec!["dismiss", ""],
        vec!["dismiss", " "],
        vec!["dismiss", "0"],
        vec!["dismiss", "000"],
        vec!["dismiss", "abc"],
        vec!["dismiss", "+1"],
        vec!["dismiss", "-1"],
        vec!["dismiss", "4294967296"],
        vec!["dismiss", "app"],
        vec!["dismiss", "app", "   "],
        vec!["dismiss", "app", "Name", "extra"],
    ] {
        assert!(matches!(cli::parse(&args(&words)), cli::Cli::Bad(_)), "{words:?}");
    }
}

#[test]
fn control_service_drives_queue_without_bus() {
    let queue = Queue::new();
    queue.push_with_outcome(
        0,
        Notice {
            id: 0,
            app: "TestApp".into(),
            summary: "summary".into(),
            body: "body".into(),
            icon: None,
            actions: vec![],
            expire_ms: 0,
            arrived_at_ms: 0,
            stack_tag: None,
            progress: None,
        },
    );
    let snapshot_before = queue.snapshot();
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

    assert_eq!(queue.snapshot(), snapshot_before);
    assert_eq!(queue.history_snapshot(), history_before);
    assert!(queue.drain_close_requests().is_empty());

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
    svc.center_close();
    assert!(!queue.is_center_open());
    assert_eq!(svc.dnd_status(), (true, false, true));

    svc.center_toggle();
    assert!(queue.is_center_open());
    svc.dnd_toggle();
    assert_eq!(svc.dnd_status(), (false, false, false));
}

#[test]
fn control_service_dismiss_all_queues_user_closures_and_keeps_history() {
    let queue = Queue::new();
    queue.push_with_outcome(
        0,
        Notice {
            id: 0,
            app: "A".into(),
            summary: "a".into(),
            body: String::new(),
            icon: None,
            actions: vec![],
            expire_ms: 0,
            arrived_at_ms: 0,
            stack_tag: None,
            progress: None,
        },
    );
    queue.push_with_outcome(
        0,
        Notice {
            id: 0,
            app: "B".into(),
            summary: "b".into(),
            body: String::new(),
            icon: None,
            actions: vec![],
            expire_ms: 0,
            arrived_at_ms: 0,
            stack_tag: None,
            progress: None,
        },
    );
    let ids = queue.snapshot().into_iter().map(|notice| notice.id).collect::<Vec<_>>();
    let history = queue.history_snapshot();
    let svc = ControlService { queue: queue.clone() };

    svc.dismiss_all();

    assert_eq!(
        queue.drain_close_requests(),
        ids.into_iter()
            .map(|id| CloseRequest { id, reason: CloseReason::DismissedByUser })
            .collect::<Vec<_>>()
    );
    assert_eq!(queue.snapshot().len(), 2);
    assert_eq!(queue.history_snapshot(), history);

    let empty = Queue::new();
    ControlService { queue: empty.clone() }.dismiss_all();
    assert!(empty.drain_close_requests().is_empty());
}

#[test]
fn notify_tag_replacement_runs_on_an_isolated_dbus_and_keeps_one_history_entry() {
    let bus = IsolatedBus::start();
    let queue = Queue::new();
    let _server = server(&bus, queue.clone());
    let client = client(&bus);
    let notifications = zbus::blocking::Proxy::new(
        &client,
        SERVICE,
        NOTIFICATION_PATH,
        "org.freedesktop.Notifications",
    )
    .expect("notification proxy");
    let control = zbus::blocking::Proxy::new(&client, SERVICE, CONTROL_PATH, "com.nobody.Control")
        .expect("control proxy");

    let marker: u32 = notifications
        .call(
            "Notify",
            &(
                "Marker",
                0_u32,
                "",
                "marker",
                "",
                Vec::<String>::new(),
                HashMap::<String, OwnedValue>::new(),
                0_i32,
            ),
        )
        .expect("marker Notify");
    let (events, ready, listener) = closed_events(&client, marker);
    ready.recv_timeout(Duration::from_secs(1)).expect("signal listener startup");

    let first_hints: HashMap<String, OwnedValue> = HashMap::from([
        ("x-canonical-private-synchronous".into(), string_hint("canonical")),
        ("x-dunst-stack-tag".into(), string_hint("volume")),
        ("value".into(), OwnedValue::from(10_i32)),
    ]);
    let first: u32 = notifications
        .call(
            "Notify",
            &("Mixer", 0_u32, "", "first", "body one", Vec::<String>::new(), first_hints, 0_i32),
        )
        .expect("first Notify");

    let second_hints: HashMap<String, OwnedValue> = HashMap::from([
        ("x-dunst-stack-tag".into(), string_hint("volume")),
        ("value".into(), OwnedValue::from(90_i32)),
    ]);
    let second: u32 = notifications
        .call(
            "Notify",
            &(
                "Mixer",
                0_u32,
                "",
                "second",
                "body two",
                Vec::<String>::new(),
                second_hints,
                2_000_i32,
            ),
        )
        .expect("second Notify");
    assert_eq!(second, first, "tag replacement must preserve the notification ID");

    let caps: Vec<String> = notifications.call("GetCapabilities", &()).expect("GetCapabilities");
    assert!(caps.contains(&"x-dunst-stack-tag".to_string()));
    assert!(caps.contains(&"x-canonical-private-synchronous".to_string()));
    assert!(caps.contains(&"value".to_string()));

    let list: serde_json::Value = serde_json::from_str(
        &control.call::<_, _, String>("List", &()).expect("List after replacement"),
    )
    .expect("valid list JSON");
    let active = list["notifications"].as_array().unwrap();
    assert_eq!(active.len(), 2);
    let target = active.iter().find(|item| item["app"] == "Mixer").expect("tagged notice");
    assert_eq!(target["id"], first);
    assert_eq!(target["summary"], "second");
    assert_eq!(target["body"], "body two");
    assert_eq!(target["progress"], 90);
    let history: serde_json::Value = serde_json::from_str(
        &control.call::<_, _, String>("History", &()).expect("History after replacement"),
    )
    .expect("valid history JSON");
    let history = history["notifications"].as_array().unwrap();
    assert_eq!(history.len(), 2);
    let historical_target =
        history.iter().find(|item| item["app"] == "Mixer").expect("tagged history entry");
    assert_eq!(historical_target["id"], first);
    assert_eq!(historical_target["summary"], "second");
    assert_eq!(historical_target["progress"], 90);

    let third_hints: HashMap<String, OwnedValue> =
        HashMap::from([("x-dunst-stack-tag".into(), string_hint("volume"))]);
    let third: u32 = notifications
        .call(
            "Notify",
            &(
                "Mixer",
                0_u32,
                "",
                "third",
                "body three",
                Vec::<String>::new(),
                third_hints,
                2_000_i32,
            ),
        )
        .expect("third Notify");
    assert_eq!(third, first, "tag replacement must preserve the notification ID");
    let final_list: serde_json::Value = serde_json::from_str(
        &control.call::<_, _, String>("List", &()).expect("List after progress removal"),
    )
    .expect("valid final list JSON");
    let final_target = final_list["notifications"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["app"] == "Mixer")
        .expect("final tagged notice");
    assert_eq!(final_target["id"], first);
    assert_eq!(final_target["progress"], serde_json::Value::Null);
    let final_history: serde_json::Value = serde_json::from_str(
        &control.call::<_, _, String>("History", &()).expect("History after progress removal"),
    )
    .expect("valid final history JSON");
    let final_historical_target = final_history["notifications"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["app"] == "Mixer")
        .expect("final tagged history entry");
    assert_eq!(final_historical_target["id"], first);
    assert_eq!(final_historical_target["progress"], serde_json::Value::Null);
    notifications.call::<_, _, ()>("CloseNotification", &marker).expect("close marker");
    let captured = events.recv_timeout(Duration::from_secs(1)).expect("closed marker signal");
    listener.join().expect("signal listener");
    assert_eq!(captured, vec![(marker, 3)], "replacement must not close the old notification");
}

#[test]
fn default_action_emits_once_before_close_and_keeps_history() {
    let bus = IsolatedBus::start();
    let queue = Queue::new();
    let server = server(&bus, queue.clone());
    let client = client(&bus);
    let notifications = zbus::blocking::Proxy::new(
        &client,
        SERVICE,
        NOTIFICATION_PATH,
        "org.freedesktop.Notifications",
    )
    .expect("notification proxy");
    let id: u32 = notifications
        .call(
            "Notify",
            &(
                "App",
                0_u32,
                "",
                "summary",
                "body",
                vec!["default".to_string(), "Abrir".to_string()],
                HashMap::<String, OwnedValue>::new(),
                0_i32,
            ),
        )
        .expect("Notify");
    let caps: Vec<String> = notifications.call("GetCapabilities", &()).unwrap();
    assert!(caps.iter().any(|c| c == "body"));
    assert!(caps.iter().any(|c| c == "actions"));
    let (events, ready, listener) = lifecycle_events(&client, id);
    ready.recv_timeout(Duration::from_secs(1)).expect("signal listener startup");

    commands::request_default_action(&queue, id);
    commands::request_default_action(&queue, id);
    zbus::block_on(host::flush_lifecycle_events(&server));

    let captured = events.recv_timeout(Duration::from_secs(1)).expect("action lifecycle signals");
    listener.join().expect("signal listener");
    assert_eq!(
        captured,
        vec![
            LifecycleEvent::Action(id, "default".into()),
            LifecycleEvent::Closed(id, CloseReason::DismissedByUser.code()),
        ]
    );
    assert!(queue.snapshot().is_empty());
    assert_eq!(queue.history_snapshot().len(), 1);
    assert!(queue.drain_action_requests().is_empty());
}

#[test]
fn named_action_emits_once_and_rejects_unknown_keys() {
    let bus = IsolatedBus::start();
    let queue = Queue::new();
    let server = server(&bus, queue.clone());
    let client = client(&bus);
    let notifications = zbus::blocking::Proxy::new(
        &client,
        SERVICE,
        NOTIFICATION_PATH,
        "org.freedesktop.Notifications",
    )
    .expect("notification proxy");
    let id: u32 = notifications
        .call(
            "Notify",
            &(
                "Brave",
                0_u32,
                "",
                "Video publicado",
                "youtube.com",
                vec!["settings".to_string(), "Configurar".to_string()],
                HashMap::<String, OwnedValue>::new(),
                0_i32,
            ),
        )
        .expect("Notify with named action");
    commands::request_action(&queue, id, "unknown");
    assert!(queue.drain_action_requests().is_empty());
    let (events, ready, listener) = lifecycle_events(&client, id);
    ready.recv_timeout(Duration::from_secs(1)).unwrap();
    commands::request_action(&queue, id, "settings");
    commands::request_action(&queue, id, "settings");
    zbus::block_on(host::flush_lifecycle_events(&server));
    assert_eq!(
        events.recv_timeout(Duration::from_secs(1)).unwrap(),
        vec![
            LifecycleEvent::Action(id, "settings".into()),
            LifecycleEvent::Closed(id, CloseReason::DismissedByUser.code()),
        ]
    );
    listener.join().unwrap();
    assert!(queue.snapshot().is_empty());
    assert_eq!(queue.history_snapshot().len(), 1);
}

#[test]
fn actions_skip_removed_expired_replaced_and_dismissed_notices() {
    let bus = IsolatedBus::start();
    let queue = Queue::new();
    let server = server(&bus, queue.clone());
    let client = client(&bus);
    let mut actionable = notice("App");
    actionable.actions =
        vec!["default".into(), "Abrir".into(), "settings".into(), "Configurar".into()];
    let ids: Vec<_> = (0..4)
        .map(|_| {
            let id = queue.push_with_outcome(0, actionable.clone()).id;
            commands::request_default_action(&queue, id);
            commands::request_action(&queue, id, "settings");
            id
        })
        .collect();
    let marker = queue.push_with_outcome(0, notice("Marker")).id;
    let (events, ready, listener) = lifecycle_events(&client, marker);
    ready.recv_timeout(Duration::from_secs(1)).expect("signal listener startup");

    queue.remove(ids[0]);
    actionable.expire_ms = 1;
    actionable.arrived_at_ms = nobody::application::clock::now_ms();
    queue.push_with_outcome(ids[1], actionable);
    queue.push_with_outcome(ids[2], notice("Replacement without action"));
    commands::request_dismissal(&queue, ids[3]);
    let history = queue.history_snapshot();
    // The expiration test uses the same real monotonic clock as the host.
    std::thread::sleep(Duration::from_millis(2));

    zbus::block_on(host::flush_lifecycle_events(&server));
    zbus::block_on(host::flush_lifecycle_events(&server));
    commands::request_dismissal(&queue, marker);
    zbus::block_on(host::flush_lifecycle_events(&server));

    assert_eq!(
        events.recv_timeout(Duration::from_secs(1)).expect("lifecycle signals"),
        vec![
            LifecycleEvent::Closed(ids[1], CloseReason::Expired.code()),
            LifecycleEvent::Closed(ids[3], CloseReason::DismissedByUser.code()),
            LifecycleEvent::Closed(marker, CloseReason::DismissedByUser.code()),
        ]
    );
    listener.join().expect("signal listener");
    assert_eq!(queue.snapshot().len(), 1);
    assert_eq!(queue.snapshot()[0].id, ids[2]);
    assert_eq!(queue.history_snapshot(), history);
    assert!(queue.drain_action_requests().is_empty());
}

#[test]
fn lifecycle_waits_for_interface_mutation_and_revalidates_pending_action() {
    use std::future::Future;
    use std::task::{Context, Waker};

    let bus = IsolatedBus::start();
    let queue = Queue::new();
    let server = server(&bus, queue.clone());
    let client = client(&bus);
    let mut actionable = notice("App");
    actionable.actions = vec!["default".into(), "Abrir".into()];
    let id = queue.push_with_outcome(0, actionable).id;
    commands::request_default_action(&queue, id);
    let marker = queue.push_with_outcome(0, notice("Marker")).id;
    let (events, ready, listener) = lifecycle_events(&client, marker);
    ready.recv_timeout(Duration::from_secs(1)).expect("signal listener startup");

    let interface = zbus::block_on(
        server.object_server().interface::<_, NotificationDaemon>(NOTIFICATION_PATH),
    )
    .expect("notification interface");
    // Hold the same writer guard used by Notify and CloseNotification.
    let daemon = zbus::block_on(interface.get_mut());
    let mut flush = Box::pin(host::flush_lifecycle_events(&server));
    let mut context = Context::from_waker(Waker::noop());
    assert!(flush.as_mut().poll(&mut context).is_pending());
    let pending = queue.drain_action_requests();
    assert_eq!(pending, vec![nobody::domain::action::ActionRequest { id, key: "default".into() }]);
    queue.request_action(id, "default");
    daemon.queue.push_with_outcome(id, notice("Replacement without action"));
    drop(daemon);
    zbus::block_on(flush);

    commands::request_dismissal(&queue, marker);
    zbus::block_on(host::flush_lifecycle_events(&server));
    assert_eq!(
        events.recv_timeout(Duration::from_secs(1)).expect("lifecycle signals"),
        vec![LifecycleEvent::Closed(marker, CloseReason::DismissedByUser.code())]
    );
    listener.join().expect("signal listener");
    assert_eq!(queue.snapshot().len(), 1);
    assert_eq!(queue.snapshot()[0].id, id);
    assert_eq!(queue.snapshot()[0].app, "Replacement without action");
}

#[test]
fn failed_action_signal_preserves_notice_and_is_not_requeued() {
    let bus = IsolatedBus::start();
    let queue = Queue::new();
    let server = server(&bus, queue.clone());
    let mut actionable = notice("App");
    actionable.actions = vec!["default".into(), "Abrir".into()];
    let id = queue.push_with_outcome(0, actionable).id;
    commands::request_default_action(&queue, id);
    let active = queue.snapshot();
    let history = queue.history_snapshot();

    zbus::block_on(server.clone().close()).expect("close test connection");
    assert!(server.is_closed());
    zbus::block_on(host::flush_lifecycle_events(&server));
    assert!(queue.drain_action_requests().is_empty(), "failed action must not be retried");
    zbus::block_on(host::flush_lifecycle_events(&server));
    assert_eq!(queue.snapshot(), active);
    assert_eq!(queue.history_snapshot(), history);
    assert!(queue.drain_close_requests().is_empty());
}

#[test]
fn control_dismiss_runs_full_lifecycle_on_an_isolated_bus() {
    let bus = IsolatedBus::start();
    let queue = Queue::new();
    let first = queue.push_with_outcome(0, notice("A"));
    let second = queue.push_with_outcome(0, notice("A"));
    let third = queue.push_with_outcome(0, notice("B"));
    let marker = queue.push_with_outcome(0, notice("Marker"));
    let history = queue.history_snapshot();
    let server = server(&bus, queue.clone());
    let client = client(&bus);
    let control = zbus::blocking::Proxy::new(&client, SERVICE, CONTROL_PATH, "com.nobody.Control")
        .expect("control proxy");

    let invalid_id = control.call::<_, _, ()>("Dismiss", &(0_u32));
    assert!(matches!(
        invalid_id,
        Err(zbus::Error::MethodError(name, _, _)) if name.to_string().ends_with("InvalidArgs")
    ));
    for app in ["", "   ", "\t\n"] {
        let invalid_app = control.call::<_, _, ()>("DismissApp", &app);
        assert!(matches!(
            invalid_app,
            Err(zbus::Error::MethodError(name, _, _)) if name.to_string().ends_with("InvalidArgs")
        ));
    }

    control.call::<_, _, ()>("CenterToggle", &()).expect("CenterToggle");
    assert!(queue.is_center_open());
    control.call::<_, _, ()>("CenterClose", &()).expect("CenterClose");
    assert!(!queue.is_center_open());
    control.call::<_, _, ()>("CenterOpen", &()).expect("CenterOpen");
    control.call::<_, _, ()>("CenterOpen", &()).expect("repeated CenterOpen");
    assert!(queue.is_center_open());
    control.call::<_, _, ()>("DndOn", &()).expect("DndOn");
    assert_eq!(
        control.call::<_, _, (bool, bool, bool)>("DndStatus", &()).unwrap(),
        (true, false, true)
    );

    let list: String = control.call("List", &()).expect("List");
    let list: serde_json::Value = serde_json::from_str(&list).expect("valid List JSON");
    assert_eq!(list["notifications"].as_array().unwrap().len(), 4);
    let history_json: String = control.call("History", &()).expect("History");
    let history_json: serde_json::Value =
        serde_json::from_str(&history_json).expect("valid History JSON");
    assert_eq!(history_json["notifications"].as_array().unwrap().len(), 4);

    let (events, ready, listener) = closed_events(&client, marker.id);
    ready.recv_timeout(Duration::from_secs(1)).expect("signal listener startup");
    control.call::<_, _, ()>("DismissApp", &"A").expect("DismissApp");
    control.call::<_, _, ()>("DismissApp", &"A").expect("repeated DismissApp");
    control.call::<_, _, ()>("Dismiss", &(first.id + 99_999)).expect("missing ID is a no-op");
    control.call::<_, _, ()>("DismissApp", &"No such app").expect("missing app is a no-op");
    control.call::<_, _, ()>("Dismiss", &marker.id).expect("sentinel dismissal");
    zbus::block_on(host::flush_lifecycle_events(&server));

    let captured = events.recv_timeout(Duration::from_secs(1)).expect("NotificationClosed signals");
    listener.join().expect("signal listener");
    assert_eq!(
        captured,
        vec![(second.id, 2), (first.id, 2), (marker.id, 2)],
        "snapshot selection, no-op requests and deduplication must be reflected in the signal stream"
    );
    assert_eq!(queue.snapshot().len(), 1);
    assert_eq!(queue.snapshot()[0].id, third.id);
    assert_eq!(queue.snapshot()[0].app, "B");
    assert_eq!(queue.history_snapshot(), history);
    assert!(queue.is_center_open());
    assert_eq!(
        control.call::<_, _, (bool, bool, bool)>("DndStatus", &()).unwrap(),
        (true, false, true)
    );
    assert!(queue.drain_close_requests().is_empty());

    let list: String = control.call("List", &()).expect("List after DismissApp");
    let list: serde_json::Value = serde_json::from_str(&list).expect("valid List JSON");
    assert_eq!(list["notifications"].as_array().unwrap().len(), 1);
    assert_eq!(list["notifications"][0]["app"], "B");
    let history_after_app: String = control.call("History", &()).expect("History after DismissApp");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&history_after_app).unwrap()["notifications"]
            .as_array()
            .unwrap()
            .len(),
        4
    );

    let (all_events, all_ready, all_listener) = closed_events(&client, third.id);
    all_ready.recv_timeout(Duration::from_secs(1)).expect("second signal listener startup");
    control.call::<_, _, ()>("DismissAll", &()).expect("DismissAll");
    control.call::<_, _, ()>("DismissAll", &()).expect("repeated DismissAll");
    zbus::block_on(host::flush_lifecycle_events(&server));
    let all_captured = all_events.recv_timeout(Duration::from_secs(1)).expect("DismissAll signal");
    all_listener.join().expect("second signal listener");
    assert_eq!(all_captured, vec![(third.id, 2)]);
    assert!(queue.snapshot().is_empty());
    assert_eq!(queue.history_snapshot(), history);
    assert!(queue.is_center_open());
    assert_eq!(
        control.call::<_, _, (bool, bool, bool)>("DndStatus", &()).unwrap(),
        (true, false, true)
    );
    assert!(queue.drain_close_requests().is_empty());

    let no_op_marker = queue.push_with_outcome(0, notice("NoOpMarker"));
    let history_with_marker = queue.history_snapshot();
    let (no_op_events, no_op_ready, no_op_listener) = closed_events(&client, no_op_marker.id);
    no_op_ready.recv_timeout(Duration::from_secs(1)).expect("no-op signal listener startup");
    control.call::<_, _, ()>("Dismiss", &(first.id + 99_999)).expect("missing ID is a no-op");
    control.call::<_, _, ()>("DismissApp", &"No such app").expect("missing app is a no-op");
    control.call::<_, _, ()>("Dismiss", &no_op_marker.id).expect("no-op sentinel dismissal");
    zbus::block_on(host::flush_lifecycle_events(&server));
    let no_op_captured = no_op_events.recv_timeout(Duration::from_secs(1)).expect("no-op signal");
    no_op_listener.join().expect("no-op signal listener");
    assert_eq!(no_op_captured, vec![(no_op_marker.id, 2)]);
    assert!(queue.snapshot().is_empty());
    assert_eq!(queue.history_snapshot(), history_with_marker);
    assert!(queue.drain_close_requests().is_empty());
}

#[test]
fn cli_binary_dismisses_on_an_isolated_bus_and_rejects_bad_syntax() {
    let bin = env!("CARGO_BIN_EXE_nobody");
    let bus = IsolatedBus::start();
    let queue = Queue::new();
    let first = queue.push_with_outcome(0, notice("A")).id;
    let second = queue.push_with_outcome(0, notice("A")).id;
    queue.push_with_outcome(0, notice("B"));
    let _server = server(&bus, queue.clone());

    let id = first.to_string();
    let success = Command::new(bin)
        .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
        .args(["dismiss", id.as_str()])
        .output()
        .expect("run dismiss by ID");
    assert_eq!(success.status.code(), Some(0));
    assert!(success.stdout.is_empty());
    assert!(success.stderr.is_empty());

    let success = Command::new(bin)
        .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
        .args(["dismiss", "app", "A"])
        .output()
        .expect("run dismiss by app");
    assert_eq!(success.status.code(), Some(0));
    assert!(success.stdout.is_empty());
    assert!(success.stderr.is_empty());
    assert_eq!(
        queue.drain_close_requests(),
        vec![
            CloseRequest { id: first, reason: CloseReason::DismissedByUser },
            CloseRequest { id: second, reason: CloseReason::DismissedByUser },
        ]
    );

    for bad_args in [
        &["dismiss", "0"][..],
        &["dismiss", "+1"][..],
        &["dismiss", "-1"][..],
        &["dismiss", "4294967296"][..],
        &["dismiss", "app"][..],
        &["dismiss", "app", "   "][..],
        &["dismiss", "42", "extra"][..],
    ] {
        let output = Command::new(bin)
            .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
            .args(bad_args)
            .output()
            .expect("run invalid dismiss syntax");
        assert_eq!(output.status.code(), Some(2), "args {bad_args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("uso: nobody"));
        assert!(output.stdout.is_empty());
    }

    for args in [&["dismiss", "1"][..], &["dismiss", "app", "A"][..]] {
        let output = Command::new(bin)
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/tmp/nobody-nonexistent-bus")
            .args(args)
            .output()
            .expect("run dismiss without daemon");
        assert_eq!(output.status.code(), Some(1), "args {args:?}");
        assert!(!output.stderr.is_empty());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn cli_binary_execution_contract() {
    let bin = env!("CARGO_BIN_EXE_nobody");

    for bad_args in [
        &["list"][..],
        &["history"][..],
        &["list", "extra"][..],
        &["list", "--json", "extra"][..],
        &["history", "extra"][..],
        &["history", "--json", "extra"][..],
    ] {
        let output = std::process::Command::new(bin).args(bad_args).output().expect("runs binary");
        assert_eq!(output.status.code(), Some(2), "args {bad_args:?} should exit with 2");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("uso: nobody"), "stderr should contain usage: {stderr}");
        assert!(output.stdout.is_empty(), "stdout should be empty on bad args");
    }

    for cmd_args in [&["list", "--json"][..], &["history", "--json"][..]] {
        let output = std::process::Command::new(bin)
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/tmp/nobody-nonexistent-bus")
            .args(cmd_args)
            .output()
            .expect("runs binary");
        assert_eq!(output.status.code(), Some(1), "should exit with 1 when daemon is down");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.is_empty(), "stderr should contain error message");
        assert!(output.stdout.is_empty(), "stdout must be empty without partial JSON on failure");
    }
}
