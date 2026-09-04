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

- **domain** (`notice.rs`, `queue.rs`, `ids.rs`, `close.rs`) — pure entities:
  notification, bounded queue (`KEEP=12`), id allocation, close reasons.
  No I/O, no logging.
- **application** (`policy.rs`, `commands.rs`, `clock.rs`) — use cases:
  expiry policy (default 5s), queue commands, monotonic clock. No I/O.
- **infrastructure/dbus** (`daemon.rs`, `host.rs`, `validation.rs`,
  `markup.rs`) — `org.freedesktop.Notifications` adapter: validates and
  truncates payloads, strips markup, owns the bus name, flushes lifecycle
  events every 100ms.
- **infrastructure/icons** (`resolver.rs`, `lookup.rs`, `desktop.rs`,
  `cache.rs`) — icon lookup scoped to known locations (name, hint,
  desktop-entry).
- **presentation** (`theme.rs`, `shell/window.rs`, `feed.rs`, `geometry.rs`,
  `popup.rs`, `anim.rs`) — layer-shell overlay: polls `snapshot`, diffs by
  id, draws at most 5 cards with enter/exit animation.

## Data flow

`Notify` → expire pending (`commands::expire`) → validate/truncate →
`strip_markup` → resolve icon on a blocking thread →
`policy::effective_expire_timeout` → `queue::push_with_outcome` → emit
`NotificationClosed`. Render: window ticker reads `snapshot`, `feed` diffs,
`geometry` places, `popup` draws. Dismiss: `window` →
`commands::request_dismissal` → `queue.request_close` → `host` drains and
emits.

## Where new code goes

| Change | Place |
|---|---|
| Timeout/expiry rule | `application/policy.rs` |
| Queue orchestration | `application/commands.rs` |
| Entity, id, close reason | `domain/` |
| D-Bus validation, markup | `infrastructure/dbus/` |
| Icon lookup | `infrastructure/icons/` |
| Layout, geometry, input region | `presentation/shell/geometry.rs` |
| Widget, card, animation, theme | `presentation/shell/`, `presentation/theme.rs` |

## Tests

Integration tests use only the public API (`nobody::...`):

- `tests/queue_invariants.rs` — queue caps at `KEEP=12`, ghost `replaces_id`
  never squats an arbitrary id.
- `tests/dbus_contract.rs` — capabilities announce only `body` +
  `icon-static`; closing an unknown id is silent.
