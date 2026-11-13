//! Small inputs shared by the panels. Each edits one config field in place and only
//! writes it when the user changes something (so viewing a key never modifies it).

use std::path::Path;

use eframe::egui;

use crate::catalog::{self, Actions, Arg, ModuleInfo, ValueKind, join_action, split_action};
use crate::config::{ValueRef, format_color, parse_color};

/// Width of text fields and dropdowns.
const WIDTH: f32 = 200.0;
/// Color shown when a color is first switched on.
const DEFAULT_COLOR: [u8; 3] = [0x44, 0x44, 0x44];

/// What the action arguments can be chosen from.
pub struct Choices {
    pub page_count: u8,
    pub folders: Vec<String>,
}

/// Which live values a picker offers.
#[derive(Clone, Copy)]
pub enum Pick {
    Text,
    Image,
    /// Text values with known states, for `color_by`.
    States,
}

impl Pick {
    fn accepts(self, value: &catalog::ValueInfo) -> bool {
        match self {
            Self::Text => value.kind == ValueKind::Text,
            Self::Image => value.kind == ValueKind::Image,
            Self::States => !value.states.is_empty(),
        }
    }
}

/// Text field for an optional text: empty means none.
pub fn optional_text(ui: &mut egui::Ui, value: &mut Option<String>, hint: &str) {
    let mut text = value.clone().unwrap_or_default();
    let field = egui::TextEdit::singleline(&mut text)
        .hint_text(hint)
        .desired_width(WIDTH);
    if ui.add(field).changed() {
        *value = (!text.is_empty()).then_some(text);
    }
}

/// Checkbox to turn a color on, and its color picker.
pub fn optional_color(ui: &mut egui::Ui, value: &mut Option<String>) {
    ui.horizontal(|ui| {
        let mut on = value.is_some();
        let mut rgb = value
            .as_deref()
            .and_then(|c| parse_color(c).ok())
            .unwrap_or(DEFAULT_COLOR);
        let mut changed = ui.checkbox(&mut on, "").changed();
        if on {
            changed |= ui.color_edit_button_srgb(&mut rgb).changed();
            ui.weak(format_color(rgb));
        }
        if changed {
            *value = on.then(|| format_color(rgb));
        }
    });
}

/// "Choose…" button opening the desktop's file picker, and the chosen file's name.
pub fn icon_file(ui: &mut egui::Ui, icon: &mut Option<String>) {
    ui.horizontal(|ui| {
        if ui.button("Choose…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .set_title("Choose a key icon")
                .add_filter("Images", &["png", "jpg", "jpeg"])
                .pick_file()
        {
            *icon = Some(path.display().to_string());
        }
        if let Some(path) = icon.clone() {
            let name = Path::new(&path)
                .file_name()
                .map_or(path.clone(), |n| n.to_string_lossy().into_owned());
            ui.label(name).on_hover_text(&path);
            if ui
                .small_button("Remove")
                .on_hover_text("Remove the icon")
                .clicked()
            {
                *icon = None;
            }
        }
    });
}

/// Dropdown of every module's live values of one kind, e.g. "spotify:title".
pub fn value_picker(ui: &mut egui::Ui, id: &str, value: &mut Option<ValueRef>, pick: Pick) {
    let selected = value
        .as_ref()
        .map_or("None".to_string(), ToString::to_string);
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .width(WIDTH)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "None");
            for module in catalog::all() {
                for info in module.values.iter().filter(|v| pick.accepts(v)) {
                    let option = ValueRef {
                        module: module.name.to_string(),
                        name: info.name.to_string(),
                    };
                    let text = option.to_string();
                    ui.selectable_value(value, Some(option), text)
                        .on_hover_text(info.help);
                }
            }
        });
}

/// Dropdown of the modules.
pub fn module_picker(ui: &mut egui::Ui, module: &mut Option<String>) {
    egui::ComboBox::from_id_salt("module")
        .selected_text(module.as_deref().unwrap_or("None"))
        .width(WIDTH)
        .show_ui(ui, |ui| {
            ui.selectable_value(module, None, "None");
            for info in catalog::all() {
                ui.selectable_value(module, Some(info.name.to_string()), info.name)
                    .on_hover_text(info.description);
            }
        });
}

/// Like [`action_picker`], for a required action (empty text means not chosen yet).
pub fn action_text(
    ui: &mut egui::Ui,
    id: &str,
    module: &ModuleInfo,
    action: &mut String,
    choices: &Choices,
) {
    let mut optional = (!action.is_empty()).then(|| action.clone());
    action_picker(ui, id, module, &mut optional, choices);
    *action = optional.unwrap_or_default();
}

/// Chooses one of the module's actions, then its argument if it takes one.
/// For the shell module, a text field for the command line.
pub fn action_picker(
    ui: &mut egui::Ui,
    id: &str,
    module: &ModuleInfo,
    action: &mut Option<String>,
    choices: &Choices,
) {
    let Actions::List(list) = &module.actions else {
        optional_text(ui, action, "Command, e.g. notify-send hi");
        return;
    };
    let current = action.clone().unwrap_or_default();
    let (name, arg) = split_action(&current);
    ui.vertical(|ui| {
        let mut chosen = name.to_string();
        egui::ComboBox::from_id_salt(id)
            .selected_text(if name.is_empty() { "None" } else { name })
            .width(WIDTH)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut chosen, String::new(), "None");
                for info in *list {
                    ui.selectable_value(&mut chosen, info.name.to_string(), info.name)
                        .on_hover_text(info.help);
                }
            });
        if chosen != name {
            *action = module.action(&chosen).map(|info| {
                let default = default_arg(info.arg, choices);
                join_action(info.name, default.as_deref())
            });
            return;
        }
        if let Some(info) = module.action(name)
            && info.arg != Arg::None
        {
            let mut arg = arg.unwrap_or_default().to_string();
            if arg_input(ui, id, info.arg, &mut arg, choices) {
                *action = Some(join_action(name, Some(&arg)));
            }
        }
    });
}

/// Argument given to a newly chosen action.
fn default_arg(arg: Arg, choices: &Choices) -> Option<String> {
    match arg {
        Arg::None => None,
        Arg::Page => Some("1".into()),
        Arg::Folder => Some(choices.folders.first().cloned().unwrap_or_default()),
        Arg::Id => Some(String::new()),
    }
}

/// Input for an action's argument. Returns true if it changed.
fn arg_input(ui: &mut egui::Ui, id: &str, kind: Arg, arg: &mut String, choices: &Choices) -> bool {
    let before = arg.clone();
    match kind {
        Arg::None => {}
        Arg::Page => {
            egui::ComboBox::from_id_salt((id, "page"))
                .selected_text(format!("Page {arg}"))
                .width(WIDTH)
                .show_ui(ui, |ui| {
                    for page in 1..=choices.page_count {
                        ui.selectable_value(arg, page.to_string(), format!("Page {page}"));
                    }
                });
        }
        Arg::Folder if choices.folders.is_empty() => {
            ui.weak("No folder yet: add one on the left.");
        }
        Arg::Folder => {
            egui::ComboBox::from_id_salt((id, "folder"))
                .selected_text(arg.clone())
                .width(WIDTH)
                .show_ui(ui, |ui| {
                    for folder in &choices.folders {
                        ui.selectable_value(arg, folder.clone(), folder.as_str());
                    }
                });
        }
        Arg::Id => {
            let field = egui::TextEdit::singleline(arg)
                .hint_text("Channel ID")
                .desired_width(WIDTH);
            ui.add(field).on_hover_text(
                "In Discord: Settings > Advanced > Developer Mode, then right-click a voice channel > Copy Channel ID",
            );
        }
    }
    *arg != before
}
