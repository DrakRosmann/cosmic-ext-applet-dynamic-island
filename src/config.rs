// SPDX-License-Identifier: GPL-3.0-only

//! What the island shows, as picked in its popup: kept in COSMIC's
//! configuration, and watched, so the island on each screen follows.

use cosmic::{
    cosmic_config::{Config, ConfigGet, ConfigSet},
    iced::{Subscription, stream},
};
use futures::channel::mpsc;

const VERSION: u64 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TitleWidth {
    Small,
    #[default]
    Medium,
    Large,
}

impl TitleWidth {
    pub const ALL: [TitleWidth; 3] = [TitleWidth::Small, TitleWidth::Medium, TitleWidth::Large];

    fn id(self) -> &'static str {
        match self {
            TitleWidth::Small => "small",
            TitleWidth::Medium => "medium",
            TitleWidth::Large => "large",
        }
    }

    fn from_id(id: &str) -> Option<TitleWidth> {
        Self::ALL.into_iter().find(|w| w.id() == id)
    }

    pub fn pixels(self) -> f32 {
        match self {
            TitleWidth::Small => 100.0,
            TitleWidth::Medium => 160.0,
            TitleWidth::Large => 240.0,
        }
    }
}

/// Each setting, by the key it's saved under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    ShowCover,
    ShowTitle,
    ShowBars,
    Media,
    Recording,
    Timer,
    Charging,
}

impl Toggle {
    fn key(self) -> &'static str {
        match self {
            Toggle::ShowCover => "show_cover",
            Toggle::ShowTitle => "show_title",
            Toggle::ShowBars => "show_bars",
            Toggle::Media => "media",
            Toggle::Recording => "recording",
            Toggle::Timer => "timer",
            Toggle::Charging => "charging",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub show_cover: bool,
    pub show_title: bool,
    pub show_bars: bool,
    pub title_width: TitleWidth,
    pub media: bool,
    pub recording: bool,
    pub timer: bool,
    pub charging: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_cover: true,
            show_title: true,
            show_bars: true,
            title_width: TitleWidth::default(),
            media: true,
            recording: true,
            timer: true,
            charging: true,
        }
    }
}

impl Settings {
    pub fn get(&self, toggle: Toggle) -> bool {
        match toggle {
            Toggle::ShowCover => self.show_cover,
            Toggle::ShowTitle => self.show_title,
            Toggle::ShowBars => self.show_bars,
            Toggle::Media => self.media,
            Toggle::Recording => self.recording,
            Toggle::Timer => self.timer,
            Toggle::Charging => self.charging,
        }
    }

    fn field(&mut self, toggle: Toggle) -> &mut bool {
        match toggle {
            Toggle::ShowCover => &mut self.show_cover,
            Toggle::ShowTitle => &mut self.show_title,
            Toggle::ShowBars => &mut self.show_bars,
            Toggle::Media => &mut self.media,
            Toggle::Recording => &mut self.recording,
            Toggle::Timer => &mut self.timer,
            Toggle::Charging => &mut self.charging,
        }
    }

    /// As saved, each setting not saved yet as it is at first.
    pub fn load() -> Settings {
        let mut settings = Settings::default();
        let Ok(config) = Config::new(crate::APP_ID, VERSION) else {
            return settings;
        };
        for toggle in [
            Toggle::ShowCover,
            Toggle::ShowTitle,
            Toggle::ShowBars,
            Toggle::Media,
            Toggle::Recording,
            Toggle::Timer,
            Toggle::Charging,
        ] {
            if let Ok(value) = config.get::<bool>(toggle.key()) {
                *settings.field(toggle) = value;
            }
        }
        if let Some(width) = config
            .get::<String>("title_width")
            .ok()
            .and_then(|id| TitleWidth::from_id(&id))
        {
            settings.title_width = width;
        }
        settings
    }

    pub fn set(&mut self, toggle: Toggle, value: bool) {
        *self.field(toggle) = value;
        save(toggle.key(), value);
    }

    pub fn set_title_width(&mut self, width: TitleWidth) {
        self.title_width = width;
        save("title_width", width.id().to_owned());
    }
}

fn save<T: serde::Serialize>(key: &str, value: T) {
    if let Err(err) = Config::new(crate::APP_ID, VERSION).and_then(|c| c.set(key, value)) {
        tracing::warn!("cannot save {key}: {err}");
    }
}

/// When the settings change, here or in the island on another screen.
pub fn subscription() -> Subscription<()> {
    Subscription::run_with("settings", |_| {
        stream::channel(4, |output: mpsc::Sender<()>| async move {
            let output = std::sync::Mutex::new(output);
            let watcher = Config::new(crate::APP_ID, VERSION).ok().and_then(|config| {
                config
                    .watch(move |_, _| {
                        if let Ok(mut output) = output.lock() {
                            _ = output.try_send(());
                        }
                    })
                    .inspect_err(|err| tracing::warn!("cannot watch the settings: {err}"))
                    .ok()
            });
            futures::future::pending::<()>().await;
            drop(watcher);
        })
    })
}
