// SPDX-License-Identifier: GPL-3.0-only

//! The island: a black pill in the panel showing what's going on, music,
//! a timer, the charger plugged in, that opens into its live activities.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use cosmic::{
    Element, Task, app,
    iced::{
        self, Alignment, Color, Length, Limits, Subscription,
        core::text::{Ellipsize, EllipsizeHeightLimit},
        platform_specific::shell::commands::popup::destroy_popup,
        window,
    },
    theme,
    widget,
};

use crate::{
    i18n::strings,
    media::{self, Control, Media},
    power::{self, Battery},
    timer::{self, Timer},
    widgets::{self, Bars, Expand, GRAY, GREEN, ORANGE, WHITE, alpha, ghost, glyph, island, tint},
};

/// The popup's width: the cards and the room for their glow.
const WIDTH: f32 = 400.0;
/// Around the cards, for their glow to show.
const MARGIN: f32 = 12.0;
const CARD_RADIUS: f32 = 30.0;
/// A paused player stays on the island this long, then leaves it.
const LINGER: Duration = Duration::from_secs(5 * 60);
/// How long the island shows the charger plugged in, or the timer done.
const FLASH_CHARGING: Duration = Duration::from_secs(4);
const FLASH_TIMER: Duration = Duration::from_secs(10);
const PRESETS: [u64; 4] = [1, 5, 10, 25];

#[derive(Default)]
pub struct App {
    core: app::Core,
    popup: Option<window::Id>,
    media: Vec<Media>,
    /// The player picked in the popup, if not the one playing.
    selected: Option<String>,
    /// When each player was seen paused.
    paused_since: HashMap<String, Instant>,
    /// Where the progress is being dragged to, in microseconds.
    seeking: Option<f32>,
    timer: Option<Timer>,
    battery: Option<Battery>,
    flash: Option<(Flash, Instant)>,
    hovered: bool,
}

/// Something that happened, shown for a moment over the rest.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Flash {
    Charging,
    TimerDone,
}

impl Flash {
    fn lasts(self) -> Duration {
        match self {
            Flash::Charging => FLASH_CHARGING,
            Flash::TimerDone => FLASH_TIMER,
        }
    }
}

/// What the island shows, the most pressing first.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Activity<'a> {
    Flash(Flash),
    Media(&'a Media),
    Timer(&'a Timer),
    Idle,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    Closed(window::Id),
    Hover(bool),
    Media(Vec<Media>),
    Control(Control),
    SelectPlayer(String),
    Seek(f32),
    SeekDone,
    Battery(Option<Battery>),
    StartTimer(u64),
    ToggleTimer,
    AddMinute,
    CancelTimer,
    Tick,
    None,
}

fn run<F: Future<Output = zbus::Result<()>> + Send + 'static>(
    what: &'static str,
    future: F,
) -> app::Task<Message> {
    Task::future(async move {
        if let Err(err) = future.await {
            tracing::warn!("cannot {what}: {err}");
        }
        cosmic::action::app(Message::None)
    })
}

fn popup_limits() -> Limits {
    Limits::NONE
        .min_width(WIDTH)
        .max_width(WIDTH)
        .min_height(1.0)
        .max_height(1000.0)
}

fn one_line() -> Ellipsize {
    Ellipsize::End(EllipsizeHeightLimit::Lines(1))
}

fn label<'a>(text: impl Into<String>, size: u16, color: Color) -> widget::Text<'a, cosmic::Theme> {
    widget::text(text.into())
        .size(size)
        .class(theme::Text::Color(color))
}

/// 3:07, from microseconds.
fn time(micros: f32) -> String {
    let seconds = (micros.max(0.0) / 1_000_000.0) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

impl App {
    /// The player shown: the one picked, or the one playing.
    fn shown_media(&self) -> Option<&Media> {
        self.selected
            .as_ref()
            .and_then(|service| self.media.iter().find(|m| &m.service == service))
            .or_else(|| self.media.first())
    }

    /// Playing, or paused not long ago.
    fn is_live(&self, media: &Media) -> bool {
        media.playing
            || self
                .paused_since
                .get(&media.service)
                .is_some_and(|since| since.elapsed() < LINGER)
    }

    fn flash(&self) -> Option<Flash> {
        self.flash
            .filter(|(flash, at)| at.elapsed() < flash.lasts())
            .map(|(flash, _)| flash)
    }

    fn activity(&self) -> Activity<'_> {
        if let Some(flash) = self.flash() {
            return Activity::Flash(flash);
        }
        match (self.shown_media().filter(|m| self.is_live(m)), &self.timer) {
            // A running timer before paused music, as the phone does.
            (Some(media), Some(timer)) if !media.playing && timer.running() => Activity::Timer(timer),
            (Some(media), _) => Activity::Media(media),
            (None, Some(timer)) => Activity::Timer(timer),
            (None, None) => Activity::Idle,
        }
    }

    /// The timer, when the island shows something else: in a bubble beside it.
    fn bubble(&self) -> Option<&Timer> {
        match self.activity() {
            Activity::Timer(_) => None,
            _ => self.timer.as_ref(),
        }
    }

    /// The island's size in the panel: its height, the panel's less some air,
    /// and its width, wide when something is on.
    fn pill_size(&self) -> (f32, f32) {
        let applet = &self.core.applet;
        let size = f32::from(applet.suggested_size(true).0);
        let minor = f32::from(applet.suggested_padding(true).1);
        let height = (size + 2.0 * minor - 6.0).max(size);
        if !applet.is_horizontal() {
            return (height, height);
        }
        let width = match self.activity() {
            Activity::Idle => height * 3.4,
            _ => (height * 8.5).clamp(180.0, 300.0),
        };
        (width, height)
    }

    fn bubble_width(&self, height: f32) -> f32 {
        if self.core.applet.is_horizontal() { height * 2.6 } else { height }
    }

    /// The applet's whole width, to center the popup under it.
    fn applet_width(&self) -> f32 {
        let (width, height) = self.pill_size();
        let major = f32::from(self.core.applet.suggested_padding(true).0);
        let bubble = match self.bubble() {
            Some(_) => self.bubble_width(height) + 6.0,
            None => 0.0,
        };
        width + bubble + 2.0 * major
    }

    fn close_popup(&mut self) -> app::Task<Message> {
        self.seeking = None;
        match self.popup.take() {
            Some(id) => destroy_popup(id),
            None => Task::none(),
        }
    }

    fn control(&self, control: Control) -> app::Task<Message> {
        let Some(media) = self.shown_media() else {
            return Task::none();
        };
        run("control the player", media::control(media.service.clone(), control))
    }
}

impl cosmic::Application for App {
    type Message = Message;
    type Executor = cosmic::executor::Default;
    type Flags = ();
    const APP_ID: &'static str = crate::APP_ID;

    fn init(core: app::Core, _flags: ()) -> (Self, app::Task<Message>) {
        // COSMIC_ISLAND_PREVIEW=popup opens the popup by itself, and =timer
        // starts a timer, to take screenshots without clicking the panel.
        let preview = std::env::var("COSMIC_ISLAND_PREVIEW").unwrap_or_default();
        let mut tasks = Vec::new();
        if preview.contains("timer") {
            tasks.push(Task::done(cosmic::action::app(Message::StartTimer(1))));
        }
        if preview.contains("popup") {
            tasks.push(Task::future(async {
                tokio::time::sleep(Duration::from_millis(1500)).await;
                cosmic::action::app(Message::TogglePopup)
            }));
        }
        let task = Task::batch(tasks);
        (
            Self {
                core,
                ..Self::default()
            },
            task,
        )
    }

    fn core(&self) -> &app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Message) -> app::Task<Message> {
        match message {
            Message::TogglePopup => {
                if self.popup.is_some() {
                    return self.close_popup();
                }
                let Some(parent) = self.core.main_window_id() else {
                    return Task::none();
                };
                let id = window::Id::unique();
                self.popup = Some(id);
                // Under the middle of the island, however wide it is.
                let icon = f32::from(self.core.applet.suggested_size(true).0);
                let half = ((self.applet_width() - icon) / 2.0).round() as i32;
                let horizontal = self.core.applet.is_horizontal();
                return cosmic::surface::surface_task(cosmic::surface::action::app_popup(
                    |_: &Self| Default::default(),
                    move |app: &mut Self| {
                        let mut settings = app.core.applet.get_popup_settings(
                            parent,
                            id,
                            None,
                            horizontal.then_some(half),
                            None,
                        );
                        settings.positioner.size_limits = popup_limits();
                        settings
                    },
                    None,
                ));
            }
            Message::Closed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                    self.seeking = None;
                }
            }
            Message::Hover(hovered) => self.hovered = hovered,
            Message::Media(media) => {
                let now = Instant::now();
                for m in &media {
                    if m.playing {
                        self.paused_since.remove(&m.service);
                    } else {
                        self.paused_since.entry(m.service.clone()).or_insert(now);
                    }
                }
                self.paused_since
                    .retain(|service, _| media.iter().any(|m| &m.service == service));
                // A player that starts playing takes the island.
                if let Some(selected) = &self.selected {
                    let was_playing = self
                        .media
                        .iter()
                        .filter(|m| m.playing)
                        .map(|m| &m.service)
                        .collect::<Vec<_>>();
                    let started = media
                        .iter()
                        .any(|m| m.playing && &m.service != selected && !was_playing.contains(&&m.service));
                    if started {
                        self.selected = None;
                    }
                }
                self.media = media;
            }
            Message::Control(control) => {
                // Shown at once, before the player says so.
                if let Control::PlayPause = control
                    && let Some(service) = self.shown_media().map(|m| m.service.clone())
                    && let Some(media) = self.media.iter_mut().find(|m| m.service == service)
                {
                    media.position = media.position_now();
                    media.at = Instant::now();
                    media.playing = !media.playing;
                    if media.playing {
                        self.paused_since.remove(&service);
                    } else {
                        self.paused_since.insert(service, Instant::now());
                    }
                }
                return self.control(control);
            }
            Message::SelectPlayer(service) => self.selected = Some(service),
            Message::Seek(position) => self.seeking = Some(position),
            Message::SeekDone => {
                let Some(position) = self.seeking.take() else {
                    return Task::none();
                };
                let Some(service) = self.shown_media().map(|m| m.service.clone()) else {
                    return Task::none();
                };
                let Some(media) = self.media.iter_mut().find(|m| m.service == service) else {
                    return Task::none();
                };
                media.position = position as i64;
                media.at = Instant::now();
                if let Some(track) = media.track.clone() {
                    return run("seek", media::seek(service, track, position as i64));
                }
            }
            Message::Battery(battery) => {
                let was_charging = self.battery.map(|b| b.charging);
                if was_charging == Some(false) && battery.is_some_and(|b| b.charging) {
                    self.flash = Some((Flash::Charging, Instant::now()));
                }
                self.battery = battery;
            }
            Message::StartTimer(minutes) => {
                self.timer = Some(Timer::start(Duration::from_secs(minutes * 60)));
                if self.flash().is_some_and(|f| f == Flash::TimerDone) {
                    self.flash = None;
                }
            }
            Message::ToggleTimer => {
                if let Some(timer) = &mut self.timer {
                    timer.toggle();
                }
            }
            Message::AddMinute => {
                if let Some(timer) = &mut self.timer {
                    timer.add(Duration::from_secs(60));
                }
            }
            Message::CancelTimer => self.timer = None,
            Message::Tick => {
                if self.flash.is_some() && self.flash().is_none() {
                    self.flash = None;
                }
                if self.timer.is_some_and(|t| t.done()) {
                    self.timer = None;
                    self.flash = Some((Flash::TimerDone, Instant::now()));
                    let t = strings();
                    return run(
                        "notify",
                        timer::notify(t.timer_done.to_owned(), t.timer_done_body.to_owned()),
                    );
                }
            }
            Message::None => {}
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            media::subscription().map(Message::Media),
            power::subscription().map(Message::Battery),
        ];
        // Each second, while something counts down or moves on.
        let lingering = self
            .paused_since
            .values()
            .any(|since| since.elapsed() < LINGER);
        let progressing = self.popup.is_some() && self.shown_media().is_some_and(|m| m.playing);
        let every = if self.timer.is_some_and(|t| t.running()) {
            Some(Duration::from_millis(250))
        } else if self.flash.is_some() || progressing {
            Some(Duration::from_millis(500))
        } else if lingering {
            // Only to let the player go, in its time.
            Some(Duration::from_secs(5))
        } else {
            None
        };
        if let Some(every) = every {
            subscriptions.push(iced::time::every(every).map(|_| Message::Tick));
        }
        Subscription::batch(subscriptions)
    }

    fn view(&self) -> Element<'_, Message> {
        let applet = &self.core.applet;
        let (major, minor) = applet.suggested_padding(true);
        let (width, height) = self.pill_size();
        let horizontal = applet.is_horizontal();
        let pad = (height * 0.18).round();
        let inner = height - 2.0 * pad;
        let t = strings();

        let content: Element<'_, Message> = match self.activity() {
            Activity::Idle => widget::space::horizontal().into(),
            Activity::Media(media) => {
                let color = tint(media.color);
                let art = cover(media, inner, inner * 0.3);
                if horizontal {
                    widget::row::with_children(vec![
                        art,
                        label(media.title.clone(), 13, alpha(WHITE, 0.92))
                            .font(cosmic::font::semibold())
                            .ellipsize(one_line())
                            .width(Length::Fill)
                            .into(),
                        Bars::new(color, media.playing, inner * 0.62).into(),
                    ])
                    .spacing(pad + 2.0)
                    .align_y(Alignment::Center)
                    .into()
                } else {
                    art
                }
            }
            Activity::Timer(timer) => {
                let icon = glyph("alarm-symbolic", (inner * 0.8) as u16, ORANGE);
                if horizontal {
                    let color = if timer.running() { ORANGE } else { alpha(ORANGE, 0.6) };
                    widget::row::with_children(vec![
                        icon,
                        widget::space::horizontal().into(),
                        label(timer::clock(timer.left()), 15, color)
                            .font(cosmic::font::semibold())
                            .into(),
                    ])
                    .align_y(Alignment::Center)
                    .into()
                } else {
                    icon
                }
            }
            Activity::Flash(Flash::Charging) => {
                let battery = self.battery.map_or(0.0, |b| b.percentage);
                let icon = glyph(&battery_icon(battery, true), (inner * 0.8) as u16, GREEN);
                if horizontal {
                    widget::row::with_children(vec![
                        label(t.charging, 13, WHITE).font(cosmic::font::semibold()).into(),
                        widget::space::horizontal().into(),
                        label(format!("{battery:.0}%"), 14, GREEN)
                            .font(cosmic::font::semibold())
                            .into(),
                        icon,
                    ])
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .into()
                } else {
                    icon
                }
            }
            Activity::Flash(Flash::TimerDone) => {
                let icon = glyph("alarm-symbolic", (inner * 0.8) as u16, ORANGE);
                if horizontal {
                    widget::row::with_children(vec![
                        icon,
                        widget::space::horizontal().into(),
                        label(t.timer_done, 13, ORANGE).font(cosmic::font::semibold()).into(),
                    ])
                    .align_y(Alignment::Center)
                    .into()
                } else {
                    icon
                }
            }
        };

        let glow = match self.activity() {
            Activity::Media(media) if media.playing => media.color.map(|c| tint(Some(c))),
            _ => None,
        };
        let pill = widget::container(content)
            .width(Length::Fixed(width))
            .height(Length::Fixed(height))
            .padding([pad, pad + 2.0])
            .align_y(Alignment::Center)
            .class(island(height / 2.0, glow, self.hovered, false));
        let pill = widget::mouse_area(pill)
            .on_press(Message::TogglePopup)
            .on_middle_press(Message::Control(Control::PlayPause))
            .on_enter(Message::Hover(true))
            .on_exit(Message::Hover(false));

        let mut items: Vec<Element<'_, Message>> = vec![pill.into()];
        if let Some(timer) = self.bubble() {
            let bubble_width = self.bubble_width(height);
            let content: Element<'_, Message> = if horizontal {
                widget::row::with_children(vec![
                    glyph("alarm-symbolic", (inner * 0.7) as u16, ORANGE),
                    label(timer::clock(timer.left()), 12, ORANGE)
                        .font(cosmic::font::semibold())
                        .into(),
                ])
                .spacing(3)
                .align_y(Alignment::Center)
                .into()
            } else {
                glyph("alarm-symbolic", (inner * 0.8) as u16, ORANGE)
            };
            let bubble = widget::container(content)
                .width(Length::Fixed(bubble_width))
                .height(Length::Fixed(height))
                .center_x(Length::Fixed(bubble_width))
                .center_y(Length::Fixed(height))
                .class(island(height / 2.0, None, false, false));
            items.push(widget::mouse_area(bubble).on_press(Message::TogglePopup).into());
        }
        let items: Element<'_, Message> = if horizontal {
            widget::row::with_children(items)
                .spacing(6)
                .align_y(Alignment::Center)
                .into()
        } else {
            widget::column::with_children(items)
                .spacing(6)
                .align_x(Alignment::Center)
                .into()
        };
        let (x, y) = if horizontal {
            (major, minor)
        } else {
            (minor, major)
        };
        let body = widget::container(items)
            .padding([f32::from(y) - (height - f32::from(applet.suggested_size(true).0)) / 2.0, f32::from(x)])
            .center_y(Length::Shrink);
        applet.autosize_window(body).into()
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        if self.popup != Some(id) {
            return widget::text("").into();
        }
        let mut cards: Vec<Element<'_, Message>> = Vec::with_capacity(3);
        if let Some(flash) = self.flash()
            && flash == Flash::TimerDone
        {
            cards.push(self.timer_done_card());
        }
        cards.push(match self.shown_media() {
            Some(media) => self.media_card(media),
            None => self.empty_card(),
        });
        cards.push(self.timer_card());
        if let Some(battery) = self.battery.filter(|b| b.charging) {
            cards.push(battery_card(battery));
        }
        let content = widget::column::with_children(cards)
            .spacing(10)
            .padding(MARGIN)
            .width(Length::Fill);
        cosmic::widget::autosize::autosize(
            Expand::new(content),
            cosmic::widget::Id::new("cosmic-applet-autosize"),
        )
        .limits(popup_limits())
        .into()
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::Closed(id))
    }
}

/// A cover, rounded, or a note where there's none.
fn cover<'a>(media: &Media, side: f32, radius: f32) -> Element<'a, Message> {
    match &media.art {
        Some(path) => widget::image(widget::image::Handle::from_path(path))
            .width(Length::Fixed(side))
            .height(Length::Fixed(side))
            .content_fit(iced::ContentFit::Cover)
            .border_radius([radius; 4])
            .into(),
        None => widget::container(glyph("audio-x-generic-symbolic", (side * 0.55) as u16, WHITE))
            .center(Length::Fixed(side))
            .class(theme::Container::custom(move |_| widget::container::Style {
                background: Some(iced::Background::Color(alpha(WHITE, 0.14))),
                border: iced::Border {
                    radius: radius.into(),
                    ..Default::default()
                },
                ..Default::default()
            }))
            .into(),
    }
}

fn battery_icon(percentage: f64, charging: bool) -> String {
    let level = ((percentage / 10.0).round() * 10.0).clamp(0.0, 100.0) as u32;
    match (level, charging) {
        (100, true) => "battery-level-100-charged-symbolic".to_owned(),
        (_, true) => format!("battery-level-{level}-charging-symbolic"),
        (_, false) => format!("battery-level-{level}-symbolic"),
    }
}

fn card<'a>(
    content: impl Into<Element<'a, Message>>,
    glow: Option<Color>,
) -> Element<'a, Message> {
    widget::container(content)
        .padding(18)
        .width(Length::Fill)
        .class(island(CARD_RADIUS, glow, false, true))
        .into()
}

fn round_button<'a>(
    icon: &str,
    icon_size: u16,
    side: f32,
    background: Option<Color>,
    color: Color,
    message: Option<Message>,
) -> Element<'a, Message> {
    ghost(
        widget::container(glyph(icon, icon_size, color)).center(Length::Fixed(side)),
        side / 2.0,
        background,
    )
    .padding(0)
    .on_press_maybe(message)
    .into()
}

fn chip<'a>(text: String, selected: bool, message: Message) -> Element<'a, Message> {
    ghost(
        label(text, 13, if selected { WHITE } else { GRAY }).font(cosmic::font::semibold()),
        14.0,
        Some(alpha(WHITE, if selected { 0.18 } else { 0.08 })),
    )
    .padding([6, 12])
    .on_press(message)
    .into()
}

fn battery_card<'a>(battery: Battery) -> Element<'a, Message> {
    let t = strings();
    let mut about = vec![
        label(t.battery, 12, GRAY).into(),
        label(
            if battery.full { t.charged.to_owned() } else { t.charging.to_owned() },
            15,
            WHITE,
        )
        .font(cosmic::font::semibold())
        .into(),
    ];
    if let Some(seconds) = battery.time_to_full {
        let (h, m) = (seconds / 3600, seconds / 60 % 60);
        let left = if h > 0 { format!("{h} h {m} min") } else { format!("{m} min") };
        about.push(label(format!("{left} {}", t.until_full), 12, GRAY).into());
    }
    card(
        widget::row::with_children(vec![
            glyph(&battery_icon(battery.percentage, true), 28, GREEN),
            widget::column::with_children(about).spacing(2).width(Length::Fill).into(),
            label(format!("{:.0}%", battery.percentage), 26, GREEN)
                .font(cosmic::font::semibold())
                .into(),
        ])
        .spacing(14)
        .align_y(Alignment::Center),
        None,
    )
}

impl App {
    fn media_card<'a>(&'a self, media: &'a Media) -> Element<'a, Message> {
        let color = tint(media.color);
        let art = widget::mouse_area(cover(media, 72.0, 16.0))
            .on_press(Message::Control(Control::Raise));
        let mut source = vec![];
        // The player's icon, when the theme has one.
        if let Some(path) = media
            .desktop_entry
            .as_deref()
            .and_then(|entry| widget::icon::from_name(entry).size(14).path())
        {
            source.push(
                widget::icon::icon(widget::icon::from_path(path))
                    .size(14)
                    .into(),
            );
        }
        source.push(label(media.player.clone(), 12, GRAY).ellipsize(one_line()).into());
        let about = widget::column::with_children(vec![
            label(media.title.clone(), 16, WHITE)
                .font(cosmic::font::semibold())
                .ellipsize(one_line())
                .into(),
            label(
                if media.artist.is_empty() { strings().paused.to_owned() } else { media.artist.clone() },
                14,
                if media.artist.is_empty() { GRAY } else { alpha(WHITE, 0.7) },
            )
            .ellipsize(one_line())
            .into(),
            widget::row::with_children(source)
                .spacing(5)
                .align_y(Alignment::Center)
                .into(),
        ])
        .spacing(2)
        .width(Length::Fill);
        let top = widget::row::with_children(vec![
            art.into(),
            about.into(),
            widget::container(Bars::new(color, media.playing, 26.0).count(6))
                .align_y(Alignment::Start)
                .height(Length::Fixed(72.0))
                .padding([4, 0])
                .into(),
        ])
        .spacing(14)
        .align_y(Alignment::Center);

        let mut rows: Vec<Element<'a, Message>> = vec![top.into()];
        if media.length > 0 {
            let length = media.length as f32;
            let position = self.seeking.unwrap_or(media.position_now() as f32).min(length);
            let slider = widget::slider(0.0..=length, position, Message::Seek)
                .on_release(Message::SeekDone)
                .width(Length::Fill)
                .class(widgets::progress_class(color));
            rows.push(
                widget::row::with_children(vec![
                    label(time(position), 12, GRAY)
                        .width(Length::Fixed(40.0))
                        .into(),
                    slider.into(),
                    label(format!("-{}", time(length - position)), 12, GRAY)
                        .width(Length::Fixed(44.0))
                        .align_x(Alignment::End)
                        .into(),
                ])
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        rows.push(
            widget::row::with_children(vec![
                widget::space::horizontal().into(),
                round_button(
                    "media-skip-backward-symbolic",
                    24,
                    48.0,
                    None,
                    WHITE,
                    media.can_previous.then_some(Message::Control(Control::Previous)),
                ),
                round_button(
                    if media.playing {
                        "media-playback-pause-symbolic"
                    } else {
                        "media-playback-start-symbolic"
                    },
                    32,
                    60.0,
                    None,
                    WHITE,
                    Some(Message::Control(Control::PlayPause)),
                ),
                round_button(
                    "media-skip-forward-symbolic",
                    24,
                    48.0,
                    None,
                    WHITE,
                    media.can_next.then_some(Message::Control(Control::Next)),
                ),
                widget::space::horizontal().into(),
            ])
            .spacing(24)
            .align_y(Alignment::Center)
            .into(),
        );
        // More than one player: each by name, to pick from.
        if self.media.len() > 1 {
            let chips = self
                .media
                .iter()
                .map(|m| {
                    chip(
                        m.player.clone(),
                        m.service == media.service,
                        Message::SelectPlayer(m.service.clone()),
                    )
                })
                .collect::<Vec<_>>();
            rows.push(
                widget::container(widget::row::with_children(chips).spacing(6).wrap())
                    .center_x(Length::Fill)
                    .into(),
            );
        }
        card(
            widget::column::with_children(rows).spacing(14),
            media.playing.then_some(color).filter(|_| media.color.is_some()),
        )
    }

    fn empty_card(&self) -> Element<'_, Message> {
        let t = strings();
        card(
            widget::row::with_children(vec![
                widget::container(glyph("audio-x-generic-symbolic", 22, GRAY))
                    .center(Length::Fixed(44.0))
                    .class(theme::Container::custom(|_| widget::container::Style {
                        background: Some(iced::Background::Color(alpha(WHITE, 0.1))),
                        border: iced::Border {
                            radius: 12.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }))
                    .into(),
                widget::column::with_children(vec![
                    label(t.nothing_playing, 15, WHITE).font(cosmic::font::semibold()).into(),
                    label(t.nothing_playing_about, 12, GRAY).into(),
                ])
                .spacing(2)
                .width(Length::Fill)
                .into(),
            ])
            .spacing(14)
            .align_y(Alignment::Center),
            None,
        )
    }

    fn timer_card(&self) -> Element<'_, Message> {
        let t = strings();
        let Some(timer) = &self.timer else {
            let presets = PRESETS
                .iter()
                .map(|&minutes| chip(format!("{minutes} min"), false, Message::StartTimer(minutes)))
                .collect::<Vec<_>>();
            return card(
                widget::column::with_children(vec![
                    widget::row::with_children(vec![
                        glyph("alarm-symbolic", 16, ORANGE),
                        label(t.start_timer, 13, GRAY).into(),
                    ])
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
                    widget::row::with_children(presets).spacing(6).into(),
                ])
                .spacing(12),
                None,
            );
        };
        let running = timer.running();
        card(
            widget::column::with_children(vec![
                widget::row::with_children(vec![
                    widget::column::with_children(vec![
                        label(if running { t.timer } else { t.paused }, 12, GRAY).into(),
                        label(timer::clock(timer.left()), 34, ORANGE)
                            .font(cosmic::font::semibold())
                            .into(),
                    ])
                    .width(Length::Fill)
                    .into(),
                    ghost(label("+1:00", 13, ORANGE).font(cosmic::font::semibold()), 18.0, Some(alpha(ORANGE, 0.16)))
                        .padding([10, 12])
                        .on_press(Message::AddMinute)
                        .into(),
                    round_button(
                        if running { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" },
                        20,
                        44.0,
                        Some(alpha(ORANGE, 0.22)),
                        ORANGE,
                        Some(Message::ToggleTimer),
                    ),
                    round_button(
                        "window-close-symbolic",
                        18,
                        44.0,
                        Some(alpha(WHITE, 0.14)),
                        WHITE,
                        Some(Message::CancelTimer),
                    ),
                ])
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
                widgets::meter(timer.progress(), ORANGE, 5.0),
            ])
            .spacing(12),
            None,
        )
    }

    fn timer_done_card(&self) -> Element<'_, Message> {
        let t = strings();
        card(
            widget::row::with_children(vec![
                glyph("alarm-symbolic", 28, ORANGE),
                widget::column::with_children(vec![
                    label(t.timer_done, 16, ORANGE).font(cosmic::font::semibold()).into(),
                    label(t.timer_done_body, 12, GRAY).into(),
                ])
                .spacing(2)
                .width(Length::Fill)
                .into(),
            ])
            .spacing(14)
            .align_y(Alignment::Center),
            Some(ORANGE),
        )
    }
}
