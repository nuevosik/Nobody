//! Nobody — biblioteca do daemon `org.freedesktop.Notifications`.
//!
//! As quatro camadas da Clean Architecture moram aqui; `src/main.rs` é só
//! bootstrap fino que compõe `domain` + `infrastructure` + `presentation`
//! via `Queue` compartilhada.

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;
