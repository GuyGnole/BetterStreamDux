//! Undo / redo of config edits. Changes made in quick succession (typing a label,
//! dragging a slider) are grouped into one step.

use std::time::{Duration, Instant};

use crate::config::Config;

/// Changes closer together than this are one undo step.
const GROUP: Duration = Duration::from_millis(800);
const MAX_STEPS: usize = 100;

pub struct History {
    undo: Vec<Config>,
    redo: Vec<Config>,
    /// The config as last recorded.
    current: Config,
    last_change: Option<Instant>,
}

impl History {
    pub fn new(config: &Config) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            current: config.clone(),
            last_change: None,
        }
    }

    /// Call once per frame with the edited config.
    pub fn record(&mut self, config: &Config, now: Instant) {
        if *config == self.current {
            return;
        }
        let same_step = self.last_change.is_some_and(|t| now - t < GROUP);
        if !same_step {
            self.undo.push(self.current.clone());
            if self.undo.len() > MAX_STEPS {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.current = config.clone();
        self.last_change = Some(now);
    }

    /// The config before the last step, if any.
    pub fn undo(&mut self) -> Option<Config> {
        let previous = self.undo.pop()?;
        self.redo
            .push(std::mem::replace(&mut self.current, previous.clone()));
        self.last_change = None;
        Some(previous)
    }

    /// The config the last undo went back from, if any.
    pub fn redo(&mut self) -> Option<Config> {
        let next = self.redo.pop()?;
        self.undo
            .push(std::mem::replace(&mut self.current, next.clone()));
        self.last_change = None;
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(brightness: u8) -> Config {
        Config::parse(&format!("brightness = {brightness}")).unwrap()
    }

    #[test]
    fn undo_and_redo() {
        let t = Instant::now();
        let mut history = History::new(&config(10));
        history.record(&config(20), t);
        history.record(&config(30), t + Duration::from_secs(5));
        assert_eq!(history.undo().unwrap().brightness, 20);
        assert_eq!(history.undo().unwrap().brightness, 10);
        assert!(history.undo().is_none());
        assert_eq!(history.redo().unwrap().brightness, 20);
        assert!(history.can_redo());
    }

    #[test]
    fn quick_changes_are_one_step() {
        let t = Instant::now();
        let mut history = History::new(&config(10));
        history.record(&config(20), t);
        history.record(&config(30), t + Duration::from_millis(100));
        assert_eq!(history.undo().unwrap().brightness, 10);
    }

    #[test]
    fn a_new_change_clears_redo() {
        let t = Instant::now();
        let mut history = History::new(&config(10));
        history.record(&config(20), t);
        history.undo();
        history.record(&config(40), t + Duration::from_secs(5));
        assert!(!history.can_redo());
    }
}
