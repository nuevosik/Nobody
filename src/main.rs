use std::time::Duration;

use gpui::{App, AsyncApp};
use gpui_platform::application;

use nobody::application::cli::{self, Cli};
use nobody::domain::queue::Queue;
use nobody::infrastructure::config;
use nobody::infrastructure::dbus::{control, host};
use nobody::infrastructure::fullscreen;
use nobody::presentation::shell;

fn on_off(v: bool) -> &'static str {
    if v { "on" } else { "off" }
}

fn run_control(cli: Cli) -> i32 {
    let result = match cli {
        Cli::CenterOpen => control::center_open(),
        Cli::CenterClose => control::center_close(),
        Cli::CenterToggle => control::center_toggle(),
        Cli::Dismiss(id) => control::dismiss(id),
        Cli::DismissApp(app) => control::dismiss_app(&app),
        Cli::ListJson => match control::list_json() {
            Ok(json) => {
                println!("{json}");
                return 0;
            }
            Err(e) => Err(e),
        },
        Cli::HistoryJson => match control::history_json() {
            Ok(json) => {
                println!("{json}");
                return 0;
            }
            Err(e) => Err(e),
        },
        Cli::DismissAll => control::dismiss_all(),
        Cli::DndOn => control::dnd_on(),
        Cli::DndOff => control::dnd_off(),
        Cli::DndToggle => control::dnd_toggle(),
        Cli::DndStatus => match control::dnd_status() {
            Ok((manual, auto, effective)) => {
                println!(
                    "manual={} tela-cheia={} efetivo={}",
                    on_off(manual),
                    on_off(auto),
                    on_off(effective)
                );
                return 0;
            }
            Err(e) => Err(e),
        },
        Cli::Bad(msg) => {
            eprintln!("{msg}");
            return 2;
        }
        Cli::Daemon => unreachable!("daemon não passa por run_control"),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn run_daemon() -> zbus::Result<()> {
    let queue = Queue::new();
    let runtime_config = config::load();
    let initial_connection = zbus::block_on(host::serve(queue.clone(), runtime_config))?;

    application().run(move |cx: &mut App| {
        let host_queue = queue.clone();
        let host_config = runtime_config;
        cx.spawn(async move |cx: &mut AsyncApp| {
            let mut conn = initial_connection;
            loop {
                if conn.is_closed() {
                    eprintln!("nobody: conexão com o session bus perdida; tentando reconectar");
                    loop {
                        match host::serve(host_queue.clone(), host_config).await {
                            Ok(reconnected) => {
                                conn = reconnected;
                                eprintln!("nobody: conexão com o session bus restabelecida");
                                break;
                            }
                            Err(error) if host::is_retryable(&error) => {
                                eprintln!("nobody: reconexão D-Bus falhou: {error}");
                                cx.background_executor().timer(Duration::from_secs(1)).await;
                            }
                            Err(zbus::Error::NameTaken) => {
                                eprintln!(
                                    "nobody: outro daemon ocupa {}",
                                    host::NOTIFICATION_SERVICE
                                );
                                std::process::exit(1);
                            }
                            Err(error) => {
                                eprintln!("nobody: reconexão D-Bus encerrada: {error}");
                                std::process::exit(1);
                            }
                        }
                    }
                }
                host::flush_lifecycle_events(&conn).await;
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
        })
        .detach();

        let quiet_queue = queue.clone();
        cx.spawn(async move |_cx: &mut AsyncApp| {
            loop {
                let changed = fullscreen::changed();
                quiet_queue.set_quiet(fullscreen::quiet_mode());
                changed.await;
            }
        })
        .detach();

        if let Err(e) = shell::open_window(cx, queue, runtime_config) {
            eprintln!("nobody: falha ao abrir janela LayerShell: {e:#}");
            std::process::exit(1);
        }
    });
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse(&args) {
        Cli::Daemon => {
            if let Err(error) = run_daemon() {
                if matches!(error, zbus::Error::NameTaken) {
                    eprintln!(
                        "nobody: outro daemon ocupa {}. Pare o daemon atual antes de iniciar outro",
                        host::NOTIFICATION_SERVICE
                    );
                } else {
                    eprintln!(
                        "nobody: falha ao iniciar serviço no session bus ({error}). Verifique DBUS_SESSION_BUS_ADDRESS"
                    );
                }
                std::process::exit(1);
            }
        }
        cli => std::process::exit(run_control(cli)),
    }
}
