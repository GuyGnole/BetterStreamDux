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

Edit `config.toml`. Each `[[button]]` binds a key (0-14) to a module action.

| Module    | Actions                                       | Status          |
|-----------|-----------------------------------------------|-----------------|
| `spotify` | `play_pause`, `next`, `previous`, `stop`      | working         |
| `discord` | `toggle_mute`, `toggle_deafen`                | not implemented |
| `shell`   | any command line                              | working         |

## Project layout

```
src/
  main.rs          entry point: load config, connect, event loop
  config.rs        config.toml format
  deck.rs          Stream Deck hardware (connect, draw keys, read events)
  modules/
    mod.rs         Module trait + registry
    spotify.rs     one file per application
    discord.rs
    shell.rs
config.toml        your key bindings
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
