# Architecture — Nobody

Notification daemon (`org.freedesktop.Notifications`) in Rust: GPUI +
LayerShell UI, zbus D-Bus adapter. Four layers; `src/main.rs` is bootstrap
only, `src/lib.rs` re-exports the layers for integration tests.

```
domain <- application <- infrastructure / presentation
```

`domain` knows nobody; `application` knows only `domain`; `infrastructure`
and `presentation` depend inward, never on each other — they share state
only through the `Queue`.

Guards in CI (must print nothing):

```bash
! rg -n 'infrastructure::|crate::(daemon|icons)' src/presentation
! rg -n 'presentation::' src/infrastructure
```

## Layers

- **domain** (`notice.rs`, `queue.rs`, `ids.rs`, `close.rs`, `history.rs`,
  `action.rs`) — pure entities:
  notification, bounded queue (`KEEP=12`), id allocation, close reasons,
  bounded history (`HISTORY_KEEP=100`, own `seq` ids) plus shared
  manual-DND / center-open flags. Every render-relevant mutation notifies a
  change event consumed by the UI. No I/O, no logging.
- **application** (`policy.rs`, `commands.rs`, `clock.rs`, `cli.rs`,
  `config.rs`) — use cases: expiry policy (default 5s), queue commands, history
  query/clear/search, DND and center toggles, CLI arg parsing, monotonic clock,
  pure config parsing (the file I/O lives in `infrastructure/config.rs`). No I/O.
- **infrastructure/dbus** (`daemon.rs`, `host.rs`, `control.rs`, `validation.rs`,
  `markup.rs`) — `org.freedesktop.Notifications` adapter: validates and
  truncates payloads, strips markup, owns the bus name before opening the UI,
  reconnects after transport loss and flushes lifecycle events every 100ms.
  `control.rs` serves the separate `com.nobody.Control`
  interface (`/com/nobody/Control`) and holds the blocking client used by
  the CLI commands (no GUI init).
- **infrastructure/icons** (`resolver.rs`, `lookup.rs`, `desktop.rs`,
  `cache.rs`, `inline.rs`) — icon lookup scoped to known locations (name, hint,
  desktop-entry), plus inline `image-data` pixels materialised into
  `~/.cache/nobody/inline/` behind a count and byte cap. `infrastructure/cache_file.rs`
  owns the shared atomic, symlink-resistant cache write.
- **infrastructure/fullscreen** (`fullscreen.rs`) — Hyprland fullscreen
  detector: listens to `.socket2.sock` events, re-queries `hyprctl` on relevant
  changes (1 s poll fallback when the socket is unavailable, 30 s safety
  resync) and publishes quiet-state changes.
- **infrastructure/activation** (`activation.rs`) — Hyprland-only focus for the
  `default` action: picks the most recent mapped window matching
  `desktop-entry` or app name and dispatches `hl.dsp.focus`; no-ops elsewhere.
- **presentation** (`theme.rs`, `shell/window.rs`, `shell/center.rs`,
  `feed.rs`, `geometry.rs`, `popup.rs`, `anim.rs`) — layer-shell overlay: reads
  `snapshot` on queue change events (2 s fallback), diffs by id, draws at most 5
  cards with enter/exit animation. `center.rs` holds the notification center's
  pure text helpers — search-field editing, IME/UTF-16 range mapping, DND
  labels and age formatting — with no gpui dependency; the panel itself lives
  in `window.rs::render_center` and reads `NotificationStack.center_query`.

## Data flow

`Notify` → expire pending (`commands::expire`) → validate/truncate →
`strip_markup` → `policy::effective_expire_timeout` → enqueue → emit
`NotificationClosed`. Icon lookup then runs on the blocking pool and updates
only the matching queue revision. Render: a queue change event wakes the UI,
`feed` diffs the `snapshot`, `geometry` places, `popup` draws. Dismiss: `window` →
`commands::request_dismissal` → `queue.request_close` → `host` drains and
emits.

## Where new code goes

| Change | Place |
|---|---|
| Timeout/expiry rule | `application/policy.rs` |
| Queue orchestration | `application/commands.rs` |
| History query/clear/search, DND/center toggles | `application/commands.rs` |
| CLI arg parsing | `application/cli.rs` (`main.rs` stays bootstrap) |
| Control interface + CLI client | `infrastructure/dbus/control.rs` |
| Entity, id, close reason | `domain/` |
| D-Bus validation, markup | `infrastructure/dbus/` |
| Icon lookup | `infrastructure/icons/` |
| Fullscreen quiet detection | `infrastructure/fullscreen.rs` |
| Hyprland window focus | `infrastructure/activation.rs` |
| Layout, geometry, input region | `presentation/shell/geometry.rs` |
| Widget, card, animation, theme | `presentation/shell/`, `presentation/theme.rs` |

## Tests

Integration tests use only the public API (`nobody::...`):

- `tests/queue_invariants.rs` — queue caps at `KEEP=12`, ghost `replaces_id`
  never squats an arbitrary id.
- `tests/dbus_contract.rs` — capabilities announce `body`, `actions`,
  `icon-static`, `x-dunst-stack-tag`, `x-canonical-private-synchronous`
  and `value`; closing an unknown id is silent.
- `tests/control_cli.rs` — end-to-end lifecycle on an isolated session bus:
  tag replacement, action emission, slow/held MPRIS backpressure, icon worker
  exhaustion, bus ownership and reconnect, and the CLI binary contract.
