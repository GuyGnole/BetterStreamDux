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

- Left: "All pages" (keys shared by every page, like navigation), the main pages and
  the folders. Right-click a page or folder to rename or delete it.
- Middle: the 15 keys, drawn exactly like on the deck (live values show sample text).
  Click a key to edit it, drag a key onto another one to swap them, Delete empties it.
  Keys coming from "All pages" are marked "shared": edit them for all pages, or replace
  them on one page only.
- Below the keys, the library: ready-made buttons from every module (with icons, colors,
  live looks and actions already set). Drag one onto a key to place it, then adjust it.
- Right: the selected key's look (icon, label, color, image, live text/image), how it
  changes with a live value (per state: color, icon, label), and its actions: each
  trigger runs a list of actions, from any modules, in order.
- The status bar shows config problems as you edit; Save is enabled when the config is
  valid. Closing with unsaved changes asks first.
- Right-click a key for Copy / Cut / Paste / Clear. A copied key is a `[[button]]` block
  of config text, so it can also be pasted into the config file (or copied from it).

| Shortcut | Does |
|----------|------|
| Ctrl+S | save |
| Ctrl+Z, Ctrl+Shift+Z (or Ctrl+Y) | undo, redo |
| Ctrl+C, Ctrl+X, Ctrl+V | copy, cut, paste the selected key |
| Delete, Backspace | empty the selected key |
| Arrow keys, Escape | move the selection, unselect |

## Configure by hand

The config is TOML. Each `[[button]]` is one key on one page or folder:

| Field | Meaning |
|-------|---------|
| `key` | 0-14, left to right, top to bottom |
| `page` | main page 1-5 (default 1) |
| `folder` | folder name, instead of `page` |
| `shared` | `true`: shown on every main page (not in folders), instead of `page`. A page's own key at the same place wins |
| `symbol` | icon: a name from `src/icons.rs` (`play`, `mic`, `timer`, `folder`...) or any emoji |
| `label` | fixed text (under the icon if there is one) |
| `color` | background, `"#RRGGBB"` |
| `icon` | image file (png/jpg) as background, replaces the color |
| `display` | live text, e.g. `"spotify:title"`, replaces the label |
| `icon_value` | live background image, e.g. `"spotify:cover"`, replaces `icon` |
| `look_by` | look depending on a live value, per state: `{ value = "spotify:status", states = { playing = { symbol = "pause", color = "#1DB954" } } }` (each state may set `color`, `symbol`, `label`) |
| `short_press`, `long_press`, `while_press` | the actions, see Triggers |

**Actions** are written `"module:action"`, e.g. `"spotify:next"` or `"deck:go_to_page 2"`.
A trigger takes one action or a list, run in order, from any modules:
`short_press = ["spotify:play_pause", "deck:open_folder Music"]`.

Older configs (`module = "spotify"` with `short_press = "next"`, and `color_by`) are
converted automatically when loaded; the editor saves the new format.

Top-level fields: `brightness` (10-100), `long_press_ms` (default 500), `pages` (number of
main pages, optional).

Pomodoro durations (also in the editor, under "Pomodoro settings"):

```toml
[pomodoro]
work_minutes = 25
break_minutes = 5
long_break_minutes = 15
rounds = 4          # a long break after this many work rounds
```

**Pages and folders**: up to 5 main pages (`page = 1..5`, default 1). A folder
(`folder = "name"`) is one extra page opened from a main page with the `open_folder name`
deck action. `back` returns to that page. Folders cannot open folders, and every folder
needs a `back` key.

**Live look**: modules publish values written `module:value` (e.g. `spotify:title`).
`display` shows one as text, `icon_value` as an image, `look_by` changes the look for
some states. When a value is unavailable (e.g. Spotify closed), the fixed fields are used.

**Triggers**:

| Trigger       | Fires when                                         |
|---------------|----------------------------------------------------|
| `short_press` | key released before `long_press_ms` (default 500)  |
| `long_press`  | key held for `long_press_ms`, without waiting for release |
| `while_press` | `press` actions on down, `release` actions on up (push-to-talk). Cannot be mixed with the other two |

A key with only `short_press` fires instantly on press. Config mistakes (duplicate key,
mixed triggers, missing folder, typos...) are reported at startup.

| Module    | Actions | Values | Status |
|-----------|---------|--------|--------|
| `spotify` | `play_pause`, `play`, `pause`, `next`, `previous`, `stop`, `volume_up`, `volume_down`, `seek_forward`, `seek_back` | `status`, `title`, `artist`, `album`, `volume`, `cover` | working (MPRIS, no account needed) |
| `discord` | `mute`, `unmute`, `toggle_mute`, `deafen`, `undeafen`, `toggle_deafen`, `join_channel ID`, `leave_channel` | `status`, `muted`, `deafened`, `channel` | working (needs the setup below) |
| `shell`   | any command line | | working |
| `timer`   | `start N` (minutes), `pause_resume`, `add_minute`, `cancel` | `remaining`, `status` | built-in, notifies when done |
| `stopwatch` | `start_stop`, `reset` | `elapsed`, `status` | built-in |
| `pomodoro` | `start_pause`, `skip`, `reset` | `remaining`, `phase`, `status`, `done` | built-in, notifies at each phase |
| `counter` | `increment`, `decrement`, `reset` | `count` | built-in, kept across restarts |
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
  config/          config format: mod.rs (load/save, checks), button.rs (one key),
                   migrate.rs (converts older formats)
  catalog.rs       what each module offers (actions, values, presets), for checks and the editor
  icons.rs         named icons, from the bundled Noto Emoji font
  paths.rs         per-user directories (config, state, cache)
  handler.rs       key down/up -> short/long/while press -> deck or module action
  builtin.rs       the built-in "deck" actions (pages, folders, brightness)
  pages.rs         which page/folder is shown, navigation
  screen.rs        what each key should show right now (pure)
  render.rs        key content -> 72x72 image with text
  deck.rs          Stream Deck hardware (connect, redraw changed keys, read events)
  gui/             the editor: mod.rs (window), sidebar.rs, grid.rs, library.rs,
                   inspector.rs, widgets.rs, edit.rs (page/folder/swap/preset changes),
                   history.rs (undo/redo)
  modules/
    mod.rs         Module trait + registry
    spotify.rs     one file per application
    discord/       mod.rs (actions, values), ipc.rs (local socket), auth.rs (login)
    shell.rs
    tools/         built-in tools: timer, stopwatch, pomodoro, counter (+ shared countdown,
                   notifications)
config.example.toml  example key bindings, copied on first run
assets/fonts/      bundled fonts (DejaVu Sans Bold for text, Noto Emoji for icons; licenses next to them)
udev/              permission rule for the device
```

## Add a module

1. Create `src/modules/<app>.rs` with a struct implementing `Module` (copy `spotify.rs`).
   `run()` performs actions; `value()` returns live values and must be instant, so read
   the application's state in a background thread (see `spotify.rs`).
2. Declare its actions, values and presets in a `pub const INFO: ModuleInfo` in the same
   file: the config check, the editor's dropdowns and its library use it. A preset is a
   `[[button]]` body in config format (see the existing modules); a test checks each one.
3. In `src/modules/mod.rs`: add `mod <app>;`, register it in `Registry::start_all` and its
   `INFO` in `INFOS`.
4. Add it to the module table above.

## Checks

```sh
cargo fmt && cargo clippy && cargo test
```
