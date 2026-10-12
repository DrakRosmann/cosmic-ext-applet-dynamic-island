// SPDX-License-Identifier: GPL-3.0-only

//! The island's own widgets: the bars that dance to the music, the island
//! growing out of the panel as it opens, and the black, glowing look.

use std::{
    rc::Rc,
    sync::LazyLock,
    time::{Duration, Instant},
};

use cosmic::{
    Element, theme,
    iced::{
        self, Background, Border, Color, Event, Length, Point, Rectangle, Shadow, Size,
        Transformation, Vector,
        advanced::{
            Clipboard, Layout, Renderer as _, Shell, Widget, layout, mouse, overlay, renderer,
            widget::{Operation, Tree, tree},
        },
        window,
    },
    widget::{self, button},
};

/// The island is black, whatever the theme, as the phone's camera is.
pub const BLACK: Color = Color::BLACK;
pub const WHITE: Color = Color::WHITE;
pub const GRAY: Color = Color::from_rgb(0.62, 0.62, 0.65);
pub const ORANGE: Color = Color::from_rgb(1.0, 0.62, 0.04);
pub const GREEN: Color = Color::from_rgb(0.2, 0.84, 0.29);

static EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

/// The color of a cover, bright enough to show on black.
pub fn tint(color: Option<[u8; 3]>) -> Color {
    let Some([r, g, b]) = color else {
        return WHITE;
    };
    let [r, g, b] = [r, g, b].map(|c| f32::from(c) / 255.0);
    // Lift dark covers: mix with white until it reads on black.
    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let lift = ((0.55 - luma) / 0.55).clamp(0.0, 0.8);
    let mix = |c: f32| c + (1.0 - c) * lift;
    Color::from_rgb(mix(r), mix(g), mix(b))
}

pub fn alpha(mut color: Color, a: f32) -> Color {
    color.a = a;
    color
}

/// An audio meter as the phone shows it: bars rising and falling, or resting
/// low while paused. Not the sound itself, which a panel can't hear, but its
/// look. The bars redraw themselves; the applet isn't asked to.
pub struct Bars {
    color: Color,
    playing: bool,
    count: usize,
    width: f32,
    height: f32,
}

impl Bars {
    pub fn new(color: Color, playing: bool, height: f32) -> Self {
        Self {
            color,
            playing,
            count: 5,
            width: (height * 0.95).max(14.0),
            height,
        }
    }

    pub fn count(mut self, count: usize) -> Self {
        self.width = self.width / self.count as f32 * count as f32;
        self.count = count;
        self
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
        Size::new(Length::Fixed(self.width), Length::Fixed(self.height))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(
            Length::Fixed(self.width),
            Length::Fixed(self.height),
            Size::new(self.width, self.height),
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
        // Bars as wide as the room between them.
        let bar = (bounds.width / (2.0 * n - 1.0)).min(bounds.height / 3.0);
        let gap = (bounds.width - bar * n) / (n - 1.0).max(1.0);
        for i in 0..self.count {
            let height = (bounds.height * self.level(i, seconds)).max(bar);
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: bounds.x + i as f32 * (bar + gap),
                        y: bounds.center_y() - height / 2.0,
                        width: bar,
                        height,
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

/// The popup growing out of the island in the panel, a spring in its step,
/// as the phone's island opens into its live activity.
pub struct Expand<'a, Message> {
    content: Element<'a, Message>,
}

const EXPAND: Duration = Duration::from_millis(420);

struct Opened(Instant);

impl<'a, Message> Expand<'a, Message> {
    pub fn new(content: impl Into<Element<'a, Message>>) -> Self {
        Self {
            content: content.into(),
        }
    }
}

impl Opened {
    fn progress(&self) -> f32 {
        (self.0.elapsed().as_secs_f32() / EXPAND.as_secs_f32()).min(1.0)
    }
}

/// Past the end and back, as a spring.
fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.3;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

impl<Message> Widget<Message, cosmic::Theme, cosmic::Renderer> for Expand<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Opened>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Opened(Instant::now()))
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let node = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        layout::Node::with_children(node.size(), vec![node])
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &cosmic::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Event::Window(window::Event::RedrawRequested(_)) = event
            && tree.state.downcast_ref::<Opened>().progress() < 1.0
        {
            shell.request_redraw();
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            content(layout),
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut cosmic::Renderer,
        theme: &cosmic::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let progress = tree.state.downcast_ref::<Opened>().progress();
        let draw = |renderer: &mut cosmic::Renderer| {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                content(layout),
                cursor,
                viewport,
            );
        };
        if progress >= 1.0 {
            draw(renderer);
            return;
        }
        // Grown from the top middle, where the island sits.
        let bounds = layout.bounds();
        let scale = 0.35 + 0.65 * ease_out_back(progress);
        let origin = Point::new(bounds.center_x(), bounds.y);
        let transformation = Transformation::translate(origin.x, origin.y)
            * Transformation::scale(scale)
            * Transformation::translate(-origin.x, -origin.y);
        renderer.with_layer(bounds, |renderer| {
            renderer.with_transformation(transformation, draw);
        });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &cosmic::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            content(layout),
            cursor,
            viewport,
            renderer,
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &cosmic::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            content(layout),
            renderer,
            operation,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &cosmic::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, cosmic::Theme, cosmic::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            content(layout),
            renderer,
            viewport,
            translation,
        )
    }
}

fn content(layout: Layout<'_>) -> Layout<'_> {
    layout
        .children()
        .next()
        .expect("laid out with its content")
        .with_virtual_offset(layout.virtual_offset())
}

impl<'a, Message: 'a> From<Expand<'a, Message>> for Element<'a, Message> {
    fn from(expand: Expand<'a, Message>) -> Self {
        Self::new(expand)
    }
}

/// The island's black: a pill in the panel, a rounded card in the popup,
/// glowing in the cover's color when there is one.
pub fn island(
    radius: f32,
    glow: Option<Color>,
    hovered: bool,
    shadow: bool,
) -> theme::Container<'static> {
    theme::Container::custom(move |_| widget::container::Style {
        background: Some(Background::Color(BLACK)),
        text_color: Some(WHITE),
        icon_color: Some(WHITE),
        border: Border {
            radius: radius.into(),
            width: 1.0,
            color: match (glow, hovered) {
                (_, true) => alpha(WHITE, 0.28),
                (Some(glow), false) => alpha(glow, 0.35),
                (None, false) => alpha(WHITE, 0.08),
            },
        },
        shadow: match glow.filter(|_| shadow) {
            Some(glow) => Shadow {
                color: alpha(glow, 0.45),
                offset: Vector::ZERO,
                blur_radius: 16.0,
            },
            None => Shadow::default(),
        },
        snap: true,
    })
}

/// A symbolic icon from the system, in this color.
pub fn glyph<'a, Message: 'a>(name: &str, size: u16, color: Color) -> Element<'a, Message> {
    widget::icon::icon(widget::icon::from_name(name).symbolic(true).handle())
        .size(size)
        .class(theme::Svg::Custom(Rc::new(move |_| iced::widget::svg::Style {
            color: Some(color),
        })))
        .into()
}

/// A button with no frame, lit on hover, on black.
pub fn ghost<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
    radius: f32,
    background: Option<Color>,
) -> button::Button<'a, Message> {
    let style = move |hovered: bool| button::Style {
        background: match (background, hovered) {
            (Some(color), false) => Some(Background::Color(color)),
            (Some(color), true) => Some(Background::Color(Color {
                a: (color.a + 0.08).min(1.0),
                ..color
            })),
            (None, true) => Some(Background::Color(alpha(WHITE, 0.12))),
            (None, false) => None,
        },
        border_radius: radius.into(),
        text_color: Some(WHITE),
        icon_color: Some(WHITE),
        ..button::Style::new()
    };
    button::custom(content).class(theme::Button::Custom {
        active: Box::new(move |_, _| style(false)),
        disabled: Box::new(move |_| button::Style {
            text_color: Some(alpha(WHITE, 0.3)),
            icon_color: Some(alpha(WHITE, 0.3)),
            ..style(false)
        }),
        hovered: Box::new(move |_, _| style(true)),
        pressed: Box::new(move |_, _| style(true)),
    })
}

/// A thin progress rail in the cover's color, that can be dragged.
pub fn progress_class(color: Color) -> theme::iced::Slider {
    use iced::widget::slider;
    let style = move |handle: f32| slider::Style {
        rail: slider::Rail {
            backgrounds: (Background::Color(color), Background::Color(alpha(WHITE, 0.2))),
            width: 5.0,
            border: Border {
                radius: 2.5.into(),
                ..Default::default()
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: handle },
            background: Background::Color(if handle > 0.0 { WHITE } else { Color::TRANSPARENT }),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
        breakpoint: slider::Breakpoint { color },
    };
    theme::iced::Slider::Custom {
        active: Rc::new(move |_| style(0.0)),
        hovered: Rc::new(move |_| style(6.0)),
        dragging: Rc::new(move |_| style(7.0)),
    }
}

/// A rail that fills up, not to be dragged: the timer's.
pub fn meter<'a, Message: 'a>(value: f32, color: Color, height: f32) -> Element<'a, Message> {
    let value = value.clamp(0.0, 1.0);
    let rail = move |color: Color| {
        theme::Container::custom(move |_| widget::container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                radius: (height / 2.0).into(),
                ..Default::default()
            },
            ..Default::default()
        })
    };
    let filled = (value * 1000.0).round() as u16;
    let mut row = widget::row::with_capacity(2);
    if filled > 0 {
        row = row.push(
            widget::container(widget::space::horizontal())
                .width(Length::FillPortion(filled))
                .height(Length::Fixed(height))
                .class(rail(color)),
        );
    }
    if filled < 1000 {
        row = row.push(
            widget::container(widget::space::horizontal())
                .width(Length::FillPortion(1000 - filled))
                .height(Length::Fixed(height)),
        );
    }
    widget::container(row)
        .width(Length::Fill)
        .class(rail(alpha(WHITE, 0.18)))
        .into()
}
