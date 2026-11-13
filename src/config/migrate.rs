//! Converts configs written in older formats when they are loaded:
//!   - `module = "spotify"` + `short_press = "next"`  ->  `short_press = "spotify:next"`
//!   - `color_by = { value, colors }`                 ->  `look_by = { value, states }`
//!
//! The next save (from the editor) writes the new format.

use super::{Button, Config, Look, LookBy};

pub fn migrate(config: &mut Config) {
    config.buttons.iter_mut().for_each(migrate_button);
}

fn migrate_button(button: &mut Button) {
    if let Some(module) = button.module.take() {
        let add_module = |action: &mut String| *action = format!("{module}:{action}");
        button.short_press.iter_mut().for_each(add_module);
        button.long_press.iter_mut().for_each(add_module);
        if let Some(held) = &mut button.while_press {
            held.press.iter_mut().for_each(add_module);
            held.release.iter_mut().for_each(add_module);
        }
    }
    if let Some(old) = button.color_by.take()
        && button.look_by.is_none()
    {
        let states = old
            .colors
            .into_iter()
            .map(|(state, color)| {
                (
                    state,
                    Look {
                        color: Some(color),
                        ..Look::default()
                    },
                )
            })
            .collect();
        button.look_by = Some(LookBy {
            value: old.value,
            states,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_the_old_format() {
        let config = Config::parse(
            r##"
            [[button]]
            key = 0
            module = "discord"
            while_press = { press = "unmute", release = "mute" }
            color_by = { value = "discord:muted", colors = { false = "#3BA55D" } }
            [[button]]
            key = 1
            module = "shell"
            short_press = "echo a:b"
            "##,
        )
        .unwrap();
        let ptt = &config.buttons[0];
        let held = ptt.while_press.as_ref().unwrap();
        assert_eq!(
            (held.press.as_slice(), held.release.as_slice()),
            (
                &["discord:unmute".to_string()][..],
                &["discord:mute".to_string()][..]
            )
        );
        let look = &ptt.look_by.as_ref().unwrap().states["false"];
        assert_eq!(look.color.as_deref(), Some("#3BA55D"));
        assert_eq!(config.buttons[1].short_press, ["shell:echo a:b"]);

        let saved = config.to_toml().unwrap();
        assert!(!saved.contains("module ="), "old fields are never written");
        assert!(!saved.contains("color_by"));
    }
}
