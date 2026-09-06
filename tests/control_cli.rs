use nobody::application::cli;
use nobody::domain::close::{CloseReason, CloseRequest};
use nobody::domain::notice::Notice;
use nobody::domain::queue::Queue;
use nobody::infrastructure::dbus::control::ControlService;

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
    assert!(matches!(cli::parse(&args(&["dnd", "status"])), cli::Cli::DndStatus));
    assert!(matches!(cli::parse(&args(&["dnd", "nope"])), cli::Cli::Bad(_)));
    assert!(matches!(cli::parse(&args(&["list"])), cli::Cli::Bad(_)));
    assert!(matches!(cli::parse(&args(&["history"])), cli::Cli::Bad(_)));
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

    // Chama CenterClose via D-Bus
    let result = session.call_method(
        Some(name.clone()),
        "/com/nobody/Control",
        Some("com.nobody.Control"),
        "CenterClose",
        &(),
    );
    assert!(result.is_ok(), "CenterClose não foi entregue: {result:?}");
    for _ in 0..50 {
        if !server_queue.is_center_open() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!server_queue.is_center_open());

    // Chama CenterOpen via D-Bus
    let result = session.call_method(
        Some(name.clone()),
        "/com/nobody/Control",
        Some("com.nobody.Control"),
        "CenterOpen",
        &(),
    );
    assert!(result.is_ok(), "CenterOpen não foi entregue: {result:?}");
    for _ in 0..50 {
        if server_queue.is_center_open() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(server_queue.is_center_open());

    // Chama CenterOpen novamente (idempotente)
    let result = session.call_method(
        Some(name.clone()),
        "/com/nobody/Control",
        Some("com.nobody.Control"),
        "CenterOpen",
        &(),
    );
    assert!(result.is_ok(), "CenterOpen repetido não foi entregue: {result:?}");
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

    // Chama List via D-Bus
    let list_res = session.call_method(
        Some(name.clone()),
        "/com/nobody/Control",
        Some("com.nobody.Control"),
        "List",
        &(),
    );
    assert!(list_res.is_ok(), "List não foi entregue: {list_res:?}");
    let list_json: String = list_res.unwrap().body().deserialize().expect("deserializes string");
    let list_doc: serde_json::Value = serde_json::from_str(&list_json).expect("valid list json");
    assert_eq!(list_doc["notifications"].as_array().unwrap().len(), 1);
    assert_eq!(list_doc["notifications"][0]["app"], "Bus");

    // Chama History via D-Bus
    let hist_res = session.call_method(
        Some(name.clone()),
        "/com/nobody/Control",
        Some("com.nobody.Control"),
        "History",
        &(),
    );
    assert!(hist_res.is_ok(), "History não foi entregue: {hist_res:?}");
    let hist_json: String = hist_res.unwrap().body().deserialize().expect("deserializes string");
    let hist_doc: serde_json::Value = serde_json::from_str(&hist_json).expect("valid history json");
    assert_eq!(hist_doc["notifications"].as_array().unwrap().len(), 1);
    assert_eq!(hist_doc["notifications"][0]["app"], "Bus");
    assert_eq!(hist_doc["notifications"][0]["seq"], 1);
}

#[test]
fn cli_binary_execution_contract() {
    let bin = env!("CARGO_BIN_EXE_nobody");

    // 1. Argumentos inválidos retornam código 2 e mensagem de uso em stderr
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

    // 2. Sem daemon rodando (em barramento inválido / inexistente), retorna código 1, mensagem em stderr e stdout vazio
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
