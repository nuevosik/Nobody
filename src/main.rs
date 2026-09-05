use std::time::Duration;

use gpui::{App, AsyncApp};
use gpui_platform::application;

use nobody::domain::queue::Queue;
use nobody::infrastructure::dbus::host;
use nobody::infrastructure::fullscreen;
use nobody::presentation::shell;

fn main() {
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
                quiet_queue.set_quiet(fullscreen::quiet_mode());
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
