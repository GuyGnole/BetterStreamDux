//! Shared code for the two programs:
//!   - `betterstreamdux`     (src/main.rs): the daemon that drives the Stream Deck
//!   - `betterstreamdux-gui` (src/bin/gui.rs): the config editor

pub mod builtin;
pub mod catalog;
pub mod config;
pub mod deck;
pub mod gui;
pub mod handler;
pub mod modules;
pub mod pages;
pub mod paths;
pub mod render;
pub mod screen;
