# Nobody

A Wayland notification daemon. It owns `org.freedesktop.Notifications` on the
session bus and renders notifications top-right in a layer-shell window.

Built with [GPUI](https://github.com/zed-industries/zed) and zbus. No GTK, no
`notify-osd` fork.

## Preview

Spotify notification with the current album cover:

![Spotify notification with album cover](docs/spotify-cover.png)

Notification center with search, Do Not Disturb and Waybar integration:

![Nobody notification center with an empty inbox](docs/notification-center.png)

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

- Click or Escape dismisses a notification. Enter and Space dismiss it too, unless
  an action button is focused, in which case they activate that action.
- Keeps 12 notifications, renders the 5 most recent.
- Timeout: `-1` means server default (5s), `0` never expires; critical
  notifications never auto-expire.
- The daemon advertises the `body` and `actions` capabilities. Each received
  key/label pair (up to 10 actions, 20 key/label items; empty or repeated keys
  are ignored and labels are cut at 64 characters) becomes a button on the
  active popup: `default` shows `Open`, other keys show the received label (or
  `Action` when empty). Click, Enter or Space on the button activates the
  action; the original key is returned opaquely via `ActionInvoked` and the
  popup closes with reason 2, preserving history. Unknown keys, expired,
  removed or pending-close items do not produce an action. The center is a
  read-only history: it offers no actions and does not replay old items. The
  daemon never executes the key as a command, URL or shell argument: the
  client decides navigation. On Hyprland, the `default` action also focuses
  the most recent mapped window for the `desktop-entry` (or the application
  name), switching workspaces when needed. Other compositors may still require
  an activation token.
- Notify accepts `x-dunst-stack-tag` and `x-canonical-private-synchronous` as
  string tags from 1 to 128 characters; the first takes precedence when both
  are valid. Within the same application, an active tag replaces the previous
  notification, preserving its ID and updating content, timeout and arrival.
- Notify accepts the `value` hint only as a D-Bus `int32` between 0 and 100;
  when valid, it shows a discreet progress bar in the popup and includes the
  percentage in the accessible description. `list --json` and `history --json`
  export `progress` as an integer or `null`.
- Spotify notifications show the current album cover (via MPRIS + `curl`),
  cached under `~/.cache/nobody/covers/`.

### Center and Do Not Disturb

```sh
nobody
nobody center open | close | toggle
nobody dismiss all
nobody dismiss 42
nobody dismiss app "Spotify"
nobody dnd on | off | toggle
nobody dnd status
nobody list --json
nobody history --json
```

- `nobody list --json` and `nobody history --json` export single-line JSON
  snapshots of the active queue and the history in the format
  `{"notifications":[...]}` (`seq` is included in history entries).

- Optional configuration lives at `$XDG_CONFIG_HOME/nobody/config` when
  `XDG_CONFIG_HOME` is absolute; otherwise at `$HOME/.config/nobody/config`.
  Defaults are `default-timeout=5000`, `max-visible=5` and `anchor=top-right`.
  The file accepts those three keys, whole-line comments and repeated values
  with the last valid one winning; changes require restarting the daemon.

- Center commands open (`nobody center open`), close (`nobody center close`)
  or toggle (`nobody center toggle`) the center idempotently and predictably;
  all of them preserve active notifications, history and quiet preferences.
- The center lists the last 100 notifications of the session (most recent
  first), with search by application, title or body, a clear button and a
  Do Not Disturb control. Escape closes; expiring or dismissing a popup
  preserves the record; clearing preserves the active queue.
- Effective quiet = manual OR fullscreen. The fullscreen detector never
  overrides the manual preference; when quiet ends, only still-active
  notifications appear (history is not replayed).
- `nobody dismiss all` dismisses every active notification at once,
  preserving history. It can be bound to a desktop shortcut.
- `nobody dismiss 42` dismisses a notification by the decimal ID shown in the
  listing; `nobody dismiss app "Spotify"` dismisses every notification whose
  application is exactly the name shown in the listing (case and spaces are
  part of the comparison). Missing IDs and applications with no match finish
  successfully with no effect. Selection by application uses the snapshot at
  call time; notifications that arrive later wait for a next operation.
- Control commands return 0 when completed, 1 when the daemon/D-Bus fails and
  2 for invalid syntax; these commands do not start the GUI. Dismissing
  preserves history, center state and Do Not Disturb.
- Limitation of this delivery: in-memory history only — no database or
  persistence. JSON export shows only current snapshots and the history kept
  during the session; there is no automatic application opening. Restarting
  the daemon starts with empty history.

### Brave / YouTube

Nobody receives what the browser hands to the system via
`org.freedesktop.Notifications`. It does not read YouTube's internal bell,
does not recover old account alerts and does not replace browser
permissions/push. Chromium uses the freedesktop backend when the service
advertises `body` and `actions`; a sandboxed install (Flatpak/Snap) may use
the portal instead of the direct bus.

To validate the integration:

1. Check that `GetCapabilities` advertises `body` and `actions`.
2. In Brave, allow notifications and use the [official push test
   page](https://support.brave.com/hc/en-us/articles/360058972091-Push-Notification-Test).
   `notify-send` alone does not validate the browser.
3. In YouTube, enable receiving notifications in this browser and the
   account/channel preferences as described in the [official
   help](https://support.google.com/youtube/answer/9334950?hl=en); wait for a
   legitimate notification. The generic push test does not prove the account
   is configured.
4. If nothing arrives, check site permissions and the Google services push
   option in the installed version. Brave may need a restart to renegotiate
   capabilities; save your work first.
5. Clicking Open returns the key to the browser via `ActionInvoked`; on
   Hyprland, Nobody also follows to the workspace of the matching window.

### Waybar

With Nobody installed in `~/.local/bin`, run from this checkout:

```sh
install -Dm644 assets/nobody-badge.png "$HOME/.local/share/nobody/nobody-badge.png"
```

Add `"image#nobody"` to `modules-right` in your Waybar configuration, then
add this module at the top level:

```json
"image#nobody": {
    "exec": "printf '%s\\n' \"$HOME/.local/share/nobody/nobody-badge.png\" 'Notification center'",
    "size": 16,
    "interval": 3600,
    "on-click": "\"$HOME/.local/bin/nobody\" center toggle",
    "tooltip": true
}
```

Add to Waybar's `style.css`:

```css
#image.nobody { padding: 0 8px; }
#image.nobody:hover { background: rgba(255, 255, 255, 0.10); }
```

Restart Waybar to load the module. Clicking the icon toggles the central.
If Nobody was installed elsewhere, adjust `on-click` to that binary.
This setup is optional; the installer does not modify your Waybar configuration.

Debug:

| env | effect |
| --- | --- |
| `PREFERS_REDUCED_MOTION=1` | disables animations |

## Behavior

- `Notify`, `CloseNotification`, `GetCapabilities`, `GetServerInformation`;
  emits `NotificationClosed` and `ActionInvoked`.
- `replaces_id` replaces atomically.
- `x-dunst-stack-tag` and `x-canonical-private-synchronous` replace an active
  notification from the same application when `replaces_id` is zero; invalid
  tags are ignored and explicit replacements by ID take precedence.
- Icon path/name plus `desktop-entry` lookup scoped to known locations;
  markup is stripped. No `image-data`, persistence, body-markup, inline
  replies or activation tokens; cross-workspace focus uses Hyprland IPC.

## Architecture

Four layers, one rule: `presentation` and `infrastructure` never import each
other — they talk through the `domain` queue. Details in
`docs/architecture.md`.

```
domain <- application <- infrastructure / presentation
```

## Development

`scripts/check.sh` runs exactly what CI runs. `.git/hooks/` is not tracked, so
wire it yourself once per clone to stop a red push on this machine:

```bash
ln -sf ../../scripts/check.sh .git/hooks/pre-push
```

It runs, in order:

```bash
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
! rg -n 'infrastructure::|crate::(daemon|icons)' src/presentation
! rg -n 'presentation::' src/infrastructure
cargo test --all-targets
```

The two `rg` guards enforce the layer rule: `presentation` and
`infrastructure` never import each other.

## Release

```sh
./scripts/release.sh 0.1.1
```

Bumps `Cargo.toml`, tags `v0.1.1`, pushes. GitHub Actions builds
`nobody-<triple>.tar.gz` and attaches it to the release, which is what
`install.sh` downloads.
