//! Small tools that live inside `BetterStreamDux` (no external app): timer, stopwatch,
//! pomodoro, counter. Shared here: the countdown, time formatting, desktop notifications.
//!
//! Their logic takes the current time (`now`) as a parameter, so tests never wait.

pub mod counter;
pub mod pomodoro;
pub mod stopwatch;
pub mod timer;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use zbus::zvariant::Value;

pub const MINUTE: Duration = Duration::from_mins(1);

/// A countdown that can be paused. Shared by the timer and the pomodoro.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Countdown {
    #[default]
    Idle,
    Running {
        ends_at: Instant,
    },
    Paused {
        left: Duration,
    },
}

impl Countdown {
    pub fn start(&mut self, now: Instant, length: Duration) {
        *self = Self::Running {
            ends_at: now + length,
        };
    }

    pub fn pause_resume(&mut self, now: Instant) {
        *self = match *self {
            Self::Running { ends_at } => Self::Paused {
                left: ends_at.saturating_duration_since(now),
            },
            Self::Paused { left } => Self::Running {
                ends_at: now + left,
            },
            Self::Idle => Self::Idle,
        };
    }

    /// Adds time; starts a countdown of `extra` if idle.
    pub fn add(&mut self, now: Instant, extra: Duration) {
        match self {
            Self::Running { ends_at } => *ends_at += extra,
            Self::Paused { left } => *left += extra,
            Self::Idle => self.start(now, extra),
        }
    }

    /// Time left, `None` when idle.
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        match *self {
            Self::Running { ends_at } => Some(ends_at.saturating_duration_since(now)),
            Self::Paused { left } => Some(left),
            Self::Idle => None,
        }
    }

    /// True when a running countdown has reached zero.
    pub fn is_over(&self, now: Instant) -> bool {
        matches!(*self, Self::Running { ends_at } if now >= ends_at)
    }

    pub fn status(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running { .. } => "running",
            Self::Paused { .. } => "paused",
        }
    }
}

/// Parses an action argument in minutes, e.g. "5".
pub fn minutes(arg: &str) -> Result<Duration> {
    let minutes: u32 = arg
        .parse()
        .with_context(|| format!("'{arg}' is not a number of minutes"))?;
    ensure!(minutes > 0, "the duration must be at least 1 minute");
    Ok(MINUTE * minutes)
}

/// Time left as "04:59". Rounded up, so a 5 min countdown starts at "05:00" and ends at "00:00".
pub fn countdown_text(left: Duration) -> String {
    clock_text(left.as_millis().div_ceil(1000))
}

/// Time elapsed as "01:23". Rounded down, like a stopwatch.
pub fn elapsed_text(elapsed: Duration) -> String {
    clock_text(u128::from(elapsed.as_secs()))
}

/// "MM:SS", or "H:MM:SS" from one hour.
fn clock_text(seconds: u128) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

/// Shows a desktop notification (with a sound if the desktop plays one), without blocking.
pub fn notify(summary: &str, body: &str) {
    let (summary, body) = (summary.to_string(), body.to_string());
    std::thread::spawn(move || {
        if let Err(err) = send_notification(&summary, &body) {
            log::warn!("cannot show notification: {err:#}");
        }
    });
}

/// The freedesktop notification D-Bus call, understood by GNOME, KDE...
fn send_notification(summary: &str, body: &str) -> Result<()> {
    let bus = zbus::blocking::Connection::session()?;
    let hints: HashMap<&str, Value> = HashMap::from([("sound-name", Value::from("complete"))]);
    let no_actions: Vec<&str> = Vec::new();
    let replaces_id = 0u32;
    let default_timeout = -1i32;
    bus.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &(
            "BetterStreamDux",
            replaces_id,
            "alarm-symbolic",
            summary,
            body,
            no_actions,
            hints,
            default_timeout,
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn countdown_pauses_and_resumes() {
        let t = Instant::now();
        let mut countdown = Countdown::default();
        countdown.start(t, MINUTE);
        assert_eq!(countdown.remaining(t + 20 * SECOND), Some(40 * SECOND));
        countdown.pause_resume(t + 20 * SECOND);
        assert_eq!(
            countdown.remaining(t + 50 * SECOND),
            Some(40 * SECOND),
            "paused"
        );
        countdown.pause_resume(t + 50 * SECOND);
        assert!(!countdown.is_over(t + 89 * SECOND));
        assert!(countdown.is_over(t + 90 * SECOND));
    }

    #[test]
    fn adding_time_to_idle_starts() {
        let t = Instant::now();
        let mut countdown = Countdown::default();
        countdown.add(t, MINUTE);
        assert_eq!(countdown.status(), "running");
        assert_eq!(countdown.remaining(t), Some(MINUTE));
    }

    #[test]
    fn formats_times() {
        assert_eq!(countdown_text(MINUTE * 5), "05:00");
        assert_eq!(
            countdown_text(Duration::from_millis(299_001)),
            "05:00",
            "rounded up"
        );
        assert_eq!(countdown_text(Duration::ZERO), "00:00");
        assert_eq!(
            elapsed_text(Duration::from_millis(83_900)),
            "01:23",
            "rounded down"
        );
        assert_eq!(elapsed_text(Duration::from_secs(3723)), "1:02:03");
    }

    #[test]
    fn parses_minutes() {
        assert_eq!(minutes("5").unwrap(), MINUTE * 5);
        assert!(minutes("0").is_err());
        assert!(minutes("five").is_err());
    }
}
