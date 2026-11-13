//! Pomodoro: work, short break, work, ... and a long break every few rounds.
//! Each phase starts by itself when the previous one ends, with a notification.
//! Durations come from the `[pomodoro]` config section.
//!
//! Actions: `start_pause`, `skip` (next phase now), `reset`.
//! Values:  `remaining` (e.g. "24:59", none when idle), `phase` (`work`, `break`, `long_break`, `idle`),
//!          `status` (running/paused/idle), `done` (work rounds finished since reset).

use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use super::{Countdown, MINUTE, Notification, Tool, countdown_text};
use crate::catalog::{Actions, ModuleInfo, action, preset, state, text};
use crate::config::{Config, PomodoroSettings};

pub const INFO: ModuleInfo = ModuleInfo {
    name: "pomodoro",
    description: "Pomodoro timer: work and breaks, durations in the Pomodoro settings",
    actions: Actions::List(&[
        action("start_pause", "Start, pause or resume"),
        action("skip", "Go to the next phase now"),
        action("reset", "Stop and start over"),
    ]),
    values: &[
        text("remaining", "Time left in the phase, like 24:59", "24:59"),
        state(
            "phase",
            "Current phase",
            &["work", "break", "long_break", "idle"],
        ),
        state("status", "Pomodoro state", &["running", "paused", "idle"]),
        text("done", "Work rounds finished since reset", "2"),
    ],
    instances: true,
    presets: &[
        preset(
            "Pomodoro",
            r##"
symbol = "tomato"
label = "Pomodoro"
display = "pomodoro:remaining"
short_press = "pomodoro:start_pause"
long_press = "pomodoro:reset"
color = "#444444"
look_by = { value = "pomodoro:phase", states = { work = { color = "#C0392B" }, break = { color = "#27AE60", symbol = "coffee" }, long_break = { color = "#2980B9", symbol = "coffee" } } }
"##,
        ),
        preset(
            "Pomodoro skip",
            r##"
symbol = "next"
label = "Skip"
short_press = "pomodoro:skip"
color = "#333333"
"##,
        ),
    ],
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Work,
    Break,
    LongBreak,
}

#[derive(Default)]
pub struct Pomodoro {
    settings: PomodoroSettings,
    /// `None` until started.
    phase: Option<Phase>,
    countdown: Countdown,
    /// Work rounds finished since the last reset.
    done: u32,
}

impl Pomodoro {
    /// When the phase is over, starts the next one and returns it.
    fn advance(&mut self, now: Instant) -> Option<Phase> {
        self.countdown.is_over(now).then(|| self.next_phase(now))
    }

    fn next_phase(&mut self, now: Instant) -> Phase {
        let next = match self.phase {
            Some(Phase::Work) => {
                self.done += 1;
                if self.done.is_multiple_of(self.settings.rounds.max(1)) {
                    Phase::LongBreak
                } else {
                    Phase::Break
                }
            }
            _ => Phase::Work,
        };
        self.begin(next, now);
        next
    }

    fn begin(&mut self, phase: Phase, now: Instant) {
        self.phase = Some(phase);
        self.countdown.start(now, self.length(phase));
    }

    fn length(&self, phase: Phase) -> Duration {
        let minutes = match phase {
            Phase::Work => self.settings.work_minutes,
            Phase::Break => self.settings.break_minutes,
            Phase::LongBreak => self.settings.long_break_minutes,
        };
        MINUTE * minutes
    }
}

impl Tool for Pomodoro {
    fn apply(&mut self, action: &str, now: Instant) -> Result<()> {
        match action {
            "start_pause" if self.phase.is_none() => self.begin(Phase::Work, now),
            "start_pause" => self.countdown.pause_resume(now),
            "skip" if self.phase.is_some() => {
                self.next_phase(now);
            }
            "skip" => {}
            "reset" => {
                *self = Self {
                    settings: self.settings.clone(),
                    ..Self::default()
                }
            }
            _ => bail!("unknown pomodoro action '{action}'"),
        }
        Ok(())
    }

    fn value_at(&self, name: &str, now: Instant) -> Option<String> {
        match name {
            "remaining" => self.countdown.remaining(now).map(countdown_text),
            "phase" => Some(
                match self.phase {
                    None => "idle",
                    Some(Phase::Work) => "work",
                    Some(Phase::Break) => "break",
                    Some(Phase::LongBreak) => "long_break",
                }
                .into(),
            ),
            "status" => Some(self.countdown.status().into()),
            "done" => Some(self.done.to_string()),
            _ => None,
        }
    }

    fn update(&mut self, now: Instant) -> Option<Notification> {
        let phase = self.advance(now)?;
        let minutes = self.length(phase).as_secs() / 60;
        let (title, text) = match phase {
            Phase::Work => ("Back to work", format!("{minutes} minutes of focus")),
            Phase::Break => ("Short break", format!("Take {minutes} minutes")),
            Phase::LongBreak => ("Long break", format!("Well done! Take {minutes} minutes")),
        };
        Some(Notification {
            title: title.into(),
            text,
            urgent: false,
        })
    }

    /// New durations apply from the next phase.
    fn configure(&mut self, config: &Config) {
        self.settings = config.pomodoro.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pomodoro() -> Pomodoro {
        let settings = PomodoroSettings {
            work_minutes: 25,
            break_minutes: 5,
            long_break_minutes: 15,
            rounds: 2,
        };
        Pomodoro {
            settings,
            ..Pomodoro::default()
        }
    }

    #[test]
    fn cycles_work_and_breaks() {
        let mut t = Instant::now();
        let mut p = pomodoro();
        assert_eq!(p.value_at("phase", t).as_deref(), Some("idle"));
        p.apply("start_pause", t).unwrap();
        assert_eq!(p.value_at("remaining", t).as_deref(), Some("25:00"));

        let mut phases = Vec::new();
        for _ in 0..4 {
            t += p.countdown.remaining(t).unwrap();
            phases.push(p.advance(t).unwrap());
        }
        assert_eq!(
            phases,
            [Phase::Break, Phase::Work, Phase::LongBreak, Phase::Work]
        );
        assert_eq!(p.value_at("done", t).as_deref(), Some("2"));
    }

    #[test]
    fn pause_skip_and_reset() {
        let t = Instant::now();
        let mut p = pomodoro();
        p.apply("start_pause", t).unwrap();
        p.apply("start_pause", t + MINUTE).unwrap();
        assert_eq!(p.value_at("status", t).as_deref(), Some("paused"));
        assert_eq!(p.advance(t + MINUTE * 60), None, "paused: never ends");
        p.apply("skip", t).unwrap();
        assert_eq!(p.value_at("phase", t).as_deref(), Some("break"));
        p.apply("reset", t).unwrap();
        assert_eq!(p.value_at("phase", t).as_deref(), Some("idle"));
        assert_eq!(p.value_at("done", t).as_deref(), Some("0"));
        assert_eq!(p.settings.rounds, 2, "settings kept");
    }
}
