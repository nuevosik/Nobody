pub(crate) mod cache;
pub mod desktop;
pub(crate) mod inline;
pub mod lookup;
mod resolver;
pub use lookup::resolve_named_icon;
pub use resolver::resolve_notice_icon;
