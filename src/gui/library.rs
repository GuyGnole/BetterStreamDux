//! Bottom of the middle panel: the ready-made buttons (presets) of every module, drawn
//! like keys. Drag one onto a key to place it.

use eframe::egui;

use super::Editor;
use super::grid::preview_value;
use crate::catalog::{self, ModuleInfo, Preset};
use crate::pages::View;
use crate::screen;

const TILE_PX: f32 = 64.0;

/// What can be dragged in the editor. A single type, because egui's `take_payload`
/// discards a payload of another type: checking for one kind would lose the other.
pub enum Dragged {
    /// A key of the grid, to swap with another.
    Key(u8),
    /// A library preset, with its preview drawn under the pointer while dragging.
    Preset(&'static Preset, egui::TextureId),
}

/// The library's own state: which module tab, and the search text.
#[derive(Default)]
pub struct Library {
    /// `None`: every module.
    module: Option<&'static str>,
    search: String,
}

pub fn show(ed: &mut Editor, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Library");
        ui.weak("Drag a button onto a key. It replaces the key (Ctrl+Z to undo).");
    });
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut ed.library.module, None, "All");
        for module in catalog::all().filter(|m| !m.presets.is_empty()) {
            ui.selectable_value(&mut ed.library.module, Some(module.name), module.name);
        }
        ui.add(
            egui::TextEdit::singleline(&mut ed.library.search)
                .hint_text("Search")
                .desired_width(140.0),
        );
    });
    ui.add_space(4.0);

    let search = ed.library.search.to_lowercase();
    let shown: Vec<(&ModuleInfo, &Preset)> = catalog::all()
        .filter(|m| ed.library.module.is_none_or(|name| name == m.name))
        .flat_map(|m| m.presets.iter().map(move |p| (m, p)))
        .filter(|(m, p)| {
            search.is_empty() || p.name.to_lowercase().contains(&search) || m.name.contains(&search)
        })
        .collect();

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
            for (module, preset) in shown {
                tile(ed, ui, module, preset);
            }
        });
    });
}

/// One preset: its preview (draggable) and its name.
fn tile(ed: &mut Editor, ui: &mut egui::Ui, module: &ModuleInfo, preset: &'static Preset) {
    let Ok(button) = preset.button() else {
        return; // a broken preset is caught by the tests
    };
    let values = |value: &_| preview_value(&ed.config, &View::Page(1), value);
    let face = screen::face(&button, &values);
    let texture = ed.previews.texture(ui.ctx(), &face);

    // A fixed size lets the row wrap before a tile instead of cutting it.
    let size = egui::vec2(TILE_PX + 12.0, TILE_PX + 22.0);
    let layout = egui::Layout::top_down(egui::Align::Center);
    ui.allocate_ui_with_layout(size, layout, |ui| {
        ui.set_width(size.x);
        let image = egui::Image::new(&texture)
            .fit_to_exact_size(egui::vec2(TILE_PX, TILE_PX))
            .corner_radius(8.0)
            .sense(egui::Sense::drag());
        let response = ui
            .add(image)
            .on_hover_cursor(egui::CursorIcon::Grab)
            .on_hover_text(format!("{} ({})", preset.name, module.name));
        // Called every frame: egui only keeps the payload on the frame the drag starts.
        response.dnd_set_drag_payload(Dragged::Preset(preset, texture.id()));
        ui.add(egui::Label::new(egui::RichText::new(preset.name).small()).truncate());
    });
}

/// While a preset is dragged, draws its preview under the pointer.
pub fn draw_dragged(ctx: &egui::Context) {
    let Some(dragged) = egui::DragAndDrop::payload::<Dragged>(ctx) else {
        return;
    };
    let Dragged::Preset(_, texture) = *dragged else {
        return; // a dragged key needs no preview: egui shows the move
    };
    let Some(pointer) = ctx.pointer_interact_pos() else {
        return;
    };
    let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("dragged preset"));
    let rect = egui::Rect::from_center_size(pointer, egui::vec2(TILE_PX, TILE_PX));
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    ctx.layer_painter(layer)
        .image(texture, rect, uv, egui::Color32::from_white_alpha(220));
}
