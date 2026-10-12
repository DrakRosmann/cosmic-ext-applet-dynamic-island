// SPDX-License-Identifier: GPL-3.0-only

//! The island: what's going on, music, a timer, the charger plugged in, in
//! the panel, opening into its live activities, drawn as COSMIC draws its own
//! applets.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use cosmic::{
    Element, Task, app,
    applet::{menu_button, padded_control},
    iced::{
        self, Alignment, Length, Subscription,
        core::text::{Ellipsize, EllipsizeHeightLimit},
        platform_specific::shell::commands::popup::destroy_popup,
        window,
    },
    theme,
    widget::{self, button, divider, text},
};

use crate::{
    i18n::strings,
    media::{self, Control, Media},
    power::{self, Battery},
    recording::{self, Recorder},
    timer::{self, Timer},
    widgets::{Bars, accent, accent_of, destructive_of, icon, success_of, tinted, warning_of},
};

/// A paused player stays on the island this long, then leaves it.
const LINGER: Duration = Duration::from_secs(5 * 60);
/// How long the island shows the charger plugged in, or the timer done.
const FLASH_CHARGING: Duration = Duration::from_secs(4);
const FLASH_TIMER: Duration = Duration::from_secs(10);
const PRESETS: [u64; 4] = [1, 5, 10, 25];
/// Room for the title in the panel.
const TITLE_WIDTH: f32 = 160.0;
const COVER: f32 = 56.0;

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
    /// Apps recording from a microphone, and since when.
    recorders: Vec<(Recorder, Instant)>,
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
    Recording(&'a Recorder, Instant),
    Media(&'a Media),
    Timer(&'a Timer),
    Idle,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    Closed(window::Id),
    Media(Vec<Media>),
    Control(Control),
    SelectPlayer(String),
    Seek(f32),
    SeekDone,
    Battery(Option<Battery>),
    Recording(Vec<Recorder>),
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

fn one_line() -> Ellipsize {
    Ellipsize::End(EllipsizeHeightLimit::Lines(1))
}

/// How long since, as a stopwatch: 0:07, 1:02:03.
fn since(start: Instant) -> String {
    let seconds = start.elapsed().as_secs();
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// 3:07, from microseconds.
fn time(micros: f32) -> String {
    let seconds = (micros.max(0.0) / 1_000_000.0) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn island_icon() -> widget::icon::Handle {
    widget::icon::from_svg_bytes(
        &include_bytes!(
            "../data/icons/scalable/apps/io.github.DrakRosmann.CosmicExtAppletDynamicIsland-symbolic.svg"
        )[..],
    )
    .symbolic(true)
}

fn battery_icon(percentage: f64, charging: bool) -> String {
    let level = ((percentage / 10.0).round() * 10.0).clamp(0.0, 100.0) as u32;
    match (level, charging) {
        (100, true) => "battery-level-100-charged-symbolic".to_owned(),
        (_, true) => format!("battery-level-{level}-charging-symbolic"),
        (_, false) => format!("battery-level-{level}-symbolic"),
    }
}

fn divided<'a>() -> Element<'a, Message> {
    let space = theme::active().cosmic().spacing;
    padded_control(divider::horizontal::default())
        .padding([space.space_xxs, space.space_s])
        .into()
}

/// A cover, rounded as the theme rounds things, or a note where there's none.
/// Small ones, in the panel, less round.
fn cover<'a>(media: &Media, side: f32) -> Element<'a, Message> {
    let radii = theme::active().cosmic().corner_radii;
    let radius = if side < 32.0 { radii.radius_xs } else { radii.radius_s };
    match &media.art {
        Some(path) => widget::image(widget::image::Handle::from_path(path))
            .width(Length::Fixed(side))
            .height(Length::Fixed(side))
            .content_fit(iced::ContentFit::Cover)
            .border_radius(radius)
            .into(),
        None => widget::container(icon("audio-x-generic-symbolic", (side * 0.5) as u16))
            .center(Length::Fixed(side))
            .class(theme::Container::Card)
            .into(),
    }
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
        // Someone listening comes first, as the phone's orange dot.
        if let Some((recorder, since)) = self.recorders.first() {
            return Activity::Recording(recorder, *since);
        }
        match (self.shown_media().filter(|m| self.is_live(m)), &self.timer) {
            // A running timer before paused music, as the phone does.
            (Some(media), Some(timer)) if !media.playing && timer.running() => Activity::Timer(timer),
            (Some(media), _) => Activity::Media(media),
            (None, Some(timer)) => Activity::Timer(timer),
            (None, None) => Activity::Idle,
        }
    }

    /// The timer, when the island shows something else: beside it.
    fn side_timer(&self) -> Option<&Timer> {
        match self.activity() {
            Activity::Timer(_) => None,
            _ => self.timer.as_ref(),
        }
    }

    /// What the panel shows, and how wide it is, to center the popup under
    /// it. Each part has a set width for that.
    fn panel_parts(&self) -> (Vec<Element<'_, Message>>, f32) {
        let applet = &self.core.applet;
        let size = applet.suggested_size(true).0;
        let side = f32::from(size);
        let horizontal = applet.is_horizontal();
        let t = strings();
        let mut parts: Vec<(Element<'_, Message>, f32)> = Vec::with_capacity(4);
        let label = |text: String, width: f32| -> (Element<'_, Message>, f32) {
            (
                applet
                    .text(text)
                    .ellipsize(one_line())
                    .width(Length::Fixed(width))
                    .into(),
                width,
            )
        };
        // Wide enough for 59:59, or 9:59:59.
        let clock_width = |timer: &Timer| if timer.left().as_secs() >= 3600 { 64.0 } else { 44.0 };

        match self.activity() {
            Activity::Idle => parts.push((
                widget::icon::icon(island_icon()).size(size).into(),
                side,
            )),
            Activity::Recording(_, since) => {
                parts.push((tinted("media-record-symbolic", size, destructive_of), side));
                if horizontal {
                    let width = if since.elapsed().as_secs() >= 3600 { 64.0 } else { 44.0 };
                    parts.push(label(self::since(since), width));
                }
            }
            Activity::Media(media) => {
                parts.push((cover(media, side), side));
                if horizontal {
                    parts.push(label(media.title.clone(), TITLE_WIDTH));
                }
                parts.push((Bars::new(accent(), media.playing, side).into(), side));
            }
            Activity::Timer(timer) => {
                parts.push((tinted("alarm-symbolic", size, warning_of), side));
                if horizontal {
                    parts.push(label(timer::clock(timer.left()), clock_width(timer)));
                }
            }
            Activity::Flash(Flash::Charging) => {
                let percentage = self.battery.map_or(0.0, |b| b.percentage);
                parts.push((tinted(&battery_icon(percentage, true), size, success_of), side));
                if horizontal {
                    parts.push(label(format!("{} · {percentage:.0}%", t.charging), 130.0));
                }
            }
            Activity::Flash(Flash::TimerDone) => {
                parts.push((tinted("alarm-symbolic", size, accent_of), side));
                if horizontal {
                    parts.push(label(t.timer_done.to_owned(), 120.0));
                }
            }
        }
        if let Some(timer) = self.side_timer() {
            parts.push((tinted("alarm-symbolic", size, warning_of), side));
            if horizontal {
                parts.push(label(timer::clock(timer.left()), clock_width(timer)));
            }
        }
        let spacing = f32::from(theme::active().cosmic().spacing.space_xs);
        let width = parts.iter().map(|(_, w)| w).sum::<f32>()
            + spacing * parts.len().saturating_sub(1) as f32;
        (parts.into_iter().map(|(element, _)| element).collect(), width)
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
        (
            Self {
                core,
                ..Self::default()
            },
            Task::batch(tasks),
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
                // Under the middle of the applet, however wide it is.
                let applet = &self.core.applet;
                let width = self.panel_parts().1;
                let icon = f32::from(applet.suggested_size(true).0);
                let major = i32::from(applet.suggested_padding(true).0);
                let half = major + ((width - icon) / 2.0).round() as i32;
                let horizontal = applet.is_horizontal();
                return cosmic::surface::surface_task(cosmic::surface::action::app_popup(
                    |_: &Self| Default::default(),
                    move |app: &mut Self| {
                        app.core.applet.get_popup_settings(
                            parent,
                            id,
                            None,
                            horizontal.then_some(half),
                            None,
                        )
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
                    let started = media.iter().any(|m| {
                        m.playing && &m.service != selected && !was_playing.contains(&&m.service)
                    });
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
            Message::Recording(recorders) => {
                // Each keeps the time it started at.
                let now = Instant::now();
                self.recorders = recorders
                    .into_iter()
                    .map(|recorder| {
                        let since = self
                            .recorders
                            .iter()
                            .find(|(r, _)| r.key == recorder.key)
                            .map_or(now, |(_, since)| *since);
                        (recorder, since)
                    })
                    .collect();
            }
            Message::StartTimer(minutes) => {
                self.timer = Some(Timer::start(Duration::from_secs(minutes * 60)));
                if self.flash() == Some(Flash::TimerDone) {
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
            recording::subscription().map(Message::Recording),
        ];
        let lingering = self
            .paused_since
            .values()
            .any(|since| since.elapsed() < LINGER);
        let progressing = self.popup.is_some() && self.shown_media().is_some_and(|m| m.playing);
        let every = if self.timer.is_some_and(|t| t.running()) {
            Some(Duration::from_millis(250))
        } else if self.flash.is_some() || progressing || !self.recorders.is_empty() {
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
        let spacing = theme::active().cosmic().spacing.space_xs;
        let (parts, _) = self.panel_parts();
        let content: Element<'_, Message> = if applet.is_horizontal() {
            widget::row::with_children(parts)
                .spacing(spacing)
                .align_y(Alignment::Center)
                .into()
        } else {
            widget::column::with_children(parts)
                .spacing(spacing)
                .align_x(Alignment::Center)
                .into()
        };
        let padding = if applet.is_horizontal() {
            [minor, major]
        } else {
            [major, minor]
        };
        let button = button::custom(content)
            .padding(padding)
            .class(theme::Button::AppletIcon)
            .on_press_down(Message::TogglePopup);
        // The middle button plays or pauses, as on other players' icons.
        let area = widget::mouse_area(button).on_middle_press(Message::Control(Control::PlayPause));
        applet.autosize_window(area).into()
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        if self.popup != Some(id) {
            return widget::text("").into();
        }
        let mut content = widget::column::with_capacity(8).padding([8, 0]);
        if self.flash() == Some(Flash::TimerDone) {
            content = content.push(self.timer_done_section()).push(divided());
        }
        if !self.recorders.is_empty() {
            content = content.push(self.recording_section()).push(divided());
        }
        content = match self.shown_media() {
            Some(media) => content.extend(self.media_section(media)),
            None => content.push(self.empty_section()),
        };
        content = content.push(divided()).push(self.timer_section());
        if let Some(battery) = self.battery.filter(|b| b.charging) {
            content = content.push(divided()).push(battery_section(battery));
        }
        self.core.applet.popup_container(content).into()
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::Closed(id))
    }
}

fn battery_section<'a>(battery: Battery) -> Element<'a, Message> {
    let t = strings();
    let space = theme::active().cosmic().spacing;
    let mut about = vec![
        text::heading(if battery.full { t.charged } else { t.charging }).into(),
    ];
    if let Some(seconds) = battery.time_to_full {
        let (h, m) = (seconds / 3600, seconds / 60 % 60);
        let left = if h > 0 { format!("{h} h {m} min") } else { format!("{m} min") };
        about.push(text::caption(format!("{left} {}", t.until_full)).into());
    }
    padded_control(
        widget::row::with_children(vec![
            tinted(&battery_icon(battery.percentage, true), 24, success_of),
            widget::column::with_children(about).width(Length::Fill).into(),
            text::title4(format!("{:.0}%", battery.percentage)).into(),
        ])
        .spacing(space.space_s)
        .align_y(Alignment::Center),
    )
    .into()
}

impl App {
    fn media_section<'a>(&'a self, media: &'a Media) -> Vec<Element<'a, Message>> {
        let space = theme::active().cosmic().spacing;
        let t = strings();
        let mut about = vec![
            text::heading(media.title.clone())
                .ellipsize(one_line())
                .into(),
        ];
        if !media.artist.is_empty() {
            about.push(text::body(media.artist.clone()).ellipsize(one_line()).into());
        }
        let source = if media.playing {
            media.player.clone()
        } else {
            format!("{} · {}", media.player, t.paused)
        };
        about.push(text::caption(source).ellipsize(one_line()).into());
        // As the audio applet's player: clicking it brings the player up.
        let header = menu_button(
            widget::row::with_children(vec![
                cover(media, COVER),
                widget::column::with_children(about)
                    .width(Length::Fill)
                    .into(),
                Bars::new(accent(), media.playing, 20.0).into(),
            ])
            .spacing(space.space_s)
            .align_y(Alignment::Center),
        )
        .on_press(Message::Control(Control::Raise));

        let mut rows: Vec<Element<'a, Message>> = vec![header.into()];
        if media.length > 0 {
            let length = media.length as f32;
            let position = self.seeking.unwrap_or(media.position_now() as f32).min(length);
            rows.push(
                padded_control(
                    widget::row::with_children(vec![
                        text::caption(time(position)).width(Length::Fixed(40.0)).into(),
                        widget::slider(0.0..=length, position, Message::Seek)
                            .on_release(Message::SeekDone)
                            .width(Length::Fill)
                            .into(),
                        text::caption(format!("-{}", time(length - position)))
                            .width(Length::Fixed(44.0))
                            .align_x(Alignment::End)
                            .into(),
                    ])
                    .spacing(space.space_xs)
                    .align_y(Alignment::Center),
                )
                .into(),
            );
        }
        let control = |name: &str, message: Option<Message>| -> Element<'a, Message> {
            button::icon(widget::icon::from_name(name).symbolic(true))
                .medium()
                .on_press_maybe(message)
                .into()
        };
        let play = button::icon(
            widget::icon::from_name(if media.playing {
                "media-playback-pause-symbolic"
            } else {
                "media-playback-start-symbolic"
            })
            .symbolic(true),
        )
        .medium()
        .class(theme::Button::Suggested)
        .on_press(Message::Control(Control::PlayPause));
        rows.push(
            padded_control(
                widget::row::with_children(vec![
                    control(
                        "media-skip-backward-symbolic",
                        media.can_previous.then_some(Message::Control(Control::Previous)),
                    ),
                    play.into(),
                    control(
                        "media-skip-forward-symbolic",
                        media.can_next.then_some(Message::Control(Control::Next)),
                    ),
                ])
                .spacing(space.space_l)
                .align_y(Alignment::Center),
            )
            .align_x(Alignment::Center)
            .into(),
        );
        // More than one player: each by name, to pick from.
        if self.media.len() > 1 {
            let players = self
                .media
                .iter()
                .map(|m| {
                    let pick = Message::SelectPlayer(m.service.clone());
                    if m.service == media.service {
                        button::suggested(m.player.clone()).on_press(pick).into()
                    } else {
                        button::standard(m.player.clone()).on_press(pick).into()
                    }
                })
                .collect::<Vec<_>>();
            rows.push(
                padded_control(widget::row::with_children(players).spacing(space.space_xxs).wrap())
                    .align_x(Alignment::Center)
                    .into(),
            );
        }
        rows
    }

    fn empty_section(&self) -> Element<'_, Message> {
        let t = strings();
        let space = theme::active().cosmic().spacing;
        padded_control(
            widget::row::with_children(vec![
                icon("audio-x-generic-symbolic", 24),
                widget::column::with_children(vec![
                    text::heading(t.nothing_playing).into(),
                    text::caption(t.nothing_playing_about).into(),
                ])
                .width(Length::Fill)
                .into(),
            ])
            .spacing(space.space_s)
            .align_y(Alignment::Center),
        )
        .into()
    }

    fn timer_section(&self) -> Element<'_, Message> {
        let t = strings();
        let space = theme::active().cosmic().spacing;
        let Some(timer) = &self.timer else {
            let presets = PRESETS
                .iter()
                .map(|&minutes| {
                    // Sharing the row, each on one line.
                    button::custom(
                        text::body(format!("{minutes} min"))
                            .wrapping(iced::widget::text::Wrapping::None)
                            .align_x(Alignment::Center)
                            .width(Length::Fill),
                    )
                    .class(theme::Button::Standard)
                    .padding([space.space_xxs, space.space_xxs])
                    .width(Length::Fill)
                    .on_press(Message::StartTimer(minutes))
                    .into()
                })
                .collect::<Vec<_>>();
            return padded_control(
                widget::column::with_children(vec![
                    widget::row::with_children(vec![
                        tinted("alarm-symbolic", 16, warning_of),
                        text::body(t.start_timer).into(),
                    ])
                    .spacing(space.space_xs)
                    .align_y(Alignment::Center)
                    .into(),
                    widget::row::with_children(presets).spacing(space.space_xxs).into(),
                ])
                .spacing(space.space_xs),
            )
            .into();
        };
        let running = timer.running();
        padded_control(
            widget::column::with_children(vec![
                widget::row::with_children(vec![
                    tinted("alarm-symbolic", 24, warning_of),
                    widget::column::with_children(vec![
                        text::caption(if running { t.timer } else { t.paused }).into(),
                        text::title3(timer::clock(timer.left())).into(),
                    ])
                    .width(Length::Fill)
                    .into(),
                    button::standard("+1:00").on_press(Message::AddMinute).into(),
                    button::icon(
                        widget::icon::from_name(if running {
                            "media-playback-pause-symbolic"
                        } else {
                            "media-playback-start-symbolic"
                        })
                        .symbolic(true),
                    )
                    .on_press(Message::ToggleTimer)
                    .into(),
                    button::icon(widget::icon::from_name("window-close-symbolic").symbolic(true))
                        .on_press(Message::CancelTimer)
                        .into(),
                ])
                .spacing(space.space_xs)
                .align_y(Alignment::Center)
                .into(),
                widget::determinate_linear(timer.progress())
                    .width(Length::Fill)
                    .into(),
            ])
            .spacing(space.space_xs),
        )
        .into()
    }

    /// Who records, each with its icon and for how long.
    fn recording_section(&self) -> Element<'_, Message> {
        let t = strings();
        let space = theme::active().cosmic().spacing;
        let rows = self
            .recorders
            .iter()
            .map(|(recorder, start)| {
                let app_icon: Element<'_, Message> = match recorder
                    .icon
                    .as_deref()
                    .and_then(|name| widget::icon::from_name(name).size(32).path())
                {
                    Some(path) => widget::icon::icon(widget::icon::from_path(path)).size(32).into(),
                    None => icon("audio-input-microphone-symbolic", 32),
                };
                widget::row::with_children(vec![
                    app_icon,
                    widget::column::with_children(vec![
                        text::heading(recorder.name.clone()).ellipsize(one_line()).into(),
                        text::caption(format!("{} · {}", t.recording, since(*start))).into(),
                    ])
                    .width(Length::Fill)
                    .into(),
                    tinted("media-record-symbolic", 16, destructive_of),
                ])
                .spacing(space.space_s)
                .align_y(Alignment::Center)
                .into()
            })
            .collect::<Vec<_>>();
        padded_control(widget::column::with_children(rows).spacing(space.space_xs)).into()
    }

    fn timer_done_section(&self) -> Element<'_, Message> {
        let t = strings();
        let space = theme::active().cosmic().spacing;
        padded_control(
            widget::row::with_children(vec![
                tinted("alarm-symbolic", 24, accent_of),
                widget::column::with_children(vec![
                    text::heading(t.timer_done).into(),
                    text::caption(t.timer_done_body).into(),
                ])
                .width(Length::Fill)
                .into(),
            ])
            .spacing(space.space_s)
            .align_y(Alignment::Center),
        )
        .into()
    }
}
