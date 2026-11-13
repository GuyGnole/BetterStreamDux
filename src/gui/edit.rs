//! Config changes that touch several buttons at once: pages, folders, swapping keys,
//! placing presets. Plain functions on `Config`, tested without a window.

use anyhow::{Result, ensure};

use crate::builtin::MODULE_NAME;
use crate::catalog::{NEW_FOLDER, Preset, join_action, split_action};
use crate::config::{Button, Config, MAX_PAGES, split_module};
use crate::pages::View;

/// Adds a main page at the end. Returns its number, or `None` if there are already 5.
pub fn add_page(config: &mut Config) -> Option<u8> {
    let count = config.page_count();
    if count >= MAX_PAGES {
        return None;
    }
    config.pages = Some(count + 1);
    Some(count + 1)
}

/// Deletes a page and its keys. Later pages move down by one, and `go_to_page`
/// actions follow them; the ones pointing at the deleted page are removed.
pub fn delete_page(config: &mut Config, page: u8) {
    let count = config.page_count();
    config.buttons.retain(|b| b.view() != View::Page(page));
    for button in &mut config.buttons {
        if let Some(p) = button.page
            && p > page
        {
            button.page = Some(p - 1);
        }
    }
    rewrite_deck_actions(config, |name, arg| {
        match (name, arg.and_then(|a| a.parse::<u8>().ok())) {
            ("go_to_page", Some(p)) if p == page => None,
            ("go_to_page", Some(p)) if p > page => {
                Some(join_action(name, Some(&(p - 1).to_string())))
            }
            _ => Some(join_action(name, arg)),
        }
    });
    config.pages = Some((count - 1).max(1));
}

/// Creates a folder with a Back key, so it is valid right away. Returns its name.
pub fn add_folder(config: &mut Config) -> String {
    let folders = config.folders();
    let name = (1..=1000)
        .map(|n| format!("Folder {n}"))
        .find(|name| !folders.contains(name))
        .unwrap_or_default();
    config.buttons.push(Button {
        short_press: vec![format!("{MODULE_NAME}:back")],
        symbol: Some("back".into()),
        color: Some("#333333".into()),
        ..Button::new(&View::Folder(name.clone()), 0)
    });
    name
}

/// Renames a folder, and the `open_folder` actions pointing at it. Returns the new name.
pub fn rename_folder(config: &mut Config, old: &str, new: &str) -> Result<String> {
    let new = new.trim().to_string();
    ensure!(!new.is_empty(), "the folder name is empty");
    if new == old {
        return Ok(new);
    }
    ensure!(
        !config.folders().contains(&new),
        "a folder named '{new}' already exists"
    );
    for button in &mut config.buttons {
        if button.folder.as_deref() == Some(old) {
            button.folder = Some(new.clone());
        }
    }
    rewrite_deck_actions(config, |name, arg| match (name, arg) {
        ("open_folder", Some(folder)) if folder == old => Some(join_action(name, Some(&new))),
        _ => Some(join_action(name, arg)),
    });
    Ok(new)
}

/// Deletes a folder's keys, and the `open_folder` actions pointing at it.
pub fn delete_folder(config: &mut Config, folder: &str) {
    config
        .buttons
        .retain(|b| b.folder.as_deref() != Some(folder));
    rewrite_deck_actions(config, |name, arg| match (name, arg) {
        ("open_folder", Some(f)) if f == folder => None,
        _ => Some(join_action(name, arg)),
    });
}

/// Swaps the keys `a` and `b` in `view` (either may be empty).
pub fn swap_keys(config: &mut Config, view: &View, a: u8, b: u8) {
    for button in config.buttons.iter_mut().filter(|btn| btn.view() == *view) {
        if button.key == a {
            button.key = b;
        } else if button.key == b {
            button.key = a;
        }
    }
}

/// Puts a preset on `key` in `view`, replacing what was there. A preset that opens
/// `{new_folder}` gets a new folder (not inside a folder: folders cannot open folders).
pub fn place_preset(config: &mut Config, view: &View, key: u8, preset: &Preset) -> Result<()> {
    let mut button = preset.button()?;
    button.place(view, key);
    if button.actions().any(|a| a.contains(NEW_FOLDER)) {
        ensure!(
            !matches!(view, View::Folder(_)),
            "a folder cannot open another folder"
        );
        let folder = add_folder(config);
        for action in all_actions(&mut button) {
            *action = action.replace(NEW_FOLDER, &folder);
        }
    }
    put(config, view, key, button);
    Ok(())
}

/// Puts `button` on `key` in `view`, replacing what was there.
pub fn put(config: &mut Config, view: &View, key: u8, mut button: Button) {
    button.place(view, key);
    config
        .buttons
        .retain(|b| !(b.key == key && b.view() == *view));
    config.buttons.push(button);
}

fn all_actions(button: &mut Button) -> impl Iterator<Item = &mut String> {
    let held = button
        .while_press
        .iter_mut()
        .flat_map(|w| w.press.iter_mut().chain(w.release.iter_mut()));
    button
        .short_press
        .iter_mut()
        .chain(button.long_press.iter_mut())
        .chain(held)
}

/// Applies `change(name, arg)` to every deck action. `None` removes the action.
fn rewrite_deck_actions(
    config: &mut Config,
    change: impl Fn(&str, Option<&str>) -> Option<String>,
) {
    let map = |action: &String| match split_module(action) {
        Some((MODULE_NAME, rest)) => {
            let (name, arg) = split_action(rest);
            change(name, arg).map(|new| format!("{MODULE_NAME}:{new}"))
        }
        _ => Some(action.clone()), // other modules: unchanged
    };
    for button in &mut config.buttons {
        button.short_press = button.short_press.iter().filter_map(map).collect();
        button.long_press = button.long_press.iter().filter_map(map).collect();
        if let Some(held) = &mut button.while_press {
            held.press = held.press.iter().filter_map(map).collect();
            held.release = held.release.iter().filter_map(map).collect();
            if held.press.is_empty() && held.release.is_empty() {
                button.while_press = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    fn deck_button(view: &View, key: u8, action: &str) -> Button {
        Button {
            short_press: vec![format!("deck:{action}")],
            ..Button::new(view, key)
        }
    }

    #[test]
    fn deleting_a_page_shifts_later_pages() {
        let mut config = Config::parse("pages = 3").unwrap();
        config.buttons = vec![
            deck_button(&View::Page(1), 0, "go_to_page 3"),
            deck_button(&View::Page(1), 1, "go_to_page 2"),
            deck_button(&View::Page(2), 0, "next_page"),
            deck_button(&View::Page(3), 0, "go_to_page 1"),
        ];
        delete_page(&mut config, 2);
        config.check().unwrap();
        assert_eq!(config.page_count(), 2);
        assert_eq!(config.buttons.len(), 3, "page 2's key is gone");
        let first = config.button(&View::Page(1), 0).unwrap();
        assert_eq!(first.short_press, ["deck:go_to_page 2"], "page 3 became 2");
        let second = config.button(&View::Page(1), 1).unwrap();
        assert!(second.short_press.is_empty(), "pointed at the deleted page");
        assert!(
            config.button(&View::Page(2), 0).is_some(),
            "page 3's key moved to page 2"
        );
    }

    #[test]
    fn deleting_page_1_keeps_shared_keys() {
        let mut config = Config::parse("pages = 2").unwrap();
        config.buttons = vec![
            deck_button(&View::Shared, 14, "next_page"),
            deck_button(&View::Page(1), 0, "next_page"),
        ];
        delete_page(&mut config, 1);
        assert_eq!(config.buttons.len(), 1);
        assert!(config.buttons[0].shared);
    }

    #[test]
    fn new_folder_is_valid_and_uniquely_named() {
        let mut config = Config::parse("").unwrap();
        assert_eq!(add_folder(&mut config), "Folder 1");
        assert_eq!(add_folder(&mut config), "Folder 2");
        config.check().unwrap();
    }

    #[test]
    fn renaming_a_folder_updates_open_folder() {
        let mut config = Config::parse("").unwrap();
        let folder = add_folder(&mut config);
        config.buttons.push(Button {
            short_press: vec!["spotify:play".into(), format!("deck:open_folder {folder}")],
            ..Button::new(&View::Page(1), 0)
        });

        assert!(rename_folder(&mut config, &folder, " ").is_err());
        assert_eq!(
            rename_folder(&mut config, &folder, " Music ").unwrap(),
            "Music"
        );
        config.check().unwrap();
        let opener = config.button(&View::Page(1), 0).unwrap();
        assert_eq!(
            opener.short_press,
            ["spotify:play", "deck:open_folder Music"]
        );

        add_folder(&mut config);
        assert!(
            rename_folder(&mut config, "Folder 1", "Music").is_err(),
            "name taken"
        );
    }

    #[test]
    fn deleting_a_folder_removes_its_openers() {
        let mut config = Config::parse("").unwrap();
        let folder = add_folder(&mut config);
        config.buttons.push(deck_button(
            &View::Page(1),
            0,
            &format!("open_folder {folder}"),
        ));
        delete_folder(&mut config, &folder);
        config.check().unwrap();
        assert!(config.folders().is_empty());
        assert!(
            config
                .button(&View::Page(1), 0)
                .unwrap()
                .short_press
                .is_empty()
        );
    }

    #[test]
    fn swapping_keys_works_with_an_empty_key() {
        let mut config = Config::parse("").unwrap();
        config
            .buttons
            .push(deck_button(&View::Page(1), 0, "next_page"));
        swap_keys(&mut config, &View::Page(1), 0, 7);
        assert!(config.button(&View::Page(1), 0).is_none());
        assert!(config.button(&View::Page(1), 7).is_some());
    }

    #[test]
    fn every_preset_can_be_placed() {
        for module in catalog::all() {
            for preset in module.presets {
                let mut config = Config::parse("").unwrap();
                place_preset(&mut config, &View::Page(1), 3, preset).unwrap();
                config
                    .check()
                    .unwrap_or_else(|err| panic!("preset '{}': {err:#}", preset.name));
                assert!(config.button(&View::Page(1), 3).is_some());
            }
        }
    }

    #[test]
    fn folder_preset_creates_its_folder_but_not_inside_a_folder() {
        let deck = catalog::find("deck").unwrap();
        let open_folder = deck
            .presets
            .iter()
            .find(|p| p.name == "Open folder")
            .unwrap();
        let mut config = Config::parse("").unwrap();
        place_preset(&mut config, &View::Page(1), 0, open_folder).unwrap();
        assert_eq!(config.folders(), ["Folder 1"]);
        let inside = place_preset(
            &mut config,
            &View::Folder("Folder 1".into()),
            5,
            open_folder,
        );
        assert!(inside.is_err());
    }
}
