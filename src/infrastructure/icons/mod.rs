//! Resolução de ícones de notificação, portado da rot (`providers/notices.rs`
//! + `providers/tray.rs`).
//!
//! Acha um PNG/SVG no disco a partir de nome, hint ou desktop entry.
pub(crate) mod cache;
pub mod desktop;
pub mod lookup;
mod resolver;
pub use lookup::resolve_named_icon;
pub use resolver::resolve_notice_icon;
