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
- One application = one file in `src/modules/`, implementing the `Module` trait. Register it in `Registry::new`.
- `Module::value()` is called every 50 ms tick: never block in it. Slow reads (D-Bus, sockets, HTTP) go in the module's background thread, shared through `Arc<Mutex<..>>`.
- Document each module's actions and values in its `//!` header and in the README table.
- New config fields go in `src/config.rs`, with an example in `config.toml`.
- New module: update the module table in README.md.

## Before finishing a task
Run `cargo fmt && cargo clippy && cargo test`; fix all warnings.
