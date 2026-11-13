//! Right panel: the look and the action of the selected key.
//!
//! Edits a copy of the button, then writes it back into the config if it changed
//! (an emptied button is removed).

use std::collections::BTreeMap;

use eframe::egui;

use super::Editor;
use super::widgets::{self, Choices, Pick};
use crate::catalog;
use crate::config::{Button, ColorBy, WhilePress};
use crate::pages::View;

pub fn show(ed: &mut Editor, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    let Some(key) = ed.selected else {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| ui.weak("Select a key to edit it."));
        return;
    };
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
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::CollapsingHeader::new("Look")
            .default_open(true)
            .show(ui, |ui| look(ui, &mut button));
        ui.add_space(8.0);
        egui::CollapsingHeader::new("Action")
            .default_open(true)
            .show(ui, |ui| action(ui, &mut button, &choices, &ed.view));
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

fn look(ui: &mut egui::Ui, button: &mut Button) {
    form(ui, "look", |ui| {
        ui.label("Label");
        widgets::optional_text(ui, &mut button.label, "Text on the key");
        ui.end_row();
        ui.label("Color");
        widgets::optional_color(ui, &mut button.color);
        ui.end_row();
        ui.label("Icon");
        widgets::icon_file(ui, &mut button.icon);
        ui.end_row();
    });

    ui.add_space(6.0);
    ui.strong("Live");
    ui.weak(
        "Values from modules. When one is unavailable (app closed), the fields above are used.",
    );
    form(ui, "live", |ui| {
        ui.label("Text");
        widgets::value_picker(ui, "display", &mut button.display, Pick::Text);
        ui.end_row();
        ui.label("Image");
        widgets::value_picker(ui, "icon_value", &mut button.icon_value, Pick::Image);
        ui.end_row();
        ui.label("Color from");
        color_by(ui, &mut button.color_by);
        ui.end_row();
    });
}

/// Picks a value with known states, then a color for each state.
fn color_by(ui: &mut egui::Ui, color_by: &mut Option<ColorBy>) {
    let mut value = color_by.as_ref().map(|c| c.value.clone());
    ui.vertical(|ui| {
        widgets::value_picker(ui, "color_by", &mut value, Pick::States);
        let Some(value) = value else {
            *color_by = None;
            return;
        };
        let mut colors = match color_by.take() {
            Some(old) if old.value == value => old.colors,
            _ => BTreeMap::new(), // new value: its states are different
        };
        let states = catalog::find(&value.module)
            .and_then(|m| m.value(&value.name))
            .map_or(&[][..], |v| v.states);
        for state in states {
            ui.horizontal(|ui| {
                let mut color = colors.get(*state).cloned();
                widgets::optional_color(ui, &mut color);
                ui.label(format!("when {state}"));
                match color {
                    Some(color) => colors.insert((*state).to_string(), color),
                    None => colors.remove(*state),
                };
            });
        }
        *color_by = Some(ColorBy { value, colors });
    });
}

fn action(ui: &mut egui::Ui, button: &mut Button, choices: &Choices, view: &View) {
    form(ui, "module", |ui| {
        ui.label("Module");
        let before = button.module.clone();
        widgets::module_picker(ui, &mut button.module);
        if button.module != before {
            // Actions belong to one module.
            button.short_press = None;
            button.long_press = None;
            button.while_press = None;
        }
        ui.end_row();
    });
    let Some(module) = button.module.as_deref().and_then(catalog::find) else {
        ui.weak("Choose a module to give this key an action.");
        return;
    };
    ui.weak(module.description);

    let mut hold = button.while_press.is_some();
    ui.horizontal(|ui| {
        ui.radio_value(&mut hold, false, "Press")
            .on_hover_text("An action on short press and/or one on long press");
        ui.radio_value(&mut hold, true, "Hold")
            .on_hover_text("One action when pressed, another when released (push-to-talk)");
    });
    if hold && button.while_press.is_none() {
        button.short_press = None;
        button.long_press = None;
        button.while_press = Some(WhilePress {
            press: String::new(),
            release: String::new(),
        });
    } else if !hold {
        button.while_press = None;
    }

    form(ui, "triggers", |ui| {
        if let Some(while_press) = &mut button.while_press {
            ui.label("When pressed");
            widgets::action_text(ui, "press", module, &mut while_press.press, choices);
            ui.end_row();
            ui.label("When released");
            widgets::action_text(ui, "release", module, &mut while_press.release, choices);
            ui.end_row();
        } else {
            ui.label("Short press");
            widgets::action_picker(ui, "short", module, &mut button.short_press, choices);
            ui.end_row();
            ui.label("Long press");
            widgets::action_picker(ui, "long", module, &mut button.long_press, choices);
            ui.end_row();
        }
    });
    if button.while_press.is_none() {
        ui.weak(if button.long_press.is_some() {
            "The short press runs when the key is released."
        } else {
            "No long press: the short press runs instantly."
        });
    }
    if matches!(view, View::Folder(_)) && module.name == crate::builtin::MODULE_NAME {
        ui.weak("Every folder needs a key with the \"back\" action.");
    }
}

/// Two aligned columns: names on the left, inputs on the right.
fn form(ui: &mut egui::Ui, id: &str, add_rows: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([12.0, 10.0])
        .min_col_width(90.0)
        .show(ui, add_rows);
}
