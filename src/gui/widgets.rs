//! Small inputs shared by the panels. Each edits one config field in place and only
//! writes it when the user changes something (so viewing a key never modifies it).

use std::path::Path;

use eframe::egui;

use crate::catalog::{self, Actions, Arg, ModuleInfo, ValueKind, join_action, split_action};
use crate::config::{ValueRef, format_color, join_instance, parse_color, split_instance};
use crate::icons::{self, ICONS};

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
    /// Text values with known states, for `look_by`.
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
pub fn optional_text(ui: &mut egui::Ui, value: &mut Option<String>, hint: &str, width: f32) {
    let mut text = value.clone().unwrap_or_default();
    let field = egui::TextEdit::singleline(&mut text)
        .hint_text(hint)
        .desired_width(width);
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
        }
        if changed {
            *value = on.then(|| format_color(rgb));
        }
    });
}

/// Button showing the current icon; opens a searchable grid of icons.
pub fn symbol_picker(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    symbol: &mut Option<String>,
) {
    let current = symbol.as_deref().and_then(icons::glyph);
    let text = match (current, symbol.as_deref()) {
        (Some(glyph), Some(name)) => format!("{glyph}  {name}"),
        _ => "No icon".to_string(),
    };
    ui.push_id(id, |ui| {
        ui.menu_button(text, |ui| {
            let search_id = egui::Id::unique("icon search");
            let mut search: String = ui.data_mut(|d| d.get_temp(search_id).unwrap_or_default());
            ui.add(
                egui::TextEdit::singleline(&mut search)
                    .hint_text("Search icons")
                    .desired_width(300.0),
            );
            ui.data_mut(|d| d.insert_temp(search_id, search.clone()));
            if ui.button("No icon").clicked() {
                *symbol = None;
                ui.close();
            }
            let search = search.to_lowercase();
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .show(ui, |ui| {
                    ui.set_max_width(300.0);
                    ui.horizontal_wrapped(|ui| {
                        let matching = ICONS.iter().filter(|icon| {
                            icon.name.contains(&search) || icon.keywords.contains(&search)
                        });
                        for icon in matching {
                            let glyph = egui::RichText::new(icon.glyph.to_string()).size(22.0);
                            let button = egui::Button::new(glyph).min_size(egui::vec2(38.0, 38.0));
                            if ui.add(button).on_hover_text(icon.name).clicked() {
                                *symbol = Some(icon.name.to_string());
                                ui.close();
                            }
                        }
                    });
                });
            ui.weak("Any emoji also works, typed in the config file.");
        });
    });
}

/// "Choose…" button opening the desktop's file picker, and the chosen file's name.
pub fn icon_file(ui: &mut egui::Ui, icon: &mut Option<String>) {
    ui.horizontal(|ui| {
        if ui.button("Choose…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .set_title("Choose a key image")
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
                .on_hover_text("Remove the image")
                .clicked()
            {
                *icon = None;
            }
        }
    });
}

/// Dropdown of every module's live values of one kind, e.g. "spotify:title", plus an id
/// field for modules with several copies ("tea" in "timer.tea:remaining").
pub fn value_picker(ui: &mut egui::Ui, id: &str, value: &mut Option<ValueRef>, pick: Pick) {
    let instance = value
        .as_ref()
        .map(|v| v.instance.clone())
        .unwrap_or_default();
    let current = value.as_ref().map(|v| (v.module.clone(), v.name.clone()));
    let mut chosen = current.clone();
    ui.horizontal(|ui| {
        let selected = current
            .as_ref()
            .map_or("None".to_string(), |(module, name)| {
                format!("{module}:{name}")
            });
        egui::ComboBox::from_id_salt(id)
            .selected_text(selected)
            .width(WIDTH - 70.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut chosen, None, "None");
                for module in catalog::all() {
                    for info in module.values.iter().filter(|v| pick.accepts(v)) {
                        let option = (module.name.to_string(), info.name.to_string());
                        let text = format!("{}:{}", module.name, info.name);
                        ui.selectable_value(&mut chosen, Some(option), text)
                            .on_hover_text(info.help);
                    }
                }
            });
        if chosen != current {
            *value = chosen.map(|(module, name)| ValueRef {
                // Keep the id when switching between values of the same module.
                instance: if current.as_ref().is_some_and(|(m, _)| *m == module) {
                    instance.clone()
                } else {
                    String::new()
                },
                module,
                name,
            });
        }
        if let Some(value) = value
            && catalog::find(&value.module).is_some_and(|m| m.instances)
        {
            id_field(ui, &mut value.instance);
        }
    });
}

/// The id of a module copy ("tea" in `timer.tea`). Empty: the default copy.
fn id_field(ui: &mut egui::Ui, instance: &mut String) {
    ui.add(
        egui::TextEdit::singleline(instance)
            .hint_text("id")
            .desired_width(50.0),
    )
    .on_hover_text(
        "Which copy: keys with the same id share one timer/counter/... Empty: the default one.",
    );
}

/// A trigger's actions, run in order: one row each, "×" removes, "+ Add action" adds.
pub fn action_list(ui: &mut egui::Ui, id: &str, actions: &mut Vec<String>, choices: &Choices) {
    let mut remove = None;
    for (i, action) in actions.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            action_row(ui, &format!("{id}-{i}"), action, choices);
            if ui
                .small_button("×")
                .on_hover_text("Remove this action")
                .clicked()
            {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        actions.remove(i);
    }
    if ui.small_button("+ Add action").clicked() {
        actions.push(String::new());
    }
}

/// One "module:action": module dropdown, its id if it has copies, then the action and
/// its argument.
fn action_row(ui: &mut egui::Ui, id: &str, action: &mut String, choices: &Choices) {
    // Not `split_module`: "spotify:" (module chosen, action not yet) must still show the module.
    let (module, rest) = action.split_once(':').unwrap_or(("", action));
    let (module, instance) = split_instance(module);
    let (module, mut instance, rest) = (module.to_string(), instance.to_string(), rest.to_string());

    let mut chosen = module.clone();
    egui::ComboBox::from_id_salt((id, "module"))
        .selected_text(if module.is_empty() { "Module" } else { &module })
        .width(80.0)
        .show_ui(ui, |ui| {
            for info in catalog::all() {
                ui.selectable_value(&mut chosen, info.name.to_string(), info.name)
                    .on_hover_text(info.description);
            }
        });
    if chosen != module {
        *action = format!("{chosen}:"); // actions belong to one module: choose again
        return;
    }
    let Some(info) = catalog::find(&module) else {
        return;
    };
    if info.instances {
        id_field(ui, &mut instance);
    }
    let mut current = (!rest.is_empty()).then_some(rest.clone());
    action_picker(ui, id, info, &mut current, choices);
    let new = format!(
        "{}:{}",
        join_instance(&module, &instance),
        current.unwrap_or_default()
    );
    if new != *action {
        *action = new;
    }
}

/// Chooses one of the module's actions, then its argument if it takes one.
/// For the shell module, a text field for the command line.
fn action_picker(
    ui: &mut egui::Ui,
    id: &str,
    module: &ModuleInfo,
    action: &mut Option<String>,
    choices: &Choices,
) {
    let Actions::List(list) = &module.actions else {
        optional_text(ui, action, "Command, e.g. notify-send hi", 160.0);
        return;
    };
    let current = action.clone().unwrap_or_default();
    let (name, arg) = split_action(&current);
    let mut chosen = name.to_string();
    egui::ComboBox::from_id_salt((id, "action"))
        .selected_text(if name.is_empty() { "Action" } else { name })
        .width(110.0)
        .show_ui(ui, |ui| {
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
}

/// Argument given to a newly chosen action.
fn default_arg(arg: Arg, choices: &Choices) -> Option<String> {
    match arg {
        Arg::None => None,
        Arg::Page => Some("1".into()),
        Arg::Folder => Some(choices.folders.first().cloned().unwrap_or_default()),
        Arg::Id => Some(String::new()),
        Arg::Minutes => Some("5".into()),
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
                .width(70.0)
                .show_ui(ui, |ui| {
                    for page in 1..=choices.page_count {
                        ui.selectable_value(arg, page.to_string(), format!("Page {page}"));
                    }
                });
        }
        Arg::Folder if choices.folders.is_empty() => {
            ui.weak("No folder yet");
        }
        Arg::Folder => {
            egui::ComboBox::from_id_salt((id, "folder"))
                .selected_text(arg.clone())
                .width(100.0)
                .show_ui(ui, |ui| {
                    for folder in &choices.folders {
                        ui.selectable_value(arg, folder.clone(), folder.as_str());
                    }
                });
        }
        Arg::Minutes => {
            let mut minutes: u32 = arg.parse().unwrap_or(5);
            ui.add(
                egui::DragValue::new(&mut minutes)
                    .range(1..=1440)
                    .suffix(" min"),
            );
            *arg = minutes.to_string();
        }
        Arg::Id => {
            let field = egui::TextEdit::singleline(arg)
                .hint_text("Channel ID")
                .desired_width(100.0);
            ui.add(field).on_hover_text(
                "In Discord: Settings > Advanced > Developer Mode, then right-click a voice channel > Copy Channel ID",
            );
        }
    }
    *arg != before
}
