# Nobody

Notification daemon for Wayland, written in Rust. It owns the
`org.freedesktop.Notifications` name on the session bus and shows notifications
in a GPUI layer on the top-right corner of the screen.

## Run

Nobody needs a Wayland session with Layer Shell and a D-Bus session bus.
Stop any daemon that already owns the notifications name (e.g. mako) and run:

```bash
cargo run --release
```

## Current behavior

- Implements `Notify`, `CloseNotification`, `GetCapabilities` and
  `GetServerInformation` from the Desktop Notifications protocol.
- Keeps up to 12 active notifications and renders the five most recent.
- Emits `NotificationClosed` for expiry, user dismissal,
  `CloseNotification` and capacity eviction.
- Atomically replaces a notification when it receives `replaces_id`.
- Honors the client-requested timeout; `-1` uses the server default (5 seconds),
  `0` never expires and critical notifications never expire automatically.
- Accepts icon paths/names and `desktop-entry`. Lookup is limited to known
  locations so it does not scan the disk on every notification.
- Supports text body. Markup is stripped before rendering; actions,
  `image-data` and persistent history are not supported yet.
- Click, Enter, Space or Escape dismisses a notification. Set
  `PREFERS_REDUCED_MOTION=1` to disable animations.

## Architecture

Clean Architecture in four layers (`src/lib.rs` re-exports all of them;
`src/main.rs` is bootstrap only). `presentation` never imports
`infrastructure` and vice versa — layers only talk through `Queue`.
Details in `docs/architecture.md`.

```
src/
  main.rs
  lib.rs
  domain/
    mod.rs
    notice.rs
    queue.rs
    ids.rs
    close.rs
  application/
    mod.rs
    policy.rs
    commands.rs
    clock.rs
  infrastructure/
    mod.rs
    dbus/
      mod.rs
      daemon.rs
      validation.rs
      markup.rs
      host.rs
    icons/
      mod.rs
      resolver.rs
      lookup.rs
      desktop.rs
      cache.rs
  presentation/
    mod.rs
    theme.rs
    shell/
      mod.rs
      window.rs
      geometry.rs
      feed.rs
      popup.rs
      anim.rs
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## Next steps

- [ ] Notification actions and `ActionInvoked`.
- [ ] `image-data` support and full icon themes.
- [ ] Appearance and per-urgency timeout configuration.
- [ ] Notification history/persistence.
