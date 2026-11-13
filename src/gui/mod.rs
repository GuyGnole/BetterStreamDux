//! The config editor (`betterstreamdux-gui`). One file per area of the window:
//!   sidebar.rs    pages and folders (left)
//!   grid.rs       the 15 keys, drawn like on the deck (middle)
//!   inspector.rs  look and action of the selected key (right)
//!   widgets.rs    small inputs shared by the panels (color, value and action pickers)
//!   edit.rs       config changes touching several buttons (pages, folders, swaps)
//!
//! The editor only edits the config file: saving it makes the daemon reload it.

mod edit;
mod grid;
mod inspector;
mod sidebar;
mod widgets;

use std::path::PathBuf;

use anyhow::{Result, anyhow};
use eframe::egui;

use crate::builtin::{BRIGHTNESS_MAX, BRIGHTNESS_MIN};
use crate::config::Config;
use crate::pages::View;

const OK_COLOR: egui::Color32 = egui::Color32::from_rgb(90, 200, 120);

/// Opens the editor window on the config file (first argument, or the default location).
pub fn run() -> Result<()> {
    let path = match std::env::args().nth(1) {
        Some(path) => PathBuf::from(path),
        None => Config::default_path()?,
    };
    let config = Config::load_or_create(&path)?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("BetterStreamDux")
            .with_app_id("betterstreamdux")
            .with_inner_size([1180.0, 680.0])
            .with_min_inner_size([980.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "BetterStreamDux",
        options,
        Box::new(|cc| {
            set_style(&cc.egui_ctx);
            Ok(Box::new(Editor::new(path, config)))
        }),
    )
    .map_err(|err| anyhow!("cannot open the window: {err}"))
}

fn set_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = 26.0;
    });
}

pub struct Editor {
    path: PathBuf,
    config: Config,
    /// The config as last saved, to detect unsaved changes.
    saved: String,
    /// The page or folder shown in the middle.
    view: View,
    selected: Option<u8>,
    previews: grid::Previews,
    /// Folder being renamed: (current name, text typed so far).
    renaming: Option<(String, String)>,
    /// Result of the last save or rename, shown in the status bar.
    message: String,
    /// The "unsaved changes" dialog is open.
    confirm_close: bool,
    /// Closing was confirmed: let the window close.
    closing: bool,
}

impl Editor {
    fn new(path: PathBuf, config: Config) -> Self {
        Self {
            path,
            saved: config.to_toml().unwrap_or_default(),
            config,
            view: View::Page(1),
            selected: None,
            previews: grid::Previews::default(),
            renaming: None,
            message: String::new(),
            confirm_close: false,
            closing: false,
        }
    }

    fn is_dirty(&self) -> bool {
        self.config.to_toml().ok().as_deref() != Some(self.saved.as_str())
    }

    fn save(&mut self) {
        match self.config.save(&self.path) {
            Ok(()) => {
                self.saved = self.config.to_toml().unwrap_or_default();
                self.message = "Saved. The daemon reloads it automatically.".into();
            }
            Err(err) => self.message = format!("Save failed: {err:#}"),
        }
    }

    fn show_view(&mut self, view: View) {
        self.view = view;
        self.selected = None;
        self.renaming = None;
    }

    fn clear_key(&mut self, key: u8) {
        let view = &self.view;
        self.config
            .buttons
            .retain(|b| !(b.key == key && b.view() == *view));
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, can_save: bool, dirty: bool) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.heading("BetterStreamDux");
            ui.separator();
            ui.label("Brightness");
            ui.add(
                egui::Slider::new(&mut self.config.brightness, BRIGHTNESS_MIN..=BRIGHTNESS_MAX)
                    .step_by(10.0)
                    .suffix("%"),
            );
            ui.separator();
            ui.label("Long press after");
            ui.add(
                egui::DragValue::new(&mut self.config.long_press_ms)
                    .range(100..=3000)
                    .speed(10)
                    .suffix(" ms"),
            )
            .on_hover_text("How long a key must be held to count as a long press");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let save = ui.add_enabled(can_save, egui::Button::new("Save"));
                if save.on_hover_text("Ctrl+S").clicked() {
                    self.save();
                }
                if dirty {
                    ui.colored_label(ui.visuals().warn_fg_color, "Unsaved changes");
                }
            });
        });
        ui.add_space(4.0);
    }

    fn status_bar(&self, ui: &mut egui::Ui, check: &Result<(), String>) {
        ui.horizontal(|ui| {
            match check {
                Ok(()) => ui.colored_label(OK_COLOR, "Config is valid"),
                Err(err) => {
                    ui.colored_label(ui.visuals().error_fg_color, format!("Problem: {err}"))
                }
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(self.path.display().to_string());
                ui.label(&self.message);
            });
        });
    }

    /// Ctrl+S saves; Delete clears the selected key (unless typing in a text field).
    fn keyboard(&mut self, ctx: &egui::Context, can_save: bool) {
        let save = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
        if ctx.input_mut(|i| i.consume_shortcut(&save)) && can_save {
            self.save();
        }
        if !ctx.egui_wants_keyboard_input()
            && ctx.input(|i| i.key_pressed(egui::Key::Delete))
            && let Some(key) = self.selected
        {
            self.clear_key(key);
        }
    }

    /// Asks before closing the window with unsaved changes.
    fn on_close_request(&mut self, ctx: &egui::Context, dirty: bool) {
        if ctx.input(|i| i.viewport().close_requested()) && dirty && !self.closing {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
    }

    fn close_dialog(&mut self, ctx: &egui::Context, can_save: bool) {
        egui::Modal::new(egui::Id::unique("unsaved-changes-dialog")).show(ctx, |ui| {
            ui.heading("Unsaved changes");
            ui.label("Save them before closing?");
            if !can_save {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "The config has a problem (see the status bar), it cannot be saved as is.",
                );
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(can_save, egui::Button::new("Save and close"))
                    .clicked()
                {
                    self.save();
                    self.close(ctx);
                }
                if ui.button("Close without saving").clicked() {
                    self.close(ctx);
                }
                if ui.button("Cancel").clicked() {
                    self.confirm_close = false;
                }
            });
        });
    }

    fn close(&mut self, ctx: &egui::Context) {
        self.closing = true;
        self.confirm_close = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for Editor {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let check = self.config.check().map_err(|err| format!("{err:#}"));
        let dirty = self.is_dirty();
        let can_save = dirty && check.is_ok();
        let ctx = ui.ctx().clone();
        self.keyboard(&ctx, can_save);
        self.on_close_request(&ctx, dirty);

        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui, can_save, dirty));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui, &check));
        egui::Panel::left("pages")
            .resizable(false)
            .exact_size(200.0)
            .show(ui, |ui| sidebar::show(self, ui));
        egui::Panel::right("key")
            .resizable(false)
            .exact_size(380.0)
            .show(ui, |ui| inspector::show(self, ui));
        egui::CentralPanel::default().show(ui, |ui| grid::show(self, ui));

        if self.confirm_close {
            self.close_dialog(&ctx, can_save);
        }
    }
}
