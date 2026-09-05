# Nobody

A Wayland notification daemon. It owns `org.freedesktop.Notifications` on the
session bus and renders notifications top-right in a layer-shell window.

Built with [GPUI](https://github.com/zed-industries/zed) and zbus. No GTK, no
`notify-osd` fork.

## Install

```sh
curl -fsSL https://github.com/nuevosik/Nobody/releases/latest/download/install.sh | sh
```

That drops `nobody` in `~/.local/bin`. Override the destination with
`NOBODY_INSTALL_DIR`.

### Compositor

Stop any daemon already owning the name (e.g. mako), start nobody:

```conf
exec-once = pkill mako; nobody
```

Make sure `~/.local/bin` is on `PATH` for the compositor.

### From source

```sh
git clone https://github.com/nuevosik/Nobody
cd Nobody
cargo run --release
```

Needs a Wayland session with Layer Shell, a D-Bus session bus, Rust, and the
usual GPUI/Linux packages (`libwayland`, `libxkbcommon`, Vulkan).

## Use

- Click, Enter, Space or Escape dismisses a notification.
- Keeps 12 notifications, renders the 5 most recent.
- Timeout: `-1` means server default (5s), `0` never expires; critical
  notifications never auto-expire.
- Spotify notifications show the current album cover (via MPRIS + `curl`),
  cached under `~/.cache/nobody/covers/`.

Debug:

| env | effect |
| --- | --- |
| `PREFERS_REDUCED_MOTION=1` | disables animations |

## Behavior

- `Notify`, `CloseNotification`, `GetCapabilities`, `GetServerInformation`;
  emits `NotificationClosed`.
- `replaces_id` replaces atomically.
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

## Release

```sh
./scripts/release.sh 0.1.1
```

Bumps `Cargo.toml`, tags `v0.1.1`, pushes. GitHub Actions builds
`nobody-<triple>.tar.gz` and attaches it to the release, which is what
`install.sh` downloads.
