use nobody::application::cli;
use nobody::domain::close::{CloseReason, CloseRequest};
use nobody::domain::notice::Notice;
use nobody::domain::queue::Queue;
use nobody::infrastructure::dbus::control::ControlService;

#[test]
fn cli_parse_covers_all_commands() {
    let args = |words: &[&str]| words.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(matches!(cli::parse(&args(&[])), cli::Cli::Daemon));
    assert!(matches!(cli::parse(&args(&["center", "toggle"])), cli::Cli::CenterToggle));
    assert!(matches!(cli::parse(&args(&["dismiss", "all"])), cli::Cli::DismissAll));
    assert!(matches!(cli::parse(&args(&["dnd", "status"])), cli::Cli::DndStatus));
    assert!(matches!(cli::parse(&args(&["dnd", "nope"])), cli::Cli::Bad(_)));
}

#[test]
fn control_service_drives_queue_without_bus() {
    let queue = Queue::new();
    let svc = ControlService { queue: queue.clone() };
    svc.center_toggle();
    assert!(queue.is_center_open());
    svc.dnd_on();
    assert_eq!(svc.dnd_status(), (true, false, true));
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

/// Fala com um daemon de teste pelo barramento da sessão, sem iniciar a GUI
/// nem tocar no daemon real (nome próprio). Pula se não houver barramento.
#[test]
fn control_commands_reach_a_test_daemon() {
    let session = match zbus::blocking::Connection::session() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sem session bus, pulando teste live: {e}");
            return;
        }
    };
    let name = format!("com.nobody.Test{}", std::process::id());
    let server_queue = Queue::new();
    let serve_queue = server_queue.clone();
    let serve_name = name.clone();
    let _server = std::thread::spawn(move || {
        let _conn = zbus::blocking::connection::Builder::session()
            .ok()?
            .name(serve_name)
            .ok()?
            .serve_at("/com/nobody/Control", ControlService { queue: serve_queue })
            .ok()?
            .build()
            .ok()?;
        std::thread::sleep(std::time::Duration::from_secs(10));
        Some(())
    });
    assert!(!server_queue.is_center_open());
    let mut delivered = false;
    for _ in 0..50 {
        match session.call_method(
            Some(name.clone()),
            "/com/nobody/Control",
            Some("com.nobody.Control"),
            "CenterToggle",
            &(),
        ) {
            Ok(_) => {
                delivered = true;
                break;
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    }
    assert!(delivered, "daemon de teste não respondeu");
    // Dá um instante para o dispatch assíncrono entregar a chamada.
    for _ in 0..50 {
        if server_queue.is_center_open() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(server_queue.is_center_open());

    server_queue.push_with_outcome(
        0,
        Notice {
            id: 0,
            app: "Bus".into(),
            summary: "dismiss".into(),
            body: String::new(),
            icon: None,
            actions: vec![],
            expire_ms: 0,
            arrived_at_ms: 0,
        },
    );
    let id = server_queue.snapshot()[0].id;
    let result = session.call_method(
        Some(name.clone()),
        "/com/nobody/Control",
        Some("com.nobody.Control"),
        "DismissAll",
        &(),
    );
    assert!(result.is_ok(), "DismissAll não foi entregue: {result:?}");
    let mut requests = Vec::new();
    for _ in 0..50 {
        requests = server_queue.drain_close_requests();
        if !requests.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(requests, vec![CloseRequest { id, reason: CloseReason::DismissedByUser }]);
}
