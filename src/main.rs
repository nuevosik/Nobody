//! Nobody — daemon `org.freedesktop.Notifications` com GPUI + LayerShell.
//!
//! Arquitetura limpa:
//!   domain        → `state`, `queue`
//!   application   → `provider`, `time`
//!   infrastructure→ `daemon`, `icons`
//!   presentation  → `ui::{stack, popup, anim}`, `theme`

mod application;
mod domain;
mod infrastructure;
mod presentation;

use gpui::App;
use gpui_platform::application;

fn main() {
    application().run(|cx: &mut App| {
        if let Err(e) = presentation::shell::stack::open_window(cx) {
            eprintln!("nobody: falha ao abrir janela LayerShell: {e:#}");
            std::process::exit(1);
        }
    });
}
