//! Turns raw key down/up events into triggers (short press, long press, while press)
//! and runs the matching action: built-in `deck` actions here, others in `modules/`.
//!
//! Time is passed in as `now` so the logic can be tested without waiting.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::builtin::{self, DeckAction};
use crate::config::{Button, Config, ValueRef};
use crate::modules::Registry;
use crate::pages::{Navigator, View};
use crate::screen::{self, Screen};

pub struct Handler {
    config: Config,
    modules: Registry,
    nav: Navigator,
    brightness: u8,
    long_press: Duration,
    /// Keys currently held down.
    pressed: HashMap<u8, Pressed>,
}

struct Pressed {
    /// Copy of the button taken at key down: if the page changes while the key is held,
    /// release and long press still belong to the button that was pressed.
    button: Button,
    since: Instant,
    long_press_done: bool,
}

impl Handler {
    pub fn new(config: Config, modules: Registry) -> Self {
        Self {
            nav: Navigator::new(config.page_count()),
            brightness: config.brightness,
            long_press: Duration::from_millis(config.long_press_ms),
            pressed: HashMap::new(),
            modules,
            config,
        }
    }

    /// Switches to a new config (the file changed), keeping the modules running.
    pub fn set_config(&mut self, config: Config) {
        self.nav.set_page_count(config.page_count());
        if let View::Folder(name) = self.nav.view()
            && !config.folders().contains(&name)
        {
            self.nav.back(); // the open folder was deleted
        }
        self.brightness = config.brightness;
        self.long_press = Duration::from_millis(config.long_press_ms);
        self.pressed.clear();
        self.config = config;
    }

    /// What the deck should show right now.
    pub fn screen(&self, clock: &str) -> Screen {
        let values = |v: &ValueRef| self.value(v, clock);
        screen::build(&self.config, &self.nav.view(), self.brightness, &values)
    }

    /// A live value: built-in for "deck:...", otherwise asked to the module.
    fn value(&self, value: &ValueRef, clock: &str) -> Option<String> {
        if value.module != builtin::MODULE_NAME {
            return self.modules.value(&value.module, &value.name);
        }
        builtin::value(&value.name, clock, &self.nav.label(), self.brightness)
    }

    pub fn key_down(&mut self, key: u8, now: Instant) {
        let Some(button) = self.config.shown_button(&self.nav.view(), key).cloned() else {
            log::debug!("key {key} has no binding");
            return;
        };
        if let Some(while_press) = &button.while_press {
            self.run(&button, &while_press.press);
        } else if button.long_press.is_none()
            && let Some(action) = &button.short_press
        {
            // No long press to tell apart: run right away instead of waiting for release.
            self.run(&button, action);
        }
        // Otherwise: short or long? We only know on release or when the delay runs out (`tick`).
        self.pressed.insert(
            key,
            Pressed {
                button,
                since: now,
                long_press_done: false,
            },
        );
    }

    pub fn key_up(&mut self, key: u8) {
        let Some(pressed) = self.pressed.remove(&key) else {
            return;
        };
        let button = &pressed.button;
        if let Some(while_press) = &button.while_press {
            self.run(button, &while_press.release);
        } else if button.long_press.is_some()
            && !pressed.long_press_done
            && let Some(action) = &button.short_press
        {
            self.run(button, action);
        }
    }

    /// Call regularly: fires `long_press` for keys held long enough.
    pub fn tick(&mut self, now: Instant) {
        let mut due = Vec::new();
        for pressed in self.pressed.values_mut() {
            if let Some(action) = &pressed.button.long_press
                && !pressed.long_press_done
                && now - pressed.since >= self.long_press
            {
                pressed.long_press_done = true;
                due.push((pressed.button.clone(), action.clone()));
            }
        }
        for (button, action) in due {
            self.run(&button, &action);
        }
    }

    /// Runs one action. Errors are logged, they must never stop the program.
    fn run(&mut self, button: &Button, action: &str) {
        let module = button.module.as_deref().unwrap_or_default();
        log::info!("{module}::{action}");
        let result = if module == builtin::MODULE_NAME {
            DeckAction::parse(action).map(|a| self.run_deck_action(a))
        } else {
            self.modules.run(module, action)
        };
        if let Err(err) = result {
            log::error!("{module}::{action} failed: {err:#}");
        }
    }

    fn run_deck_action(&mut self, action: DeckAction) {
        match action {
            DeckAction::NextPage => self.nav.next_page(),
            DeckAction::PreviousPage => self.nav.previous_page(),
            DeckAction::GoToPage(page) => self.nav.go_to_page(page),
            DeckAction::OpenFolder(name) => self.nav.open_folder(&name),
            DeckAction::Back => self.nav.back(),
            DeckAction::BrightnessUp => self.brightness = builtin::brightness_up(self.brightness),
            DeckAction::BrightnessDown => {
                self.brightness = builtin::brightness_down(self.brightness);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::modules::Module;

    /// Fake module that records every action it receives.
    struct Recorder(Rc<RefCell<Vec<String>>>);

    impl Module for Recorder {
        fn name(&self) -> &'static str {
            "shell" // any action text is valid for the shell module
        }
        fn run(&mut self, action: &str) -> anyhow::Result<()> {
            self.0.borrow_mut().push(action.to_string());
            Ok(())
        }
    }

    fn setup() -> (Handler, Rc<RefCell<Vec<String>>>) {
        let config = Config::parse(
            r#"
            long_press_ms = 500
            [[button]]
            key = 0
            module = "shell"
            short_press = "quick"
            [[button]]
            key = 1
            module = "shell"
            short_press = "short"
            long_press = "long"
            [[button]]
            key = 2
            module = "shell"
            while_press = { press = "down", release = "up" }
            [[button]]
            key = 3
            module = "deck"
            short_press = "next_page"
            [[button]]
            key = 4
            module = "deck"
            short_press = "brightness_up"
            [[button]]
            key = 5
            module = "deck"
            short_press = "open_folder f"
            [[button]]
            page = 2
            key = 2
            module = "shell"
            short_press = "page2"
            display = "deck:page_number"
            [[button]]
            folder = "f"
            key = 0
            module = "deck"
            short_press = "back"
            display = "deck:page_number"
            "#,
        )
        .unwrap();
        let log = Rc::new(RefCell::new(Vec::new()));
        let modules = Registry::from(vec![Box::new(Recorder(log.clone())) as Box<dyn Module>]);
        (Handler::new(config, modules), log)
    }

    fn ms(start: Instant, n: u64) -> Instant {
        start + Duration::from_millis(n)
    }

    /// Text shown on `key` right now.
    fn text(h: &Handler, key: usize) -> Option<String> {
        h.screen("00:00").keys[key].text.clone()
    }

    fn tap(h: &mut Handler, key: u8) {
        h.key_down(key, Instant::now());
        h.key_up(key);
    }

    #[test]
    fn short_press_only_fires_immediately() {
        let (mut h, log) = setup();
        h.key_down(0, Instant::now());
        assert_eq!(*log.borrow(), ["quick"]);
        h.key_up(0);
        assert_eq!(*log.borrow(), ["quick"]);
    }

    #[test]
    fn quick_release_is_short_press() {
        let (mut h, log) = setup();
        let t = Instant::now();
        h.key_down(1, t);
        h.tick(ms(t, 200));
        assert!(log.borrow().is_empty());
        h.key_up(1);
        assert_eq!(*log.borrow(), ["short"]);
    }

    #[test]
    fn holding_is_long_press_once() {
        let (mut h, log) = setup();
        let t = Instant::now();
        h.key_down(1, t);
        h.tick(ms(t, 500));
        h.tick(ms(t, 900));
        h.key_up(1);
        assert_eq!(*log.borrow(), ["long"]);
    }

    #[test]
    fn while_press_runs_on_down_and_up() {
        let (mut h, log) = setup();
        let t = Instant::now();
        h.key_down(2, t);
        h.tick(ms(t, 2000));
        h.key_up(2);
        assert_eq!(*log.borrow(), ["down", "up"]);
    }

    #[test]
    fn unbound_key_does_nothing() {
        let (mut h, log) = setup();
        tap(&mut h, 9);
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn next_page_shows_other_buttons() {
        let (mut h, log) = setup();
        tap(&mut h, 3);
        assert_eq!(text(&h, 2).as_deref(), Some("2/2"));
        tap(&mut h, 2);
        assert_eq!(*log.borrow(), ["page2"]);
    }

    #[test]
    fn release_after_page_change_goes_to_pressed_button() {
        let (mut h, log) = setup();
        h.key_down(2, Instant::now()); // while_press on page 1
        tap(&mut h, 3); // go to page 2, where key 2 is another button
        h.key_up(2);
        assert_eq!(*log.borrow(), ["down", "up"]);
    }

    #[test]
    fn folder_opens_and_goes_back() {
        let (mut h, _) = setup();
        tap(&mut h, 5);
        assert_eq!(text(&h, 0).as_deref(), Some("f"));
        tap(&mut h, 0);
        assert_eq!(text(&h, 0), None, "back on page 1, key 0 has no text");
    }

    #[test]
    fn new_config_keeps_page_when_possible() {
        let (mut h, _) = setup();
        tap(&mut h, 3); // page 2
        h.set_config(Config::parse("pages = 2\nbrightness = 40").unwrap());
        assert_eq!(h.nav.view(), View::Page(2));
        assert_eq!(h.screen("00:00").brightness, 40);
        h.set_config(Config::parse("").unwrap()); // only 1 page left
        assert_eq!(h.nav.view(), View::Page(1));
    }

    #[test]
    fn deleted_folder_is_left() {
        let (mut h, _) = setup();
        tap(&mut h, 5); // open folder f
        h.set_config(Config::parse("").unwrap());
        assert_eq!(h.nav.view(), View::Page(1));
    }

    #[test]
    fn shared_key_works_on_every_page_but_not_in_folders() {
        let (mut h, log) = setup();
        let mut config = h.config.clone();
        config.buttons.push(Button {
            module: Some("shell".into()),
            short_press: Some("shared".into()),
            ..Button::new(&View::Shared, 9)
        });
        h.set_config(config);
        tap(&mut h, 9); // page 1
        tap(&mut h, 3); // next page
        tap(&mut h, 9); // page 2
        h.nav.go_to_page(1); // page 2 has no navigation key
        tap(&mut h, 5); // open folder f
        tap(&mut h, 9); // nothing in the folder
        assert_eq!(*log.borrow(), ["shared", "shared"]);
    }

    #[test]
    fn brightness_up_changes_screen() {
        let (mut h, _) = setup();
        tap(&mut h, 4);
        assert_eq!(h.screen("00:00").brightness, 70);
    }
}
