// SPDX-License-Identifier: GPL-3.0-only

//! The bars that dance to the music, and icons in the theme's colors.

use std::{
    rc::Rc,
    sync::LazyLock,
    time::{Duration, Instant},
};

use cosmic::{
    Element, theme,
    iced::{
        self, Border, Color, Event, Length, Rectangle, Size,
        advanced::{
            Clipboard, Layout, Renderer as _, Shell, Widget, layout, mouse, renderer,
            widget::Tree,
        },
        window,
    },
    widget,
};

static EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

pub fn accent() -> Color {
    theme::active().cosmic().accent_color().into()
}

pub fn alpha(mut color: Color, a: f32) -> Color {
    color.a = a;
    color
}

/// An audio meter as phones show it: bars rising and falling, or resting
/// low while paused. Not the sound itself, which a panel can't hear, but its
/// look. The bars redraw themselves; the applet isn't asked to.
pub struct Bars {
    color: Color,
    playing: bool,
    count: usize,
    side: f32,
}

impl Bars {
    /// As wide as high: `side` across.
    pub fn new(color: Color, playing: bool, side: f32) -> Self {
        Self {
            color,
            playing,
            count: 4,
            side,
        }
    }

    /// A bar's height, from 0 to 1, at this time: two slow waves apiece so
    /// they never fall in step.
    fn level(&self, bar: usize, seconds: f32) -> f32 {
        if !self.playing {
            return 0.2;
        }
        let i = bar as f32;
        let first = (seconds * (5.1 + i * 1.7) + i * 1.3).sin();
        let second = (seconds * (8.3 - i * 0.9) + i * 2.9).sin();
        (0.55 + 0.28 * first + 0.17 * second).clamp(0.18, 1.0)
    }
}

impl<Message> Widget<Message, cosmic::Theme, cosmic::Renderer> for Bars {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.side), Length::Fixed(self.side))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(
            Length::Fixed(self.side),
            Length::Fixed(self.side),
            Size::new(self.side, self.side),
        ))
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &cosmic::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if self.playing
            && let Event::Window(window::Event::RedrawRequested(now)) = event
        {
            // About 20 frames a second: smooth enough, and light on the panel.
            shell.request_redraw_at(*now + Duration::from_millis(50));
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut cosmic::Renderer,
        _theme: &cosmic::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let seconds = EPOCH.elapsed().as_secs_f32();
        let n = self.count as f32;
        // Bars as wide as the room between them, in the middle 80%.
        let height = bounds.height * 0.8;
        let bar = bounds.width / (2.0 * n - 1.0);
        let gap = (bounds.width - bar * n) / (n - 1.0).max(1.0);
        for i in 0..self.count {
            let h = (height * self.level(i, seconds)).max(bar);
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: bounds.x + i as f32 * (bar + gap),
                        y: bounds.center_y() - h / 2.0,
                        width: bar,
                        height: h,
                    },
                    border: Border {
                        radius: (bar / 2.0).into(),
                        ..Default::default()
                    },
                    ..renderer::Quad::default()
                },
                if self.playing { self.color } else { alpha(self.color, 0.5) },
            );
        }
    }
}

impl<'a, Message: 'a> From<Bars> for Element<'a, Message> {
    fn from(bars: Bars) -> Self {
        Self::new(bars)
    }
}

/// A symbolic icon from the system, in the theme's text color.
pub fn icon<'a, Message: 'a>(name: &str, size: u16) -> Element<'a, Message> {
    widget::icon::from_name(name).symbolic(true).size(size).icon().into()
}

/// A symbolic icon in one of the theme's colors: its accent, warning…
pub fn tinted<'a, Message: 'a>(
    name: &str,
    size: u16,
    color: fn(&cosmic::Theme) -> Color,
) -> Element<'a, Message> {
    widget::icon::icon(widget::icon::from_name(name).symbolic(true).handle())
        .size(size)
        .class(theme::Svg::Custom(Rc::new(move |theme| iced::widget::svg::Style {
            color: Some(color(theme)),
        })))
        .into()
}

pub fn accent_of(theme: &cosmic::Theme) -> Color {
    theme.cosmic().accent_color().into()
}

pub fn warning_of(theme: &cosmic::Theme) -> Color {
    theme.cosmic().warning_color().into()
}

pub fn destructive_of(theme: &cosmic::Theme) -> Color {
    theme.cosmic().destructive_color().into()
}

pub fn success_of(theme: &cosmic::Theme) -> Color {
    theme.cosmic().success_color().into()
}
