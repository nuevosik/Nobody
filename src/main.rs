//! Nobody — daemon `org.freedesktop.Notifications` com GPUI + LayerShell.
//!
//! Arquitetura limpa:
//!   domain        → `notice`, `queue`, `close`, `ids`
//!   application   → `policy`, `commands`, `clock`
//!   infrastructure→ `dbus::{daemon, host, markup, validation}`, `icons`
//!   presentation  → `shell::{window, geometry, feed, popup, anim}`, `theme`
//!
//! Bootstrap: cria a `Queue` compartilhada e entrega uma cópia ao `host`
//! (D-Bus) e outra à `window` (UI). As camadas só se falam via `Queue` —
//! `presentation` nunca importa `infrastructure` e vice-versa.

mod application;
mod domain;
mod infrastructure;
mod presentation;

use std::time::Duration;

use gpui::{App, AsyncApp};
use gpui_platform::application;

use crate::domain::queue::Queue;

fn main() {
    application().run(|cx: &mut App| {
        let queue = Queue::new();

        // Infrastructure: hospeda o nome D-Bus e drena lifecycle a cada 100ms.
        let host_queue = queue.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let Some(conn) = crate::infrastructure::dbus::host::serve(host_queue.clone()).await
            else {
                return;
            };
            loop {
                crate::infrastructure::dbus::host::flush_lifecycle_events(&conn, &host_queue).await;
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
        })
        .detach();

        if let Err(e) = presentation::shell::open_window(cx, queue) {
            eprintln!("nobody: falha ao abrir janela LayerShell: {e:#}");
            std::process::exit(1);
        }
    });
}
