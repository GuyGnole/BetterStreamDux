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
- One application = one file in `src/modules/`, implementing the `Module` trait. Register it in `Registry::new`.
- New config fields go in `src/config.rs`, with an example in `config.toml`.
- New module: update the module table in README.md.

## Before finishing a task
Run `cargo fmt && cargo clippy && cargo test`; fix all warnings.
