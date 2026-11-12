//! Turns raw key down/up events into triggers (short press, long press, while press)
//! and runs the matching module action.
//!
//! Time is passed in as `now` so the logic can be tested without waiting.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::modules::Registry;

pub struct Handler {
    config: Config,
    modules: Registry,
    long_press: Duration,
    /// Keys currently held that have a `long_press` binding.
    held: HashMap<u8, Held>,
}

struct Held {
    since: Instant,
    long_press_done: bool,
}

impl Handler {
    pub fn new(config: Config, modules: Registry) -> Self {
        let long_press = Duration::from_millis(config.long_press_ms);
        Self {
            config,
            modules,
            long_press,
            held: HashMap::new(),
        }
    }

    pub fn key_down(&mut self, key: u8, now: Instant) {
        let Some(button) = self.config.button(key) else {
            log::debug!("key {key} has no binding");
            return;
        };
        if let Some(while_press) = &button.while_press {
            run(&mut self.modules, &button.module, &while_press.press);
        } else if button.long_press.is_none() {
            // No long press to tell apart: run right away instead of waiting for release.
            if let Some(action) = &button.short_press {
                run(&mut self.modules, &button.module, action);
            }
        } else {
            // Short or long? We only know on release or when the delay runs out (see `tick`).
            self.held.insert(
                key,
                Held {
                    since: now,
                    long_press_done: false,
                },
            );
        }
    }

    pub fn key_up(&mut self, key: u8) {
        let Some(button) = self.config.button(key) else {
            return;
        };
        if let Some(while_press) = &button.while_press {
            run(&mut self.modules, &button.module, &while_press.release);
        } else if let Some(held) = self.held.remove(&key)
            && !held.long_press_done
            && let Some(action) = &button.short_press
        {
            run(&mut self.modules, &button.module, action);
        }
    }

    /// Call regularly: fires `long_press` for keys held long enough.
    pub fn tick(&mut self, now: Instant) {
        for (key, held) in &mut self.held {
            if held.long_press_done || now - held.since < self.long_press {
                continue;
            }
            held.long_press_done = true;
            if let Some(button) = self.config.button(*key)
                && let Some(action) = &button.long_press
            {
                run(&mut self.modules, &button.module, action);
            }
        }
    }
}

/// Runs one action. Errors are logged, they must never stop the program.
fn run(modules: &mut Registry, module: &str, action: &str) {
    log::info!("{module}::{action}");
    if let Err(err) = modules.run(module, action) {
        log::error!("{module}::{action} failed: {err:#}");
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
            "rec"
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
            module = "rec"
            short_press = "quick"
            [[button]]
            key = 1
            module = "rec"
            short_press = "short"
            long_press = "long"
            [[button]]
            key = 2
            module = "rec"
            while_press = { press = "down", release = "up" }
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
        h.key_down(9, Instant::now());
        h.key_up(9);
        assert!(log.borrow().is_empty());
    }
}
