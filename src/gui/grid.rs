//! Middle panel: the 15 keys, drawn with the deck's own rendering code.
//! Click a key to edit it; drag a key onto another one to swap them; drop a library
//! preset on a key to place it.

use std::collections::HashMap;
use std::time::Duration;

use eframe::egui;
use image::DynamicImage;

use super::library::PresetPayload;
use super::{Editor, edit};
use crate::config::{Config, ValueRef};
use crate::pages::View;
use crate::render::{self, KEY_SIZE};
use crate::screen::{self, KeyFace};
use crate::{builtin, catalog};

const KEY_PX: f32 = 92.0;
const GAP: f32 = 14.0;
/// Old previews are dropped past this many, to bound memory.
const MAX_CACHED: usize = 300;

/// Rendered key images, cached by content.
#[derive(Default)]
pub struct Previews {
    textures: HashMap<KeyFace, egui::TextureHandle>,
}

impl Previews {
    pub fn texture(&mut self, ctx: &egui::Context, face: &KeyFace) -> egui::TextureHandle {
        if self.textures.len() > MAX_CACHED {
            self.textures.clear();
        }
        self.textures
            .entry(face.clone())
            .or_insert_with(|| {
                ctx.load_texture("key", to_color_image(face), egui::TextureOptions::LINEAR)
            })
            .clone()
    }
}

pub fn show(ed: &mut Editor, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    ui.heading(match &ed.view {
        View::Page(page) => format!("Page {page}"),
        View::Folder(name) => format!("Folder: {name}"),
        View::Shared => "All pages".to_string(),
    });
    if ed.view == View::Shared {
        ui.weak("Keys set here appear on every page, unless a page has its own key at that place.");
    } else {
        ui.weak("Click a key to edit it. Drag a key onto another one to swap them.");
    }
    ui.add_space(16.0);

    let values = |value: &ValueRef| preview_value(&ed.config, &ed.view, value);
    let screen = screen::build(&ed.config, &ed.view, ed.config.brightness, &values);
    let mut clicked = None;
    let mut swap = None;
    let mut dropped = None;
    let width = 5.0 * KEY_PX + 4.0 * GAP;
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - width) / 2.0).max(0.0)); // center the grid
        egui::Grid::new("keys").spacing([GAP, GAP]).show(ui, |ui| {
            for (key, face) in (0u8..).zip(&screen.keys) {
                let texture = ed.previews.texture(ui.ctx(), face);
                // Keys inherited from "All pages" are edited there, so they don't move here.
                let inherited = ed.config.shows_shared(&ed.view, key);
                let response = key_cell(ui, &texture, ed.selected == Some(key), inherited);
                if response.clicked() {
                    clicked = Some(key);
                }
                if response.dragged() && !inherited {
                    response.dnd_set_drag_payload(key);
                }
                if let Some(from) = response.dnd_release_payload::<u8>() {
                    swap = Some((*from, key));
                }
                if let Some(payload) = response.dnd_release_payload::<PresetPayload>() {
                    dropped = Some((key, payload.preset));
                }
                if key % 5 == 4 {
                    ui.end_row();
                }
            }
        });
    });

    if let Some(key) = clicked {
        ed.selected = Some(key);
    }
    if let Some((from, to)) = swap
        && from != to
    {
        edit::swap_keys(&mut ed.config, &ed.view, from, to);
        ed.selected = Some(to);
    }
    if let Some((key, preset)) = dropped {
        match edit::place_preset(&mut ed.config, &ed.view, key, preset) {
            Ok(()) => ed.selected = Some(key),
            Err(err) => ed.message = format!("{err:#}"),
        }
    }
    // Keeps the clock key current.
    ui.ctx().request_repaint_after(Duration::from_secs(10));
}

fn key_cell(
    ui: &mut egui::Ui,
    texture: &egui::TextureHandle,
    selected: bool,
    inherited: bool,
) -> egui::Response {
    let image = egui::Image::new(texture)
        .fit_to_exact_size(egui::vec2(KEY_PX, KEY_PX))
        .corner_radius(12.0)
        .sense(egui::Sense::click_and_drag());
    let response = ui
        .add(image)
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    let stroke = if selected {
        Some(egui::Stroke::new(3.0, ui.visuals().selection.bg_fill))
    } else if response.dnd_hover_payload::<u8>().is_some()
        || response.dnd_hover_payload::<PresetPayload>().is_some()
        || response.hovered()
    {
        Some(egui::Stroke::new(1.5, ui.visuals().weak_text_color()))
    } else {
        None
    };
    if let Some(stroke) = stroke {
        ui.painter().rect_stroke(
            response.rect.expand(4.0),
            15.0,
            stroke,
            egui::StrokeKind::Outside,
        );
    }
    if inherited {
        badge(ui, response.rect, "shared");
        return response.on_hover_text("Same on every page: edit it in \"All pages\"");
    }
    response
}

/// Small label in a key's top-left corner.
fn badge(ui: &egui::Ui, key: egui::Rect, text: &str) {
    let rect = egui::Rect::from_min_size(
        key.left_top() + egui::vec2(6.0, 6.0),
        egui::vec2(48.0, 17.0),
    );
    ui.painter()
        .rect_filled(rect, 4.0, egui::Color32::from_black_alpha(170));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(11.0),
        egui::Color32::from_gray(225),
    );
}

/// A live value for the preview: the deck's own values, and sample text for modules
/// (the editor does not connect to Spotify or Discord).
pub fn preview_value(config: &Config, view: &View, value: &ValueRef) -> Option<String> {
    if value.module == builtin::MODULE_NAME {
        let page_label = match view {
            View::Page(page) => format!("{page}/{}", config.page_count()),
            View::Folder(name) => name.clone(),
            View::Shared => format!("1/{}", config.page_count()),
        };
        let clock = chrono::Local::now().format("%H:%M").to_string();
        return builtin::value(&value.name, &clock, &page_label, config.brightness);
    }
    let sample = catalog::find(&value.module)?.value(&value.name)?.sample;
    (!sample.is_empty()).then(|| sample.to_string())
}

fn to_color_image(face: &KeyFace) -> egui::ColorImage {
    // A missing icon file shows the key without it (the config check does not open files).
    let image = render::render(face)
        .or_else(|_| {
            render::render(&KeyFace {
                icon: None,
                ..face.clone()
            })
        })
        .unwrap_or_else(|_| DynamicImage::new_rgb8(KEY_SIZE, KEY_SIZE))
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw())
}
