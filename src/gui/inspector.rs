//! Right panel: the selected key's look, how its look changes with a live value, and
//! its actions.
//!
//! Edits a copy of the button, then writes it back into the config if it changed
//! (an emptied button is removed).

use eframe::egui;

use super::Editor;
use super::widgets::{self, Choices, Pick};
use crate::catalog;
use crate::config::{Button, Look, LookBy, WhilePress};
use crate::pages::View;

const FIELD: f32 = 200.0;

pub fn show(ed: &mut Editor, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    let Some(key) = ed.selected else {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            ui.weak("Select a key to edit it,");
            ui.weak("or drag a button from the library onto a key.");
        });
        return;
    };
    if ed.config.shows_shared(&ed.view, key) {
        inherited_key(ed, ui, key);
        return;
    }
    let original = ed.config.button(&ed.view, key).cloned();
    let mut button = original
        .clone()
        .unwrap_or_else(|| Button::new(&ed.view, key));
    let choices = Choices {
        page_count: ed.config.page_count(),
        folders: ed.config.folders(),
    };

    ui.horizontal(|ui| {
        ui.heading(format!("Key {key}"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let clear = ui.add_enabled(!button.is_empty(), egui::Button::new("Clear"));
            if clear.on_hover_text("Empty this key (Delete)").clicked() {
                button = Button::new(&ed.view, key);
            }
        });
    });
    if matches!(ed.view, View::Page(_)) && ed.config.button(&View::Shared, key).is_some() {
        ui.weak(
            "Replaces the key from \"All pages\" on this page. Clear it to show that key again.",
        );
    }
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::CollapsingHeader::new("Look")
            .default_open(true)
            .show(ui, |ui| look(ui, &mut button));
        egui::CollapsingHeader::new("Changes with a live value")
            .default_open(button.look_by.is_some())
            .show(ui, |ui| look_by(ui, &mut button.look_by));
        egui::CollapsingHeader::new("Actions")
            .default_open(true)
            .show(ui, |ui| actions(ui, &mut button, &choices, &ed.view));
    });

    if Some(&button) != original.as_ref() {
        let view = &ed.view;
        ed.config
            .buttons
            .retain(|b| !(b.key == key && b.view() == *view));
        if !button.is_empty() {
            ed.config.buttons.push(button);
        }
    }
}

/// A page key showing the key from "All pages": edit that one, or replace it here.
fn inherited_key(ed: &mut Editor, ui: &mut egui::Ui, key: u8) {
    ui.heading(format!("Key {key}"));
    ui.separator();
    ui.label("This key comes from \"All pages\": it is the same on every page.");
    ui.add_space(8.0);
    if ui.button("Edit it for all pages").clicked() {
        ed.view = View::Shared; // keep the key selected
    }
    if ui.button("Replace it on this page only").clicked()
        && let (View::Page(page), Some(shared)) = (&ed.view, ed.config.button(&View::Shared, key))
    {
        // Start from a copy, so only the differences need changing.
        let copy = Button {
            shared: false,
            page: Some(*page),
            ..shared.clone()
        };
        ed.config.buttons.push(copy);
    }
}

fn look(ui: &mut egui::Ui, button: &mut Button) {
    form(ui, "look", |ui| {
        ui.label("Icon");
        widgets::symbol_picker(ui, "symbol", &mut button.symbol);
        ui.end_row();
        ui.label("Icon color").on_hover_text("Default: white");
        widgets::optional_color(ui, &mut button.symbol_color);
        ui.end_row();
        ui.label("Label");
        widgets::optional_text(ui, &mut button.label, "Text on the key", FIELD);
        ui.end_row();
        ui.label("Color");
        widgets::optional_color(ui, &mut button.color);
        ui.end_row();
        ui.label("Image");
        widgets::icon_file(ui, &mut button.icon);
        ui.end_row();
        ui.label("Live text")
            .on_hover_text("Replaces the label while the value is available");
        widgets::value_picker(ui, "display", &mut button.display, Pick::Text);
        ui.end_row();
        ui.label("Live image")
            .on_hover_text("Replaces the image while the value is available");
        widgets::value_picker(ui, "icon_value", &mut button.icon_value, Pick::Image);
        ui.end_row();
    });
}

/// Picks a value with known states, then what each state changes.
fn look_by(ui: &mut egui::Ui, look_by: &mut Option<LookBy>) {
    let mut value = look_by.as_ref().map(|l| l.value.clone());
    widgets::value_picker(ui, "look_by", &mut value, Pick::States);
    let Some(value) = value else {
        *look_by = None;
        return;
    };
    let mut states = match look_by.take() {
        Some(old) if old.value == value => old.states,
        _ => std::collections::BTreeMap::default(), // another value: other states
    };
    let names = catalog::find(&value.module)
        .and_then(|m| m.value(&value.name))
        .map_or(&[][..], |v| v.states);
    ui.weak(
        "For each state: color, icon, icon color, label. Empty fields keep the key's own look.",
    );
    egui::Grid::new("states")
        .num_columns(5)
        .spacing([8.0, 8.0])
        .show(ui, |ui| {
            for name in names {
                let mut state = states.remove(*name).unwrap_or_default();
                ui.label(*name);
                widgets::optional_color(ui, &mut state.color);
                widgets::symbol_picker(ui, ("state symbol", *name), &mut state.symbol);
                // Next to the icon, so it reads as the icon's color.
                widgets::optional_color(ui, &mut state.symbol_color);
                widgets::optional_text(ui, &mut state.label, "Label", 70.0);
                ui.end_row();
                if state != Look::default() {
                    states.insert((*name).to_string(), state);
                }
            }
        });
    *look_by = Some(LookBy { value, states });
}

fn actions(ui: &mut egui::Ui, button: &mut Button, choices: &Choices, view: &View) {
    let mut hold = button.while_press.is_some();
    ui.horizontal(|ui| {
        ui.radio_value(&mut hold, false, "Press")
            .on_hover_text("Actions on short press and/or on long press");
        ui.radio_value(&mut hold, true, "Hold")
            .on_hover_text("Actions when pressed, others when released (push-to-talk)");
    });
    if hold && button.while_press.is_none() {
        // Keep what was set: the short press becomes the "pressed" actions.
        button.while_press = Some(WhilePress {
            press: std::mem::take(&mut button.short_press),
            release: Vec::new(),
        });
        button.long_press.clear();
    } else if !hold && let Some(held) = button.while_press.take() {
        button.short_press = held.press;
    }

    ui.weak("Each trigger runs its actions in order, from any modules.");
    if let Some(held) = &mut button.while_press {
        ui.strong("When pressed");
        widgets::action_list(ui, "press", &mut held.press, choices);
        ui.strong("When released");
        widgets::action_list(ui, "release", &mut held.release, choices);
    } else {
        ui.strong("Short press");
        widgets::action_list(ui, "short", &mut button.short_press, choices);
        ui.strong("Long press");
        widgets::action_list(ui, "long", &mut button.long_press, choices);
        ui.weak(if button.long_press.is_empty() {
            "No long press: the short press runs instantly."
        } else {
            "The short press runs when the key is released."
        });
    }
    if matches!(view, View::Folder(_)) {
        ui.weak("Every folder needs a key with the action deck: back.");
    }
}

/// Two aligned columns: names on the left, inputs on the right.
fn form(ui: &mut egui::Ui, id: &str, add_rows: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([12.0, 10.0])
        .min_col_width(80.0)
        .show(ui, add_rows);
}
