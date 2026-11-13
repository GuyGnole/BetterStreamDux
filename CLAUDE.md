# CLAUDE.md

Rust app controlling an Elgato Stream Deck MK.2 on Fedora (Wayland). See README.md for layout.

## Rules
- Keep it simple: the owner reviews every line. Small functions, short files, no clever abstractions.
- Comment the *why* in one line where it is not obvious. Every file starts with a `//!` doc saying what it does.
- Synchronous code only (no async/tokio) unless the owner agrees.
- Errors: `anyhow::Result`, add `.context(...)` on I/O and device calls. No `unwrap()` outside tests.
- A failing module action must be logged, never crash the program.
- Ask before adding a dependency; prefer pure-Rust crates (no extra system libs).

## Where things go
- Hardware code only in `src/deck.rs`.
- Trigger logic (short/long/while press) only in `src/handler.rs`. It takes `now: Instant` as a parameter so tests never sleep.
- Drawing flow: `handler.screen()` -> `screen::build` (pure, what to show) -> `deck.draw` (diffs, calls `render`). Never draw from elsewhere.
- Built-in deck actions (pages, folders, brightness): parse in `src/builtin.rs`, execute in `handler.rs`, validate in `config.rs`.
- Two programs, one library: `src/main.rs` (daemon, owns the deck) and `src/bin/gui.rs` (editor, never touches the hardware). Shared code lives in the lib (`src/lib.rs`).
- One application = one file in `src/modules/`, implementing the `Module` trait, with a `pub const INFO: ModuleInfo` (actions + values). Register it in `Registry::start_all` and `INFOS`. Keep `INFO` and the module's parsing in sync (there is a test per module).
- GUI: one file per panel in `src/gui/`. Config changes touching several buttons go in `src/gui/edit.rs` as plain tested functions, not in UI code. Widgets only write a field when the user changes it.
- Modules that change over time implement `Module::tick()` (called every 50 ms, must be instant); module settings come through `Module::configure(&Config)`. Time-based logic takes `now: Instant` so tests never sleep (see `modules/tools/`).
- `Module::value()` is called every 50 ms tick: never block in it. Slow reads (D-Bus, sockets, HTTP) go in the module's background thread, shared through `Arc<Mutex<..>>`.
- Document each module's actions and values in its `//!` header and in the README table.
- New config fields go in `src/config/button.rs` (with `skip_serializing_if` for options), an editor input in `src/gui/inspector.rs`, and a line in the README field table. A format change needs a conversion in `src/config/migrate.rs` (users have existing configs).
- Actions are always `module:action` strings; a trigger is a list. Presets are config-format TOML in each module's `INFO.presets`.
- Icons: names in `src/icons.rs` (a test checks every glyph exists in the bundled font).
- New module: update the module table in README.md.

## Before finishing a task
Run `cargo fmt && cargo clippy && cargo test`; fix all warnings.
