// SPDX-License-Identifier: GPL-3.0-only

//! A countdown, as the phone's timer shows in the island.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timer {
    pub total: Duration,
    state: State,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Running { ends: Instant },
    Paused { left: Duration },
}

impl Timer {
    pub fn start(total: Duration) -> Self {
        Self {
            total,
            state: State::Running {
                ends: Instant::now() + total,
            },
        }
    }

    pub fn left(&self) -> Duration {
        match self.state {
            State::Running { ends } => ends.saturating_duration_since(Instant::now()),
            State::Paused { left } => left,
        }
    }

    pub fn running(&self) -> bool {
        matches!(self.state, State::Running { .. })
    }

    pub fn done(&self) -> bool {
        self.running() && self.left().is_zero()
    }

    /// How much is gone, from 0 to 1.
    pub fn progress(&self) -> f32 {
        1.0 - self.left().as_secs_f32() / self.total.as_secs_f32().max(1.0)
    }

    pub fn toggle(&mut self) {
        self.state = match self.state {
            State::Running { .. } => State::Paused { left: self.left() },
            State::Paused { left } => State::Running {
                ends: Instant::now() + left,
            },
        };
    }

    pub fn add(&mut self, more: Duration) {
        self.total += more;
        self.state = match self.state {
            State::Running { ends } => State::Running { ends: ends + more },
            State::Paused { left } => State::Paused { left: left + more },
        };
    }
}

/// 4:05, or 1:02:03 past an hour, rounding up as a countdown does.
pub fn clock(left: Duration) -> String {
    let seconds = left.as_secs() + u64::from(left.subsec_nanos() > 0);
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[zbus::proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: HashMap<&str, zbus::zvariant::Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;
}

pub async fn notify(summary: String, body: String) -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let mut hints = HashMap::new();
    hints.insert("urgency", zbus::zvariant::Value::U8(2));
    hints.insert("sound-name", zbus::zvariant::Value::from("alarm-clock-elapsed"));
    NotificationsProxy::new(&conn)
        .await?
        .notify(
            "Dynamic Island",
            0,
            "alarm-symbolic",
            &summary,
            &body,
            &[],
            hints,
            -1,
        )
        .await?;
    Ok(())
}
