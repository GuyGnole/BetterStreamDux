//! Stopwatch.
//!
//! Actions: `start_stop`, `reset`.
//! Values:  `elapsed` (e.g. "01:23", none when idle), `status` (running/paused/idle).

use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use super::elapsed_text;
use crate::catalog::{Actions, ModuleInfo, action, state, text};
use crate::modules::Module;

pub const INFO: ModuleInfo = ModuleInfo {
    name: "stopwatch",
    description: "Stopwatch",
    actions: Actions::List(&[
        action("start_stop", "Start, or stop (press again to continue)"),
        action("reset", "Back to zero"),
    ]),
    values: &[
        text("elapsed", "Time elapsed, like 01:23", "01:23"),
        state("status", "Stopwatch state", &["running", "paused", "idle"]),
    ],
};

#[derive(Default)]
pub enum Stopwatch {
    #[default]
    Idle,
    Running {
        /// When it was last started, and the time counted before that.
        since: Instant,
        before: Duration,
    },
    Paused {
        elapsed: Duration,
    },
}

impl Stopwatch {
    fn apply(&mut self, action: &str, now: Instant) -> Result<()> {
        *self = match (action, &*self) {
            ("start_stop", Self::Idle) => Self::Running {
                since: now,
                before: Duration::ZERO,
            },
            ("start_stop", Self::Running { since, before }) => Self::Paused {
                elapsed: *before + now.saturating_duration_since(*since),
            },
            ("start_stop", Self::Paused { elapsed }) => Self::Running {
                since: now,
                before: *elapsed,
            },
            ("reset", _) => Self::Idle,
            _ => bail!("unknown stopwatch action '{action}'"),
        };
        Ok(())
    }

    fn value_at(&self, name: &str, now: Instant) -> Option<String> {
        let (elapsed, status) = match self {
            Self::Idle => (None, "idle"),
            Self::Running { since, before } => (
                Some(*before + now.saturating_duration_since(*since)),
                "running",
            ),
            Self::Paused { elapsed } => (Some(*elapsed), "paused"),
        };
        match name {
            "elapsed" => elapsed.map(elapsed_text),
            "status" => Some(status.into()),
            _ => None,
        }
    }
}

impl Module for Stopwatch {
    fn name(&self) -> &'static str {
        INFO.name
    }

    fn run(&mut self, action: &str) -> Result<()> {
        self.apply(action, Instant::now())
    }

    fn value(&self, name: &str) -> Option<String> {
        self.value_at(name, Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn stops_and_continues() {
        let t = Instant::now();
        let mut watch = Stopwatch::default();
        watch.apply("start_stop", t).unwrap();
        watch.apply("start_stop", t + 10 * SECOND).unwrap();
        assert_eq!(
            watch.value_at("elapsed", t + 60 * SECOND).as_deref(),
            Some("00:10"),
            "stopped"
        );
        watch.apply("start_stop", t + 60 * SECOND).unwrap();
        assert_eq!(
            watch.value_at("elapsed", t + 65 * SECOND).as_deref(),
            Some("00:15")
        );
        watch.apply("reset", t).unwrap();
        assert_eq!(watch.value_at("elapsed", t), None);
        assert!(watch.apply("fly", t).is_err());
    }
}
