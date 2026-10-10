//! Chemical-shift sticks with atom-linked labels. Heights are deliberately
//! uniform; the backend supplies shifts, not intensities or a spin simulation.
use super::{Action, Message, Nucleus, Report, site_description};
use iced::advanced::{
    Clipboard, Layout, Renderer as _, Shell, Widget, layout, mouse, renderer,
    widget::{Operation, Tree, tree},
};
use iced::widget::{button, responsive, text};
use iced::{Color, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, touch};

pub(super) const HEIGHT: f32 = 112.;
const MARGIN: f32 = 16.;
const BASELINE: f32 = 80.;
const STICK_TOP: f32 = 38.;
const LABEL_HEIGHT: f32 = 18.;
const LABEL_SIZE: f32 = 10.;
const LABEL_GAP: f32 = 3.;

#[derive(Debug, Clone, Copy)]
struct Axis {
    low: f64,
    high: f64,
}
impl Axis {
    fn new(report: &Report) -> Self {
        let step = if report.nucleus == Nucleus::H1 {
            2.
        } else {
            50.
        };
        let mut axis = Self {
            low: 0.,
            high: if report.nucleus == Nucleus::H1 {
                12.
            } else {
                200.
            },
        };
        for shift in report
            .rows
            .iter()
            .filter_map(|r| r.statistics.as_ref())
            .map(|s| s.median)
            .filter(|s| s.is_finite())
        {
            let low = (shift / step).floor() * step;
            let high = (shift / step).ceil() * step;
            axis.low = axis.low.min(if low.is_finite() { low } else { shift });
            axis.high = axis.high.max(if high.is_finite() { high } else { shift });
        }
        axis
    }
    fn x(self, shift: f64, width: f32) -> f32 {
        let span = self.high - self.low;
        let fraction = if span.is_finite() {
            (self.high - shift) / span
        } else {
            // A finite report can span both ends of f64 without its difference
            // fitting in f64. Scale before subtracting, preserving the axis.
            (self.high / 2. - shift / 2.) / (self.high / 2. - self.low / 2.)
        };
        MARGIN + fraction.clamp(0., 1.) as f32 * (width - 2. * MARGIN).max(0.)
    }
}

#[derive(Debug)]
struct Group {
    indices: Vec<usize>,
    center: f64,
    lane: usize,
}

fn group_label(report: &Report, indices: &[usize]) -> String {
    let mut rows = indices.iter().filter_map(|&index| report.rows.get(index));
    let Some(first) = rows.next() else {
        return "0 sites".into();
    };
    let count = 1 + rows.count();
    if count == 1 {
        format!("#{}", first.atom_id)
    } else {
        format!("{count} sites")
    }
}

fn marker_name(report: &Report, indices: &[usize]) -> String {
    // Native accessibility limits each name to 4096 bytes. A dense marker
    // keeps all owners in its action, while individual result rows provide
    // complete per-site detail instead of an unbounded combined description.
    let rows = indices.iter().filter_map(|&index| report.rows.get(index));
    let count = rows.clone().count();
    let mut description = rows
        .take(3)
        .map(|row| {
            let mut text = site_description(report.nucleus, row);
            if text.len() > 512 {
                let end = text
                    .char_indices()
                    .map(|(i, _)| i)
                    .take_while(|&i| i <= 512)
                    .last()
                    .unwrap_or(0);
                text.truncate(end);
                text.push('…');
            }
            text
        })
        .collect::<Vec<_>>()
        .join("; ");
    if count > 3 {
        description.push_str(&format!(
            "; {} additional sites. Full predictions are in the atom-linked result rows",
            count - 3
        ));
    }
    let help = if count > 1 {
        " Repeated activation cycles nearby sites."
    } else {
        ""
    };
    format!("Predicted shift marker. {description}.{help}")
}

fn groups(report: &Report, axis: Axis, width: f32) -> Vec<Group> {
    let mut shifts: Vec<_> = report
        .rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            row.statistics
                .as_ref()
                .filter(|s| s.median.is_finite())
                .map(|s| (index, s.median))
        })
        .collect();
    shifts.sort_by(|a, b| b.1.total_cmp(&a.1));
    struct Placed {
        start: usize,
        end: usize,
        center: f64,
        lane: usize,
        previous: [f32; 2],
    }
    let mut placed: Vec<Placed> = Vec::new();
    let mut ends = [f32::NEG_INFINITY; 2];
    for (end, &(index, shift)) in shifts.iter().enumerate() {
        let Some(row) = report.rows.get(index) else {
            continue;
        };
        let mut start = end;
        let mut center = shift;
        loop {
            let count = end - start + 1;
            let label = if count == 1 {
                format!("#{}", row.atom_id)
            } else {
                format!("{count} sites")
            };
            let label_width = (crate::app::text_width(&label, LABEL_SIZE) + 6.).min(width);
            let x =
                (axis.x(center, width) - label_width / 2.).clamp(0., (width - label_width).max(0.));
            let previous_ends = ends;
            if let Some((lane, last)) = ends
                .iter_mut()
                .enumerate()
                .find(|(_, last)| x >= **last + LABEL_GAP)
            {
                placed.push(Placed {
                    start,
                    end,
                    center,
                    lane,
                    previous: previous_ends,
                });
                *last = x + label_width;
                break;
            }
            // Two separate labels are preferred even for coincident shifts.
            // Only a group that cannot fit either lane absorbs its nearest
            // predecessor; the short count retains every original owner.
            let Some(previous) = placed.pop() else {
                // Retain the owner even if a future font/layout backend does
                // not admit a first label through the ordinary lane test.
                placed.push(Placed {
                    start,
                    end,
                    center,
                    lane: 0,
                    previous: ends,
                });
                ends[0] = x + label_width;
                break;
            };
            let previous_count = previous.end - previous.start + 1;
            let ratio = previous_count as f64 / (count + previous_count) as f64;
            let delta = previous.center - center;
            center = if delta.is_finite() {
                center + delta * ratio
            } else {
                center * (1. - ratio) + previous.center * ratio
            };
            start = previous.start;
            ends = previous.previous;
        }
    }
    placed
        .into_iter()
        .filter_map(|p| {
            let indices = shifts
                .get(p.start..=p.end)?
                .iter()
                .map(|&(index, _)| index)
                .collect();
            Some(Group {
                indices,
                center: p.center,
                lane: p.lane,
            })
        })
        .collect()
}

pub(super) fn view<'a>(report: &'a Report, selected: &'a [u64]) -> Element<'a, Message> {
    responsive(move |size| view_at_width(report, selected, size.width))
        .height(HEIGHT)
        .into()
}

fn marker_style(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let color = crate::appearance::color(
            crate::appearance::is_dark(theme),
            Color::from_rgb8(19, 135, 116),
        );
        button::Style {
            text_color: color,
            background: matches!(status, button::Status::Hovered | button::Status::Pressed)
                .then(|| color.scale_alpha(if selected { 0.12 } else { 0.07 }).into()),
            ..Default::default()
        }
    }
}

fn view_at_width<'a>(report: &'a Report, selected: &'a [u64], width: f32) -> Element<'a, Message> {
    let axis = Axis::new(report);
    let mut children = Vec::new();
    let mut positions = Vec::new();
    for group in groups(report, axis, width) {
        let Some(first) = group.indices.first().and_then(|&i| report.rows.get(i)) else {
            continue;
        };
        let selected = group
            .indices
            .iter()
            .filter_map(|&i| report.rows.get(i))
            .any(|r| selected.contains(&r.atom_id));
        let label = group_label(report, &group.indices);
        let button = reshiki::accessibility::button(
            format!("nmr.marker.{}.{}", report.nucleus.code(), first.atom_id),
            marker_name(report, &group.indices),
            text(label)
                .size(LABEL_SIZE)
                .font(iced::Font::with_name(reshiki::style::ui_font_family()))
                .line_height(iced::widget::text::LineHeight::Absolute(13.into()))
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .padding([2, 3])
        .checked(selected)
        .style(marker_style(selected))
        .on_press(Message::Nmr(Action::SelectMarker(group.indices)));
        children.push(button.into());
        positions.push((group.center, group.lane as f32 * LABEL_HEIGHT));
    }
    for index in 0..=4 {
        let shift = axis.low + (axis.high - axis.low) * index as f64 / 4.;
        children.push(text(format!("{shift:.0}")).size(10).into());
        positions.push((shift, 86.));
    }
    children.push(text("δ / ppm").size(9).into());
    positions.push((axis.low, 100.));
    Element::new(Plot {
        report,
        selected,
        axis,
        children,
        positions,
    })
}

struct Plot<'a> {
    report: &'a Report,
    selected: &'a [u64],
    axis: Axis,
    children: Vec<Element<'a, Message>>,
    positions: Vec<(f64, f32)>,
}
#[derive(Default)]
struct State {
    pressed: Option<(Vec<usize>, Option<touch::Finger>)>,
}
impl Plot<'_> {
    fn hit(&self, point: Point, bounds: Rectangle) -> Vec<usize> {
        if !bounds.contains(point)
            || point.y < bounds.y + STICK_TOP
            || point.y > bounds.y + BASELINE + 4.
        {
            return vec![];
        }
        self.report
            .rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                let shift = row.statistics.as_ref()?.median;
                (shift.is_finite()
                    && (self.axis.x(shift, bounds.width) + bounds.x - point.x).abs() <= 9.)
                    .then_some(index)
            })
            .collect()
    }
}
impl Widget<Message, Theme, Renderer> for Plot<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(HEIGHT))
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.resolve(Length::Fill, HEIGHT, Size::ZERO);
        let children = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(&self.positions)
            .map(|((child, tree), &(shift, y))| {
                let node = child.as_widget_mut().layout(
                    tree,
                    renderer,
                    &layout::Limits::new(Size::ZERO, Size::new(size.width, LABEL_HEIGHT)),
                );
                let x = (self.axis.x(shift, size.width) - node.size().width / 2.)
                    .clamp(0., (size.width - node.size().width).max(0.));
                node.move_to(Point::new(x, y))
            })
            .collect();
        layout::Node::with_children(size, children)
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };
        renderer.with_layer(clip, |renderer| {
            let color = theme.palette().text.scale_alpha(0.5);
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: bounds.x + MARGIN,
                        y: bounds.y + BASELINE,
                        width: (bounds.width - 2. * MARGIN).max(0.),
                        height: 1.,
                    },
                    ..Default::default()
                },
                color,
            );
            for row in &self.report.rows {
                let Some(shift) = row
                    .statistics
                    .as_ref()
                    .map(|s| s.median)
                    .filter(|s| s.is_finite())
                else {
                    continue;
                };
                let selected = self.selected.contains(&row.atom_id);
                let width = if selected { 2.5 } else { 1.5 };
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: bounds.x + self.axis.x(shift, bounds.width) - width / 2.,
                            y: bounds.y + STICK_TOP,
                            width,
                            height: BASELINE - STICK_TOP,
                        },
                        ..Default::default()
                    },
                    crate::appearance::color(
                        crate::appearance::is_dark(theme),
                        Color::from_rgb8(19, 135, 116),
                    ),
                );
            }
            for index in 0..=4 {
                let shift = self.axis.low + (self.axis.high - self.axis.low) * index as f64 / 4.;
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: bounds.x + self.axis.x(shift, bounds.width),
                            y: bounds.y + BASELINE,
                            width: 1.,
                            height: 4.,
                        },
                        ..Default::default()
                    },
                    color,
                );
            }
            for ((child, tree), layout) in self
                .children
                .iter()
                .zip(&tree.children)
                .zip(layout.children())
            {
                child
                    .as_widget()
                    .draw(tree, renderer, theme, style, layout, cursor, viewport);
            }
        });
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        for ((child, tree), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child
                .as_widget_mut()
                .operate(tree, layout, renderer, operation);
        }
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        for ((child, tree), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                tree, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
            if shell.is_event_captured() {
                return;
            }
        }
        let state = tree.state.downcast_mut::<State>();
        let press = match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                cursor.position().map(|p| (p, None))
            }
            Event::Touch(touch::Event::FingerPressed { id, position }) => {
                Some((*position, Some(*id)))
            }
            _ => None,
        };
        if let Some((point, finger)) = press {
            let indices = self.hit(point, layout.bounds());
            if !indices.is_empty() {
                state.pressed = Some((indices, finger));
                shell.capture_event();
            }
        }
        let release = match event {
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                cursor.position().map(|p| (p, None))
            }
            Event::Touch(touch::Event::FingerLifted { id, position }) => {
                Some((*position, Some(*id)))
            }
            _ => None,
        };
        if let Some((point, finger)) = release
            && let Some((indices, owner)) = state.pressed.take()
        {
            if owner == finger && indices == self.hit(point, layout.bounds()) {
                shell.publish(Message::Nmr(Action::SelectMarker(indices)));
            }
            shell.capture_event();
        }
        if matches!(
            event,
            Event::Window(iced::window::Event::Unfocused)
                | Event::Touch(touch::Event::FingerLost { .. })
        ) {
            state.pressed = None;
        }
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, tree), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(tree, layout, cursor, viewport, renderer)
            })
            .find(|&i| i != mouse::Interaction::None)
            .unwrap_or_else(|| {
                if cursor
                    .position()
                    .is_some_and(|p| !self.hit(p, layout.bounds()).is_empty())
                {
                    mouse::Interaction::Pointer
                } else {
                    mouse::Interaction::None
                }
            })
    }
}

#[cfg(test)]
mod tests;
