# Workspace Clean Arch Reorg Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorganizar `src/` flat para `domain/application/infrastructure/presentation` conforme spec, sem mudar comportamento D-Bus externo.

**Architecture:** Moves 1:1 primeiro para ter baseline verde, depois splits SRP por camada, por fim inversão `presentation -> infrastructure` via `infrastructure::dbus::host`, fechando com docs e guards.

**Tech Stack:** Rust edition 2024, GPUI (git rev f42c6e8, feature wayland), zbus 5 (async-io, blocking-api), anyhow, blocking; `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`.

**Spec:** `docs/superpowers/specs/2026-09-03-workspace-clean-arch-design.md`

## Global Constraints

- `cargo fmt --check` deve passar após cada task.
- `cargo check --all-targets` deve passar após cada task.
- `cargo clippy --all-targets -- -D warnings` deve passar após cada task.
- `cargo test --all-targets` deve passar após cada task.
- Nenhum arquivo final `>200` linhas; alvo `<=150` linhas.
- `src/presentation` nunca contém `infrastructure::` nem `crate::(daemon|icons)`; `src/infrastructure` nunca contém `presentation::`.
- Comportamento D-Bus externo inalterado: `Notify`, `CloseNotification`, `GetCapabilities` (só `body`+`icon-static`), `GetServerInformation`, sinais `NotificationClosed`.
- Limites preservados: summary 200, body 500, actions 20x64, hints 64, icon 512, `KEEP=12`, close_requests `KEEP*2`, cache 256, `.desktop` 16KB.
- Se `.git` não existir, rodar `git init` uma vez antes da Task 1 para permitir os commits do plano.

---

### Task 1: Skeleton move 1:1 (baseline verde na nova árvore)

**Files:**
- Create: `src/domain/mod.rs`, `src/application/mod.rs`, `src/infrastructure/mod.rs`, `src/infrastructure/dbus/mod.rs`, `src/infrastructure/icons/mod.rs`, `src/presentation/mod.rs`, `src/presentation/shell/mod.rs`
- Modify (git mv, conteúdo igual exceto `use`/`mod`): `src/state.rs` → `src/domain/notice.rs`, `src/queue.rs` → `src/domain/queue.rs`, `src/provider.rs` → `src/application/provider.rs`, `src/time.rs` → `src/application/clock.rs`, `src/daemon.rs` → `src/infrastructure/dbus/daemon.rs`, `src/icons.rs` → `src/infrastructure/icons.rs`, `src/theme.rs` → `src/presentation/theme.rs`, `src/ui/stack.rs` → `src/presentation/shell/stack.rs`, `src/ui/popup.rs` → `src/presentation/shell/popup.rs`, `src/ui/anim.rs` → `src/presentation/shell/anim.rs`, `src/main.rs`, `src/ui/mod.rs` (deletar após mover)
- Test: existente `cargo test --all-targets`

**Interfaces:**
- Consumes: nada novo (moves puros).
- Produces: `crate::domain::{notice::Notice, queue::{Queue, CloseReason, CloseRequest, PushOutcome, KEEP}}`, `crate::application::{provider::{effective_expire_timeout, snapshot, expire, request_dismissal, DEFAULT_EXPIRE_MS}, clock::{now_ms, elapsed_ms}}`, `crate::infrastructure::{dbus::daemon::{NotificationDaemon, NOTIFICATION_PATH, emit_notification_closed}, icons::resolve_notice_icon}`, `crate::presentation::{theme, shell::{stack::open_window}}` — mesmos nomes e assinaturas de hoje, só路径 novo.

- [ ] **Step 1: Criar `mod.rs` da nova árvore**

```rust
// src/domain/mod.rs
pub mod notice;
pub mod queue;

// src/application/mod.rs
pub mod clock;
pub mod provider;

// src/infrastructure/mod.rs
pub mod dbus;
pub mod icons;

// src/infrastructure/dbus/mod.rs
pub mod daemon;

// src/infrastructure/icons.rs (nesta task ainda é arquivo único; virará pasta na Task 5)
// conteúdo = atual src/icons.rs na íntegra

// src/presentation/mod.rs
pub mod shell;
pub mod theme;

// src/presentation/shell/mod.rs
pub mod anim;
pub mod popup;
pub mod stack;

// src/main.rs
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
```

- [ ] **Step 2: Mover arquivos e reescrever `use` (sed mecânico + ajuste manual)**

```bash
git init 2>/dev/null || true
mkdir -p src/domain src/application src/infrastructure/dbus src/presentation/shell
git mv src/state.rs src/domain/notice.rs || mv src/state.rs src/domain/notice.rs
git mv src/queue.rs src/domain/queue.rs || mv src/queue.rs src/domain/queue.rs
git mv src/provider.rs src/application/provider.rs || mv src/provider.rs src/application/provider.rs
git mv src/time.rs src/application/clock.rs || mv src/time.rs src/application/clock.rs
git mv src/daemon.rs src/infrastructure/dbus/daemon.rs || mv src/daemon.rs src/infrastructure/dbus/daemon.rs
git mv src/icons.rs src/infrastructure/icons.rs || mv src/icons.rs src/infrastructure/icons.rs
git mv src/theme.rs src/presentation/theme.rs || mv src/theme.rs src/presentation/theme.rs
git mv src/ui/stack.rs src/presentation/shell/stack.rs || mv src/ui/stack.rs src/presentation/shell/stack.rs
git mv src/ui/popup.rs src/presentation/shell/popup.rs || mv src/ui/popup.rs src/presentation/shell/popup.rs
git mv src/ui/anim.rs src/presentation/shell/anim.rs || mv src/ui/anim.rs src/presentation/shell/anim.rs
rmdir src/ui 2>/dev/null || true
```

Reescritas obrigatórias (todas as ocorrências):
- `crate::state::` → `crate::domain::notice::`
- `crate::queue::` → `crate::domain::queue::`
- `crate::provider` → `crate::application::provider`
- `crate::time::` → `crate::application::clock::`
- `crate::icons::` → `crate::infrastructure::icons::`
- `crate::daemon::` → `crate::infrastructure::dbus::daemon::`
- `crate::theme::` → `crate::presentation::theme::`
- `super::anim` / `super::popup` em `stack.rs` continuam válidos (mesmo `shell/`); `crate::ui::` → `crate::presentation::shell::`

- [ ] **Step 3: Rodar baseline**

```bash
cargo fmt
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Expected: todos PASS. Se falhar, é `use` errado — corrigir antes de commitar.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: move src flat para domain/application/infrastructure/presentation (1:1, sem lógica)"
```

---

### Task 2: Domain split (close + ids + notice)

**Files:**
- Create: `src/domain/close.rs`, `src/domain/ids.rs`
- Modify: `src/domain/queue.rs`, `src/domain/notice.rs`, `src/domain/mod.rs`
- Test: `cargo test --all-targets` (testes viajam junto, nenhuma asserção muda)

**Interfaces:**
- Consumes: `crate::domain::notice::Notice { id: u32, app: String, summary: String, body: String, icon: Option<PathBuf>, actions: Vec<String>, expire_ms: i32, arrived_at_ms: u128 }` + `is_expired_at(&self, now_ms: u128) -> bool`.
- Produces: `crate::domain::close::{CloseReason::{Expired, DismissedByUser, ClosedByCall, Undefined}, CloseReason::code(self) -> u32, CloseRequest { id: u32, reason: CloseReason }, PushOutcome { id: u32, evicted: Vec<Notice> }}`, `crate::domain::queue::{Queue::new() -> Self, push_with_outcome(&self, replaces: u32, notice: Notice) -> PushOutcome, remove(&self, id: u32) -> Option<Notice>, snapshot(&self) -> Vec<Notice>, remove_expired_at(&self, now_ms: u128) -> Vec<Notice>, request_close(&self, id: u32, reason: CloseReason), drain_close_requests(&self) -> Vec<CloseRequest>, KEEP: usize = 12}`.

- [ ] **Step 1: Criar `src/domain/close.rs` (mover de `queue.rs:14-39` + prioridade)**

```rust
//! Domain — motivos de fechamento e resultado de push.
use crate::domain::notice::Notice;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum CloseReason {
    Expired = 1,
    DismissedByUser = 2,
    ClosedByCall = 3,
    Undefined = 4,
}

impl CloseReason {
    pub const fn code(self) -> u32 {
        self as u32
    }

    pub(crate) const fn priority(self) -> u8 {
        match self {
            CloseReason::DismissedByUser | CloseReason::ClosedByCall => 3,
            CloseReason::Expired => 2,
            CloseReason::Undefined => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseRequest {
    pub id: u32,
    pub reason: CloseReason,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PushOutcome {
    pub id: u32,
    pub evicted: Vec<Notice>,
}
```

- [ ] **Step 2: Criar `src/domain/ids.rs` (mover `next_available_id` + `reserve_id` de `queue.rs:145-196`)**

```rust
//! Domain — alocação de IDs sem duplicata, com wrap-around seguro.
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::domain::notice::Notice;
use crate::domain::queue::KEEP;

pub(crate) fn next_available_id(next_id: &AtomicU32, notices: &VecDeque<Notice>) -> u32 {
    let mut attempts = 0;
    loop {
        let id = next_id
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                Some(current.checked_add(1).unwrap_or(1))
            })
            .expect("the ID generator always produces a value");
        if !notices.iter().any(|notice| notice.id == id) {
            return id;
        }
        attempts += 1;
        if attempts > KEEP * 4 {
            for cand in 1..=u32::MAX {
                if !notices.iter().any(|n| n.id == cand) {
                    let _ = next_id.fetch_update(Ordering::AcqRel, Ordering::Acquire, |_| {
                        Some(cand.checked_add(1).unwrap_or(1))
                    });
                    return cand;
                }
                if cand == u32::MAX {
                    break;
                }
            }
            return id;
        }
    }
}

pub(crate) fn reserve_id(next_id: &AtomicU32, id: u32) {
    if id == u32::MAX {
        let _ = next_id.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current == u32::MAX).then_some(1)
        });
        return;
    }
    let next = id + 1;
    let _ = next_id.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
        (current <= id).then_some(next)
    });
}
```

- [ ] **Step 3: Enxugar `src/domain/queue.rs` para delegar a `close`/`ids`**

```rust
//! Domain — fila compartilhada entre D-Bus e UI.
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::domain::close::{CloseReason, CloseRequest, PushOutcome};
use crate::domain::ids::{next_available_id, reserve_id};
use crate::domain::notice::Notice;

pub const KEEP: usize = 12;
// ... push_with_outcome/remove/snapshot/remove_expired_at/request_close/drain_close_requests
// iguais a hoje, exceto: `self.reserve_id(x)` vira `reserve_id(&self.next_id, x)`,
// `self.next_available_id(&inner)` vira `next_available_id(&self.next_id, &inner)`,
// closure `priority` em request_close vira `reason.priority() > existing.reason.priority()`,
// e os métodos privados next_available_id/reserve_id são deletados.
```

Atualizar `src/domain/mod.rs`:

```rust
pub mod close;
pub mod ids;
pub mod notice;
pub mod queue;
```

Mover os `#[cfg(test)]` de `CloseReason`/`reserve_id` para `close.rs`/`ids.rs` junto com o código; manter testes de `Queue` em `queue.rs` sem mudar asserções.

- [ ] **Step 4: Rodar**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test --all-targets
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/domain
git commit -m "refactor(domain): extrai close.rs e ids.rs de queue.rs"
```

---

### Task 3: Application split (policy + commands, clock já renomeado)

**Files:**
- Create: `src/application/policy.rs`, `src/application/commands.rs`
- Modify: `src/application/mod.rs` (trocar `provider` por `policy`+`commands`), deletar `src/application/provider.rs`
- Test: `cargo test --all-targets`

**Interfaces:**
- Consumes: `crate::domain::queue::Queue`, `crate::domain::notice::Notice`, `crate::domain::close::{CloseReason, CloseRequest, PushOutcome}`.
- Produces: `crate::application::policy::{DEFAULT_EXPIRE_MS: i32 = 5000, effective_expire_timeout(requested_timeout: i32, is_critical: bool) -> i32}`, `crate::application::commands::{snapshot(queue: &Queue) -> Vec<Notice>, expire(queue: &Queue, now_ms: u128) -> Vec<Notice>, request_dismissal(queue: &Queue, id: u32)}`, `crate::application::clock::{now_ms() -> u128, elapsed_ms(since: u128) -> u128}`.

- [ ] **Step 1: Criar `policy.rs` + testes (conteúdo atual de `provider.rs:6-17` + testes 35-50)**

```rust
//! Application — política de expiração.
pub const DEFAULT_EXPIRE_MS: i32 = 5_000;

pub fn effective_expire_timeout(requested_timeout: i32, is_critical: bool) -> i32 {
    if is_critical || requested_timeout == 0 {
        0
    } else if requested_timeout < 0 {
        DEFAULT_EXPIRE_MS
    } else {
        requested_timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uses_the_server_default_when_timeout_is_unspecified() {
        assert_eq!(effective_expire_timeout(-1, false), DEFAULT_EXPIRE_MS);
    }
    #[test]
    fn zero_and_critical_notifications_do_not_expire() {
        assert_eq!(effective_expire_timeout(0, false), 0);
        assert_eq!(effective_expire_timeout(-1, true), 0);
        assert_eq!(effective_expire_timeout(500, true), 0);
    }
    #[test]
    fn preserves_an_explicit_timeout_for_normal_notifications() {
        assert_eq!(effective_expire_timeout(500, false), 500);
    }
}
```

- [ ] **Step 2: Criar `commands.rs` (conteúdo atual de `provider.rs:19-29`)**

```rust
//! Application — comandos finos sobre o domain.
use crate::domain::close::CloseReason;
use crate::domain::notice::Notice;
use crate::domain::queue::Queue;

pub fn snapshot(queue: &Queue) -> Vec<Notice> {
    queue.snapshot()
}

pub fn expire(queue: &Queue, now_ms: u128) -> Vec<Notice> {
    queue.remove_expired_at(now_ms)
}

pub fn request_dismissal(queue: &Queue, id: u32) {
    queue.request_close(id, CloseReason::DismissedByUser);
}
```

Atualizar `mod.rs`:

```rust
pub mod clock;
pub mod commands;
pub mod policy;
```

Deletar `provider.rs`. Reescrever `use crate::application::provider::` → `crate::application::{policy, commands}::` em `daemon.rs` e `stack.rs` (ex.: `provider::expire` → `commands::expire`, `provider::effective_expire_timeout` → `policy::effective_expire_timeout`, `provider::snapshot` → `commands::snapshot`, `provider::request_dismissal` → `commands::request_dismissal`).

- [ ] **Step 3: Rodar**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test --all-targets
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/application
git commit -m "refactor(application): divide provider em policy + commands"
```

---

### Task 4: Infra dbus split (validation + markup)

**Files:**
- Create: `src/infrastructure/dbus/validation.rs`, `src/infrastructure/dbus/markup.rs`
- Modify: `src/infrastructure/dbus/daemon.rs`, `src/infrastructure/dbus/mod.rs`
- Test: `cargo test --all-targets`

**Interfaces:**
- Consumes: `std::collections::HashMap`, `zbus::zvariant::OwnedValue`.
- Produces: `crate::infrastructure::dbus::validation::{MAX_SUMMARY_LEN: usize = 200, MAX_BODY_LEN: usize = 500, MAX_ACTIONS: usize = 20, MAX_ACTION_LEN: usize = 64, MAX_HINTS: usize = 64, MAX_ICON_LEN: usize = 512, truncate(s: &str, max: usize) -> String, is_critical(hints: &HashMap<String, OwnedValue>) -> bool}`, `crate::infrastructure::dbus::markup::{strip_markup(s: &str) -> String}`; `daemon.rs` mantém `NotificationDaemon { queue: Queue }`, `NOTIFICATION_PATH`, `emit_notification_closed`, interface zbus com mesmas assinaturas.

- [ ] **Step 1: Criar `validation.rs` (mover consts `daemon.rs:19-24` + `truncate:179-184` + `is_critical:160-177` + testes `truncate_limits`, `detects_critical_urgency`)**

```rust
//! Infrastructure/dbus — limites anti-DoS e validação.
use std::collections::HashMap;
use zbus::zvariant::OwnedValue;

pub const MAX_SUMMARY_LEN: usize = 200;
pub const MAX_BODY_LEN: usize = 500;
pub const MAX_ACTIONS: usize = 20;
pub const MAX_ACTION_LEN: usize = 64;
pub const MAX_HINTS: usize = 64;
pub const MAX_ICON_LEN: usize = 512;

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

pub(crate) fn is_critical(hints: &HashMap<String, OwnedValue>) -> bool {
    if let Some(v) = hints.get("urgency") {
        if let Ok(cloned) = v.try_clone()
            && let Ok(b) = u8::try_from(cloned)
        {
            return b >= 2;
        }
        if let Ok(cloned) = v.try_clone()
            && let Ok(n) = i32::try_from(cloned)
        {
            return n >= 2;
        }
    }
    false
}
```

(Manter os dois testes movidos junto.)

- [ ] **Step 2: Criar `markup.rs` (mover `strip_markup:186-241` + teste `strip_markup_basic` sem mudar lógica nem ordem de `replace`)**

```rust
//! Infrastructure/dbus — remove markup, preserva `<` literal e decodifica entidades.
pub fn strip_markup(s: &str) -> String {
    // ... corpo idêntico ao atual daemon.rs:189-241 ...
}
```

- [ ] **Step 3: Enxugar `daemon.rs` para só protocolo**

Cabeçalho passa a:

```rust
//! Infrastructure — D-Bus `org.freedesktop.Notifications`.
use std::collections::HashMap;
use zbus::fdo;
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;

use crate::application::{commands, policy};
use crate::application::clock;
use crate::domain::close::CloseReason;
use crate::domain::notice::Notice;
use crate::domain::queue::Queue;
use crate::infrastructure::dbus::markup::strip_markup;
use crate::infrastructure::dbus::validation::{
    MAX_ACTIONS, MAX_ACTION_LEN, MAX_BODY_LEN, MAX_HINTS, MAX_ICON_LEN, MAX_SUMMARY_LEN,
    is_critical, truncate,
};
use crate::infrastructure::icons::resolve_notice_icon;
```

Corpo do `notify` idêntico, só trocando `provider::expire` → `commands::expire`, `time::now_ms` → `clock::now_ms`, `provider::effective_expire_timeout` → `policy::effective_expire_timeout`. Atualizar `mod.rs`:

```rust
pub mod daemon;
pub mod markup;
pub mod validation;
```

(`host` será declarado na Task 6.)

- [ ] **Step 4: Rodar**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test --all-targets
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/infrastructure/dbus
git commit -m "refactor(infra-dbus): extrai validation.rs e markup.rs de daemon.rs"
```

---

### Task 5: Infra icons split (resolver + lookup + desktop + cache)

**Files:**
- Create: `src/infrastructure/icons/resolver.rs`, `lookup.rs`, `desktop.rs`, `cache.rs`
- Modify: `src/infrastructure/icons.rs` → `src/infrastructure/icons/mod.rs` (só re-exporta), `src/infrastructure/mod.rs`
- Test: `cargo test --all-targets`

**Interfaces:**
- Consumes: `HashMap<String, OwnedValue>`, `std::path::{Path, PathBuf}`, env `XDG_DATA_DIRS/XDG_DATA_HOME/HOME`.
- Produces: `crate::infrastructure::icons::{resolve_notice_icon(app_icon: &str, app: &str, hints: &HashMap<String, OwnedValue>) -> Option<PathBuf>, resolve_named_icon(name: &str) -> Option<PathBuf>}` (re-exportados pelo `mod.rs`); internos `lookup::lookup_named_icon`, `desktop::{icon_from_desktop, desktop_icon_key}`, `cache::{cached_or_lookup}`.

- [ ] **Step 1: Criar `cache.rs` (mover `ICON_CACHE_LIMIT` + bloco `CACHE` de `icons.rs:106-124`)**

```rust
//! Infra/icons — cache limitado.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

pub const ICON_CACHE_LIMIT: usize = 256;

static CACHE: LazyLock<Mutex<HashMap<String, Option<PathBuf>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn cached(name: &str) -> Option<Option<PathBuf>> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(name).cloned()
}

pub fn store(name: &str, found: Option<PathBuf>) {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if cache.len() >= ICON_CACHE_LIMIT && !cache.contains_key(name) {
        let to_remove = ICON_CACHE_LIMIT / 4;
        let keys: Vec<String> = cache.keys().take(to_remove).cloned().collect();
        for k in keys {
            cache.remove(&k);
        }
    }
    cache.insert(name.to_string(), found);
}
```

- [ ] **Step 2: Criar `lookup.rs` + `desktop.rs` + `resolver.rs` (mover sem mudar lógica)**

`lookup.rs` recebe `resolve_named_icon` (com `cache::cached/store` no lugar do `CACHE` inline) + `lookup_named_icon` + `SIZES` + roots. `desktop.rs` recebe `icon_from_desktop` + `desktop_icon_key`. `resolver.rs` recebe `resolve_notice_icon` + `hint_string`. Mover cada `#[cfg(test)]` junto com sua função.

`mod.rs`:

```rust
pub mod cache;
pub mod desktop;
pub mod lookup;
mod resolver;
pub use resolver::resolve_notice_icon;
pub use lookup::resolve_named_icon;
```

- [ ] **Step 3: Rodar**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test --all-targets
```

Expected: PASS, incluindo `file_uri_traversal_is_rejected`, `thematic_lookup_rejects_directories`, `desktop_huge_file_is_rejected`.

- [ ] **Step 4: Commit**

```bash
git add src/infrastructure/icons*
git commit -m "refactor(infra-icons): divide em resolver/lookup/desktop/cache"
```

---

### Task 6: Presentation split + inversão (host sai da UI)

**Files:**
- Create: `src/infrastructure/dbus/host.rs`, `src/presentation/shell/window.rs`, `src/presentation/shell/geometry.rs`, `src/presentation/shell/feed.rs`
- Modify: `src/presentation/shell/stack.rs` (deletar após mover), `src/presentation/shell/mod.rs`, `src/infrastructure/dbus/mod.rs`, `src/main.rs`
- Test: `cargo test --all-targets` + `rg` guards

**Interfaces:**
- Consumes: `crate::domain::queue::Queue`, `crate::application::{clock, commands}`, `crate::presentation::{theme, shell::{geometry, feed, popup, anim}}`; `host` consome `zbus::Connection`, `NotificationDaemon`, `NOTIFICATION_PATH`.
- Produces: `crate::infrastructure::dbus::host::serve(queue: Queue)` (spawna task D-Bus: connect, `object_server().at`, `request_name DoNotQueue`, loop 100ms `flush_lifecycle_events` + `commands::snapshot`), `crate::presentation::shell::window::open_window(cx: &mut gpui::App) -> anyhow::Result<()>` (não recebe `Queue`? recebe: `open_window_with_queue` interno para teste), `crate::presentation::shell::geometry::{grouped_y_map(notices: &[Notice]) -> Vec<f32>, total_h_current_for(notices: &[Notice], n: usize) -> f32}`, `crate::presentation::shell::feed::{Stack { notices: Vec<Notice> }, apply_snapshot(stack: &mut Stack, snapshot: Vec<Notice>) -> Vec<(u32, f32)>}`.

- [ ] **Step 1: Criar `host.rs` (mover `spawn_dbus:165-251` + `flush_lifecycle_events:253-289` de `stack.rs`, sem `gpui`)**

```rust
//! Infrastructure/dbus — hospeda o nome e drena lifecycle.
use std::time::Duration;
use zbus::fdo::RequestNameReply;
use crate::application::commands;
use crate::application::clock;
use crate::domain::close::CloseReason;
use crate::domain::queue::Queue;
use crate::infrastructure::dbus::daemon::{self, NOTIFICATION_PATH, NotificationDaemon};

pub async fn serve(queue: Queue) {
    // ... corpo idêntico ao spawn_dbus, exceto que em vez de this.update(cx)
    // apenas chama flush_lifecycle_events + expire; o sync com Stack vive em feed.rs
}
```

Na prática: `serve` faz connect, `at`, `request_name`, loop com `flush_lifecycle_events(&conn, &queue).await` + `tokio`-like sleep via `async_io::Timer` ou mantém `cx.background_executor` se chamado de dentro do `cx.spawn` em `window.rs`. Para não trocar runtime nesta task, `window.rs::new` chama `cx.spawn(|this, cx| host::pump(conn, queue, this, cx))` onde `pump` contém o loop atual `207-249` movido, e `flush` contém `253-289`. Nenhum `gpui::Render` em `host.rs`.

- [ ] **Step 2: Criar `geometry.rs` (mover `DECK_GAP`, `MIN_HIT`, `grouped_y`, `grouped_y_map`, `total_h_current_for`, `sync_window_geometry` + teste `interleaved_apps_total_h_uses_max_y_not_last_index`)**

Assinatura de `sync_window_geometry` passa a receber `&mut Option<f32>` + `&mut usize` em vez de `self`, para não depender do struct:

```rust
pub fn sync_window_geometry(
    window: &mut gpui::Window,
    last_h: &mut Option<f32>,
    last_n: &mut usize,
    notices: &[crate::domain::notice::Notice],
    total_h: f32,
    n: usize,
) { /* corpo idêntico ao atual 102-133 */ }
```

- [ ] **Step 3: Criar `feed.rs` (mover `Stack`, `Exiting`, diff snapshot)**

```rust
//! Presentation/shell — estado de tela.
use crate::domain::notice::Notice;

#[derive(Clone, Default)]
pub struct Stack {
    pub notices: Vec<Notice>,
}

pub struct Exiting {
    pub notice: Notice,
    pub start_ms: u128,
    pub y: f32,
}
```

`window.rs` mantém `NotificationStack { stack: Stack, exiting: Vec<Exiting>, queue: Queue, last_window_h, last_input_len }`, `new` (spawna ticker + `host::serve`), `dismiss` (`commands::request_dismissal`), `render` (idêntico, só `use` novos), `open_window`. Deletar `stack.rs` antigo. `mod.rs`:

```rust
pub mod anim;
pub mod feed;
pub mod geometry;
pub mod popup;
pub mod window;
pub use window::open_window;
```

`main.rs` chama `presentation::shell::window::open_window`. Verificar com:

```bash
rg "infrastructure::|crate::(daemon|icons)" src/presentation || echo "presentation limpa"
rg "presentation::" src/infrastructure || echo "infra limpa"
```

Expected: ambas imprimem "limpa".

- [ ] **Step 4: Rodar**

```bash
cargo fmt && cargo check --all-targets && cargo clippy --all-targets -- -D warnings && cargo test --all-targets
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/presentation src/infrastructure/dbus src/main.rs
git commit -m "refactor: extrai host da UI e divide shell em window/geometry/feed"
```

---

### Task 7: Docs + testes de integração + guards

**Files:**
- Create: `docs/architecture.md`, `tests/queue_invariants.rs`, `tests/dbus_contract.rs`
- Modify: `README.md:36-48`, `.github/workflows/ci.yml` (adicionar guards `rg`)
- Test: `cargo test --all-targets`

**Interfaces:**
- Consumes: `crate::domain`, `crate::application` (testes de integração usam só API pública).
- Produces: docs e guards verdes.

- [ ] **Step 1: Criar `tests/queue_invariants.rs`**

```rust
use nobody::domain::notice::Notice;
use nobody::domain::queue::{KEEP, Queue};

fn mk(app: &str) -> Notice {
    Notice { id: 0, app: app.into(), summary: "s".into(), body: "".into(), icon: None, actions: vec![], expire_ms: 0, arrived_at_ms: 0 }
}

#[test]
fn caps_at_keep_and_reports_evicted() {
    let q = Queue::new();
    for _ in 0..KEEP { q.push_with_outcome(0, mk("A")); }
    let out = q.push_with_outcome(0, mk("B"));
    assert_eq!(q.snapshot().len(), KEEP);
    assert_eq!(out.evicted.len(), 1);
}

#[test]
fn ghost_replaces_does_not_squat() {
    let q = Queue::new();
    let ghost = q.push_with_outcome(999_999, mk("Ghost")).id;
    assert_ne!(ghost, 999_999);
}
```

Nota: exige `src/lib.rs` ou expor via `[[test]]` com `path`? Como hoje é bin-only (`src/main.rs`), adicionar `src/lib.rs` com `pub mod application; pub mod domain; pub mod infrastructure; pub mod presentation;` e `main.rs` passa a `use nobody::...`. Fazer isso neste step (mover `mod` para lib, `main.rs` só bootstrap). Se preferir não criar lib, colocar os testes como `#[cfg(test)]` em `queue.rs` — escolher lib (padrão Clean: bin fino).

- [ ] **Step 2: Criar `tests/dbus_contract.rs`**

```rust
use nobody::domain::queue::Queue;
use nobody::infrastructure::dbus::daemon::NotificationDaemon;

#[test]
fn caps_are_body_and_icon_static_only() {
    let d = NotificationDaemon { queue: Queue::new() };
    let caps = d.get_capabilities();
    assert!(caps.contains(&"body".to_string()));
    assert!(caps.contains(&"icon-static".to_string()));
    assert!(!caps.contains(&"actions".to_string()));
    assert!(!caps.contains(&"body-markup".to_string()));
}
```

(Requer `get_capabilities` `pub` — tornar `pub` nesta task se ainda for privado.)

- [ ] **Step 3: Escrever `docs/architecture.md` + atualizar README**

`docs/architecture.md` contém: árvore final (copiar da spec seção 2), Dependency Rule em 3 linhas, tabela "onde pôr código novo" (timeout → `application/policy`; formato de ícone → `infra/icons`; layout → `presentation/shell/geometry`). README `## Arquitetura`: substituir bloco `36-48` pela nova árvore + frase "presentation nunca importa infrastructure".

- [ ] **Step 4: Adicionar guards ao CI**

```yaml
# .github/workflows/ci.yml, após clippy, antes de test:
- run: "! rg -n 'infrastructure::|crate::(daemon|icons)' src/presentation"
- run: "! rg -n 'presentation::' src/infrastructure"
```

- [ ] **Step 5: Rodar tudo**

```bash
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Expected: PASS em tudo + guards silenciosos.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "docs+test: architecture.md, README, testes integração e guards de camada"
```

---
