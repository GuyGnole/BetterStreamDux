//! Small tools that live inside `BetterStreamDux` (no external app): timer, stopwatch,
//! pomodoro, counter. Shared here: copies by id (`Instances`), the countdown, time
//! formatting, desktop notifications.
//!
//! Each tool can have several independent copies, chosen by an id in the config:
//! `timer.tea:start 4` and `timer.pasta:start 10` are two timers; `timer:start 5` uses
//! the default one. Their logic takes the current time (`now`) as a parameter, so tests
//! never wait.

pub mod counter;
pub mod pomodoro;
pub mod stopwatch;
pub mod timer;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use zbus::zvariant::Value;

use crate::config::Config;
use crate::modules::Module;

pub const MINUTE: Duration = Duration::from_mins(1);

/// One copy of a tool (one timer, one stopwatch...).
pub trait Tool: Default {
    fn apply(&mut self, action: &str, now: Instant) -> Result<()>;
    fn value_at(&self, name: &str, now: Instant) -> Option<String>;
    /// Called every tick: a notification to show when something just ended.
    fn update(&mut self, _now: Instant) -> Option<Notification> {
        None
    }
    fn configure(&mut self, _config: &Config) {}
}

/// A desktop notification.
pub struct Notification {
    pub title: String,
    pub text: String,
    /// Stays on screen until dismissed (instead of going to the notification list).
    pub urgent: bool,
}

/// Every copy of one tool, by id ("" for the default copy). Copies are created on
/// first use and are fully independent.
pub struct Instances<T> {
    name: &'static str,
    all: HashMap<String, T>,
    /// Last config, to configure copies created later.
    config: Option<Config>,
}

impl<T: Tool> Instances<T> {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            all: HashMap::new(),
            config: None,
        }
    }

    fn copy(&mut self, instance: &str) -> &mut T {
        let config = &self.config;
        self.all.entry(instance.to_string()).or_insert_with(|| {
            let mut tool = T::default();
            if let Some(config) = config {
                tool.configure(config);
            }
            tool
        })
    }
}

impl<T: Tool> Module for Instances<T> {
    fn name(&self) -> &'static str {
        self.name
    }

    fn run(&mut self, instance: &str, action: &str) -> Result<()> {
        self.copy(instance).apply(action, Instant::now())
    }

    fn value(&self, instance: &str, name: &str) -> Option<String> {
        let now = Instant::now();
        match self.all.get(instance) {
            Some(tool) => tool.value_at(name, now),
            None => T::default().value_at(name, now), // not used yet: shows as new
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        for (id, tool) in &mut self.all {
            if let Some(mut notification) = tool.update(now) {
                if !id.is_empty() {
                    notification.title = format!("{} ({id})", notification.title);
                }
                notify(&notification);
            }
        }
    }

    fn configure(&mut self, config: &Config) {
        self.config = Some(config.clone());
        self.all
            .values_mut()
            .for_each(|tool| tool.configure(config));
    }
}

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
pub fn notify(notification: &Notification) {
    log::info!(
        "notification: {} - {}",
        notification.title,
        notification.text
    );
    let (title, text, urgent) = (
        notification.title.clone(),
        notification.text.clone(),
        notification.urgent,
    );
    std::thread::spawn(move || {
        if let Err(err) = send_notification(&title, &text, urgent) {
            log::warn!("cannot show notification: {err:#}");
        }
    });
}

/// The freedesktop notification D-Bus call, understood by GNOME, KDE...
fn send_notification(summary: &str, body: &str, urgent: bool) -> Result<()> {
    let bus = zbus::blocking::Connection::session()?;
    // Urgency 2 ("critical") stays on screen until dismissed; 1 is normal.
    let urgency: u8 = if urgent { 2 } else { 1 };
    let hints: HashMap<&str, Value> = HashMap::from([
        ("sound-name", Value::from("complete")),
        ("urgency", Value::from(urgency)),
    ]);
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
    fn copies_with_different_ids_are_independent() {
        let mut timers = Instances::<timer::Timer>::new("timer");
        timers.run("tea", "start 4").unwrap();
        timers.run("pasta", "start 10").unwrap();
        timers.run("pasta", "pause_resume").unwrap();
        let value = |id, name| timers.value(id, name).unwrap();
        assert_eq!(value("tea", "status"), "running");
        assert_eq!(value("pasta", "status"), "paused");
        assert_eq!(
            value("", "status"),
            "idle",
            "the default timer is untouched"
        );
        assert_eq!(value("never-used", "status"), "idle");
    }

    #[test]
    fn notification_names_the_copy() {
        let t = Instant::now();
        let mut timer = timer::Timer::default();
        timer.apply("start 1", t).unwrap();
        let notification = timer.update(t + MINUTE).unwrap();
        assert!(notification.urgent, "a timer's end stays on screen");
        assert_eq!(notification.title, "Timer");
    }

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
