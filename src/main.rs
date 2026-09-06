use std::time::Duration;

use gpui::{App, AsyncApp};
use gpui_platform::application;

use nobody::application::cli::{self, Cli};
use nobody::domain::queue::Queue;
use nobody::infrastructure::dbus::{control, host};
use nobody::infrastructure::fullscreen;
use nobody::presentation::shell;

fn on_off(v: bool) -> &'static str {
    if v { "on" } else { "off" }
}

fn run_control(cli: Cli) -> i32 {
    let result = match cli {
        Cli::CenterToggle => control::center_toggle(),
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

fn run_daemon() {
    application().run(|cx: &mut App| {
        let queue = Queue::new();

        let host_queue = queue.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let Some(conn) = host::serve(host_queue.clone()).await else {
                return;
            };
            loop {
                host::flush_lifecycle_events(&conn, &host_queue).await;
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
        })
        .detach();

        let quiet_queue = queue.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            loop {
                let quiet = blocking::unblock(fullscreen::quiet_mode).await;
                quiet_queue.set_quiet(quiet);
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        })
        .detach();

        if let Err(e) = shell::open_window(cx, queue) {
            eprintln!("nobody: falha ao abrir janela LayerShell: {e:#}");
            std::process::exit(1);
        }
    });
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse(&args) {
        Cli::Daemon => run_daemon(),
        cli => std::process::exit(run_control(cli)),
    }
}
