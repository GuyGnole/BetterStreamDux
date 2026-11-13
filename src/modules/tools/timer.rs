//! Countdown timer, with a desktop notification when it ends.
//!
//! Actions: `start N` (minutes), `pause_resume` (also restarts the last countdown),
//!          `add_minute`, `cancel`.
//! Values:  `remaining` (e.g. "04:59", none when idle), `status` (running/paused/done/idle).

use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use super::{Countdown, MINUTE, countdown_text, minutes, notify};
use crate::catalog::{Actions, Arg, ModuleInfo, action, action_with, split_action, state, text};
use crate::modules::Module;

pub const INFO: ModuleInfo = ModuleInfo {
    name: "timer",
    description: "Countdown timer, with a notification when it ends",
    actions: Actions::List(&[
        action_with("start", Arg::Minutes, "Start a countdown"),
        action(
            "pause_resume",
            "Pause or resume (restarts the last countdown when done)",
        ),
        action("add_minute", "Add one minute"),
        action("cancel", "Stop and clear the timer"),
    ]),
    values: &[
        text("remaining", "Time left, like 04:59", "04:59"),
        state(
            "status",
            "Timer state",
            &["running", "paused", "done", "idle"],
        ),
    ],
};

#[derive(Default)]
pub struct Timer {
    countdown: Countdown,
    /// Length of the last countdown started, for `pause_resume` after it ended.
    last: Option<Duration>,
    /// Reached zero; stays "done" until the next action.
    done: bool,
}

impl Timer {
    fn apply(&mut self, action: &str, now: Instant) -> Result<()> {
        self.done = false;
        match split_action(action) {
            ("start", Some(arg)) => {
                let length = minutes(arg)?;
                self.last = Some(length);
                self.countdown.start(now, length);
            }
            ("pause_resume", None) => match (self.countdown, self.last) {
                (Countdown::Idle, Some(length)) => self.countdown.start(now, length),
                _ => self.countdown.pause_resume(now),
            },
            ("add_minute", None) => self.countdown.add(now, MINUTE),
            ("cancel", None) => self.countdown = Countdown::Idle,
            _ => bail!("unknown timer action '{action}'"),
        }
        Ok(())
    }

    /// Returns true when the countdown has just ended.
    fn update(&mut self, now: Instant) -> bool {
        if !self.countdown.is_over(now) {
            return false;
        }
        self.countdown = Countdown::Idle;
        self.done = true;
        true
    }

    fn value_at(&self, name: &str, now: Instant) -> Option<String> {
        match name {
            "remaining" if self.done => Some(countdown_text(Duration::ZERO)),
            "remaining" => self.countdown.remaining(now).map(countdown_text),
            "status" if self.done => Some("done".into()),
            "status" => Some(self.countdown.status().into()),
            _ => None,
        }
    }
}

impl Module for Timer {
    fn name(&self) -> &'static str {
        INFO.name
    }

    fn run(&mut self, action: &str) -> Result<()> {
        self.apply(action, Instant::now())
    }

    fn tick(&mut self) {
        if self.update(Instant::now()) {
            notify("Timer", "Time is up!");
        }
    }

    fn value(&self, name: &str) -> Option<String> {
        self.value_at(name, Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_down_then_is_done() {
        let t = Instant::now();
        let mut timer = Timer::default();
        assert_eq!(
            timer.value_at("remaining", t),
            None,
            "idle: the label shows"
        );
        timer.apply("start 2", t).unwrap();
        assert_eq!(timer.value_at("remaining", t).as_deref(), Some("02:00"));
        assert!(!timer.update(t + MINUTE));
        assert!(timer.update(t + MINUTE * 2), "ends once");
        assert!(!timer.update(t + MINUTE * 3));
        assert_eq!(timer.value_at("status", t).as_deref(), Some("done"));
        assert_eq!(timer.value_at("remaining", t).as_deref(), Some("00:00"));
    }

    #[test]
    fn pause_resume_restarts_after_the_end() {
        let t = Instant::now();
        let mut timer = Timer::default();
        timer.apply("start 1", t).unwrap();
        timer.update(t + MINUTE);
        timer.apply("pause_resume", t + MINUTE).unwrap();
        assert_eq!(timer.value_at("status", t).as_deref(), Some("running"));
        assert_eq!(
            timer.value_at("remaining", t + MINUTE).as_deref(),
            Some("01:00")
        );
    }

    #[test]
    fn every_catalog_action_is_accepted() {
        let Actions::List(actions) = INFO.actions else {
            panic!("timer actions are a list");
        };
        for info in actions {
            let arg = (info.arg != Arg::None).then_some("1");
            let action = crate::catalog::join_action(info.name, arg);
            assert!(
                Timer::default().apply(&action, Instant::now()).is_ok(),
                "{action}"
            );
        }
    }
}
