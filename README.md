# Nobody

Notification daemon for Wayland, written in Rust. Owns
`org.freedesktop.Notifications` on the session bus and renders notifications
top-right in a GPUI layer-shell window.

## Run

Needs a Wayland session with Layer Shell and a D-Bus session bus. Stop any
daemon already owning the name (e.g. mako):

```bash
cargo run --release
```

Click, Enter, Space or Escape dismisses a notification.
`PREFERS_REDUCED_MOTION=1` disables animations.

## Behavior

- `Notify`, `CloseNotification`, `GetCapabilities`, `GetServerInformation`;
  emits `NotificationClosed`.
- Keeps 12 notifications, renders the 5 most recent; `replaces_id` replaces
  atomically.
- Timeout: `-1` means server default (5s), `0` never expires; critical
  notifications never auto-expire.
- Icon path/name plus `desktop-entry` lookup scoped to known locations;
  markup is stripped. No actions, `image-data` or persistence.

## Architecture

Four layers, one rule: `presentation` and `infrastructure` never import each
other — they talk through the `domain` queue. Details in
`docs/architecture.md`.

```
domain <- application <- infrastructure / presentation
```

## Development

```bash
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```
