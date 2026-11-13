//! Left panel: the main pages and the folders. Right-click one to rename or delete it.

use eframe::egui;

use super::{Editor, edit};
use crate::config::MAX_PAGES;
use crate::pages::View;

pub fn show(ed: &mut Editor, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    ui.heading("Pages");
    let shared = entry(ui, ed.view == View::Shared, "All pages");
    if shared
        .on_hover_text("Keys shown on every page, like page navigation")
        .clicked()
    {
        ed.show_view(View::Shared);
    }
    let count = ed.config.page_count();
    for page in 1..=count {
        let view = View::Page(page);
        let response = entry(ui, ed.view == view, &format!("Page {page}"));
        if response.clicked() {
            ed.show_view(view);
        }
        response.context_menu(|ui| {
            let delete = ui.add_enabled(count > 1, egui::Button::new("Delete page"));
            if delete
                .on_disabled_hover_text("The last page cannot be deleted")
                .clicked()
            {
                edit::delete_page(&mut ed.config, page);
                ed.show_view(View::Page(1));
                ui.close();
            }
        });
    }
    let add = ui.add_enabled(count < MAX_PAGES, egui::Button::new("+ Add page"));
    if add.on_disabled_hover_text("5 pages at most").clicked()
        && let Some(page) = edit::add_page(&mut ed.config)
    {
        ed.show_view(View::Page(page));
    }

    ui.add_space(12.0);
    ui.separator();
    ui.heading("Folders");
    for name in ed.config.folders() {
        if ed.renaming.as_ref().is_some_and(|(old, _)| *old == name) {
            rename_field(ed, ui);
            continue;
        }
        let view = View::Folder(name.clone());
        let response = entry(ui, ed.view == view, &name);
        if response.clicked() {
            ed.show_view(view.clone());
        }
        response.context_menu(|ui| {
            if ui.button("Rename").clicked() {
                ed.renaming = Some((name.clone(), name.clone()));
                ui.close();
            }
            if ui.button("Delete folder").clicked() {
                edit::delete_folder(&mut ed.config, &name);
                if ed.view == view {
                    ed.show_view(View::Page(1));
                }
                ui.close();
            }
        });
    }
    if ui.button("+ Add folder").clicked() {
        let name = edit::add_folder(&mut ed.config);
        ed.show_view(View::Folder(name));
    }

    ui.add_space(16.0);
    ui.weak("Right-click a page or folder to rename or delete it.");
    ui.weak("A key opens a folder with the deck action \"open_folder\".");
}

/// A full-width selectable line.
fn entry(ui: &mut egui::Ui, selected: bool, text: &str) -> egui::Response {
    ui.add(
        egui::Button::selectable(selected, text).min_size(egui::vec2(ui.available_width(), 28.0)),
    )
}

/// Text field replacing a folder's line while it is renamed. Enter or clicking away
/// renames, Escape cancels.
fn rename_field(ed: &mut Editor, ui: &mut egui::Ui) {
    let Some((_, text)) = &mut ed.renaming else {
        return;
    };
    let response = ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY));
    if !response.has_focus() && !response.lost_focus() {
        response.request_focus(); // first frame
    }
    if !response.lost_focus() {
        return;
    }
    let Some((old, new)) = ed.renaming.take() else {
        return;
    };
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        return;
    }
    match edit::rename_folder(&mut ed.config, &old, &new) {
        Ok(name) if ed.view == View::Folder(old) => ed.view = View::Folder(name),
        Ok(_) => {}
        Err(err) => ed.message = format!("{err:#}"),
    }
}
