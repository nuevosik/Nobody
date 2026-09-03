//! Nobody — daemon `org.freedesktop.Notifications` com GPUI + LayerShell.
//!
//! Arquitetura limpa:
//!   domain        → `state`, `queue`
//!   application   → `provider`, `time`
//!   infrastructure→ `daemon`, `icons`
//!   presentation  → `ui::{stack, popup, anim}`, `theme`

mod daemon;
mod icons;
mod provider;
mod queue;
mod state;
mod theme;
mod time;
mod ui;

use gpui::App;
use gpui_platform::application;

fn main() {
    application().run(|cx: &mut App| {
        if let Err(e) = ui::stack::open_window(cx) {
            eprintln!("nobody: falha ao abrir janela LayerShell: {e:#}");
            std::process::exit(1);
        }
    });
}
