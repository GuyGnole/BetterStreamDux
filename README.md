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

```sh
cargo run                         # uses ./config.toml
cargo run -- path/to/config.toml  # or another config file
RUST_LOG=debug cargo run          # more logs
```

## Configure

Edit `config.toml` (the comments at its top list every field). Each `[[button]]` is one key
on one page or folder.

**Pages and folders**: up to 5 main pages (`page = 1..5`, default 1). A folder
(`folder = "name"`) is one extra page opened from a main page with the `open_folder name`
deck action. `back` returns to that page. Folders cannot open folders, and every folder
needs a `back` key.

**Look**: `color`, `icon`, `label` (fixed text), or `display` (live text: `clock`,
`page_number`, `brightness`).

**Triggers**:

| Trigger       | Fires when                                         |
|---------------|----------------------------------------------------|
| `short_press` | key released before `long_press_ms` (default 500)  |
| `long_press`  | key held for `long_press_ms`, without waiting for release |
| `while_press` | `press` action on down, `release` action on up (push-to-talk). Cannot be mixed with the other two |

A key with only `short_press` fires instantly on press. Config mistakes (duplicate key,
mixed triggers, missing folder, typos...) are reported at startup.

| Module    | Actions                                       | Status          |
|-----------|-----------------------------------------------|-----------------|
| `spotify` | `play_pause`, `next`, `previous`, `stop`      | working         |
| `discord` | `mute`, `unmute`, `toggle_mute`, `toggle_deafen` | not implemented |
| `shell`   | any command line                              | working         |
| `deck`    | `next_page`, `previous_page`, `go_to_page N`, `open_folder NAME`, `back`, `brightness_up`, `brightness_down` | built-in |

## Project layout

```
src/
  main.rs          entry point: load config, connect, event loop
  config.rs        config.toml format + validation
  handler.rs       key down/up -> short/long/while press -> deck or module action
  builtin.rs       the built-in "deck" actions (pages, folders, brightness)
  pages.rs         which page/folder is shown, navigation
  screen.rs        what each key should show right now (pure)
  render.rs        key content -> 72x72 image with text
  deck.rs          Stream Deck hardware (connect, redraw changed keys, read events)
  modules/
    mod.rs         Module trait + registry
    spotify.rs     one file per application
    discord.rs
    shell.rs
config.toml        your key bindings
assets/fonts/      bundled font (DejaVu Sans Bold, license next to it)
udev/              permission rule for the device
```

## Add a module

1. Create `src/modules/<app>.rs` with a struct implementing `Module` (copy `spotify.rs`).
2. Add `mod <app>;` in `src/modules/mod.rs` and register it in `Registry::new`.
3. Bind it in `config.toml`.

## Checks

```sh
cargo fmt && cargo clippy && cargo test
```
