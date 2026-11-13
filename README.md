# BetterStreamDux

Control an Elgato Stream Deck MK.2 on Linux (Fedora), with one module per application.

## Setup

```sh
# Build dependency (libudev, used by hidapi)
sudo dnf install systemd-devel

# Allow your user to access the deck (once)
sudo cp udev/50-streamdeck.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```

## Run

Two programs share the config file `~/.config/betterstreamdux/config.toml` (created from
`config.example.toml` on first run):

```sh
cargo run                                # the daemon: drives the Stream Deck
cargo run --bin betterstreamdux-gui      # the editor window
cargo run -- path/to/config.toml         # either one accepts another config file
RUST_LOG=debug cargo run                 # more logs
```

Saving in the editor (or editing the file by hand) reloads the running daemon within a
second. An invalid file is refused and the current config keeps running.

## Editor

- Left: main pages and folders. Right-click one to rename or delete it.
- Middle: the 15 keys, drawn exactly like on the deck (live values show sample text).
  Click a key to edit it, drag a key onto another one to swap them, Delete empties it.
- Right: the selected key's look (label, color, icon, live values) and action (module,
  press or hold, and the actions, chosen from lists).
- The status bar shows config problems as you edit; Save (Ctrl+S) is enabled when the
  config is valid. Closing with unsaved changes asks first.

## Configure by hand

The config is TOML. Each `[[button]]` is one key on one page or folder:

| Field | Meaning |
|-------|---------|
| `key` | 0-14, left to right, top to bottom |
| `page` | main page 1-5 (default 1) |
| `folder` | folder name, instead of `page` |
| `label` | fixed text |
| `color` | background, `"#RRGGBB"` |
| `icon` | image file (png/jpg), replaces the color |
| `display` | live text, e.g. `"spotify:title"`, replaces the label |
| `icon_value` | live image, e.g. `"spotify:cover"`, replaces the icon |
| `color_by` | color from a live value: `{ value = "spotify:status", colors = { playing = "#1DB954" } }` |
| `module` | `deck`, `spotify`, `discord` or `shell` (needed by the triggers) |
| `short_press`, `long_press`, `while_press` | the actions, see Triggers |

Top-level fields: `brightness` (10-100), `long_press_ms` (default 500), `pages` (number of
main pages, optional).

**Pages and folders**: up to 5 main pages (`page = 1..5`, default 1). A folder
(`folder = "name"`) is one extra page opened from a main page with the `open_folder name`
deck action. `back` returns to that page. Folders cannot open folders, and every folder
needs a `back` key.

**Look**: `color`, `icon`, `label` (fixed text).

**Live look**: modules publish values written `module:value` (e.g. `spotify:title`).
`display` shows one as text, `icon_value` as an image, `color_by` picks the color from it.
When a value is unavailable (e.g. Spotify closed), the fixed field is used instead.

**Triggers**:

| Trigger       | Fires when                                         |
|---------------|----------------------------------------------------|
| `short_press` | key released before `long_press_ms` (default 500)  |
| `long_press`  | key held for `long_press_ms`, without waiting for release |
| `while_press` | `press` action on down, `release` action on up (push-to-talk). Cannot be mixed with the other two |

A key with only `short_press` fires instantly on press. Config mistakes (duplicate key,
mixed triggers, missing folder, typos...) are reported at startup.

| Module    | Actions | Values | Status |
|-----------|---------|--------|--------|
| `spotify` | `play_pause`, `play`, `pause`, `next`, `previous`, `stop`, `volume_up`, `volume_down`, `seek_forward`, `seek_back` | `status`, `title`, `artist`, `album`, `volume`, `cover` | working (MPRIS, no account needed) |
| `discord` | `mute`, `unmute`, `toggle_mute`, `deafen`, `undeafen`, `toggle_deafen`, `join_channel ID`, `leave_channel` | `status`, `muted`, `deafened`, `channel` | working (needs the setup below) |
| `shell`   | any command line | | working |
| `deck`    | `next_page`, `previous_page`, `go_to_page N`, `open_folder NAME`, `back`, `brightness_up`, `brightness_down` | `clock`, `page_number`, `brightness` | built-in |

Spotify shuffle/repeat are not supported: the Linux app ignores them over MPRIS.

## Discord setup (once)

The Discord module talks to the Discord app on your computer (official local RPC).
Discord requires your own (free) Discord application for this:

1. Go to <https://discord.com/developers/applications>, click **New Application**,
   name it e.g. `BetterStreamDux`.
2. Open **OAuth2** in the left menu:
   - copy the **Client ID**;
   - click **Reset Secret** and copy the **Client Secret**;
   - under **Redirects**, add `http://localhost` and **Save Changes**.
3. Create `~/.config/betterstreamdux/discord.toml` (keep it private, never commit it):
   ```toml
   client_id = "your client id"
   client_secret = "your client secret"
   ```
   then `chmod 600 ~/.config/betterstreamdux/discord.toml`.
4. Start BetterStreamDux with Discord open. Discord shows an approval popup: click
   **Authorize**. The login is saved in `~/.local/state/betterstreamdux/discord-token.json`
   and renewed automatically; delete that file to force a new approval.

To get a channel ID for `join_channel ID`: Discord settings → Advanced → enable
**Developer Mode**, then right-click a voice channel → **Copy Channel ID**.

## Project layout

```
src/
  main.rs          the daemon: load config, connect, event loop, reload on change
  bin/gui.rs       the editor's entry point (code in gui/)
  lib.rs           shared code used by both programs
  config.rs        config format, load/save, validation
  catalog.rs       what each module offers (actions, values), for checks and the editor
  paths.rs         per-user directories (config, state, cache)
  handler.rs       key down/up -> short/long/while press -> deck or module action
  builtin.rs       the built-in "deck" actions (pages, folders, brightness)
  pages.rs         which page/folder is shown, navigation
  screen.rs        what each key should show right now (pure)
  render.rs        key content -> 72x72 image with text
  deck.rs          Stream Deck hardware (connect, redraw changed keys, read events)
  gui/             the editor: mod.rs (window), sidebar.rs, grid.rs, inspector.rs,
                   widgets.rs, edit.rs (page/folder/swap changes)
  modules/
    mod.rs         Module trait + registry
    spotify.rs     one file per application
    discord/       mod.rs (actions, values), ipc.rs (local socket), auth.rs (login)
    shell.rs
config.example.toml  example key bindings, copied on first run
assets/fonts/      bundled font (DejaVu Sans Bold, license next to it)
udev/              permission rule for the device
```

## Add a module

1. Create `src/modules/<app>.rs` with a struct implementing `Module` (copy `spotify.rs`).
   `run()` performs actions; `value()` returns live values and must be instant, so read
   the application's state in a background thread (see `spotify.rs`).
2. Declare its actions and values in a `pub const INFO: ModuleInfo` in the same file:
   the config check and the editor's dropdowns use it.
3. In `src/modules/mod.rs`: add `mod <app>;`, register it in `Registry::start_all` and its
   `INFO` in `INFOS`.
4. Add it to the module table above.

## Checks

```sh
cargo fmt && cargo clippy && cargo test
```
