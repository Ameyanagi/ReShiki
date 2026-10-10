//! Tessellates scene primitives into a layered frame; shared by the paper and every preview program.

use super::{Camera, Document, World, dashes, layered, rgb, text_cache};
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Rectangle};
use reshiki::{
    graphics::PathCommand,
    scene::{Primitive, primitives},
};

pub(super) fn draw_document(
    frame: &mut layered::Frame<'_>,
    doc: &Document,
    camera: Camera,
    bounds: Rectangle,
) {
    draw_document_with_minimum_stroke(frame, doc, camera, bounds, 0.);
}

pub(super) fn draw_document_with_minimum_stroke(
    frame: &mut layered::Frame<'_>,
    doc: &Document,
    camera: Camera,
    bounds: Rectangle,
    minimum: f32,
) {
    draw_primitives(frame, &primitives(doc), camera, bounds, minimum);
}

pub(super) fn draw_primitives(
    frame: &mut layered::Frame<'_>,
    primitives: &[Primitive],
    camera: Camera,
    bounds: Rectangle,
    minimum: f32,
) {
    draw_primitives_with_opacity(frame, primitives, camera, bounds, minimum, 1.);
}

fn draw_primitives_with_opacity(
    frame: &mut layered::Frame<'_>,
    primitives: &[Primitive],
    camera: Camera,
    bounds: Rectangle,
    minimum: f32,
    opacity: f32,
) {
    let alpha = |mut color: Color| {
        color.a *= opacity;
        color
    };
    for primitive in primitives {
        match primitive {
            Primitive::Opacity { alpha, primitive } => draw_primitives_with_opacity(
                frame,
                std::slice::from_ref(primitive.as_ref()),
                camera,
                bounds,
                minimum,
                opacity * alpha,
            ),
            Primitive::Picture(g) => {
                if let Some(picture) = &g.picture {
                    let flip = g.axis_x.x * g.axis_y.y - g.axis_x.y * g.axis_y.x < 0.;
                    if let Some(handle) = picture.handle(flip) {
                        let width = g.axis_x.distance(World::default()) * camera.zoom;
                        let height = g.axis_y.distance(World::default()) * camera.zoom;
                        let center = camera.screen(
                            g.origin.offset(
                                (g.axis_x.x + g.axis_y.x) / 2.,
                                (g.axis_x.y + g.axis_y.y) / 2.,
                            ),
                            bounds,
                        );
                        frame.split();
                        frame.draw_image(
                            Rectangle {
                                x: center.x - width / 2.,
                                y: center.y - height / 2.,
                                width,
                                height,
                            },
                            canvas::Image::new(handle).rotation(g.axis_x.y.atan2(g.axis_x.x)),
                        );
                        frame.split();
                    }
                }
            }
            Primitive::Path {
                commands,
                style,
                filled,
            } => {
                let path = Path::new(|b| {
                    for c in commands {
                        match c {
                            PathCommand::Move(p) => b.move_to(camera.screen(*p, bounds)),
                            PathCommand::Line(p) => b.line_to(camera.screen(*p, bounds)),
                            PathCommand::Cubic(a, z, p) => b.bezier_curve_to(
                                camera.screen(*a, bounds),
                                camera.screen(*z, bounds),
                                camera.screen(*p, bounds),
                            ),
                            PathCommand::Close => b.close(),
                        }
                    }
                });
                if *filled && let Some(c) = style.fill {
                    frame.fill(&path, alpha(rgb(c.rgb())));
                }
                let dashes: Vec<_> = style.dashes().iter().map(|v| v * camera.zoom).collect();
                let stroke = Stroke::default()
                    .with_color(alpha(rgb(style.stroke.rgb())))
                    .with_width(if style.width_pt > 0. {
                        (style.width() * camera.zoom).max(minimum)
                    } else {
                        0.
                    })
                    .with_line_cap(canvas::LineCap::Round)
                    .with_line_join(canvas::LineJoin::Round);
                if dashes.is_empty() {
                    frame.stroke(&path, stroke);
                } else {
                    frame.stroke(&dashes::dashed(&path, &dashes), stroke);
                }
            }
            Primitive::Line(a, b, width) => frame.stroke(
                &Path::line(camera.screen(*a, bounds), camera.screen(*b, bounds)),
                Stroke::default()
                    .with_width((*width * camera.zoom).max(minimum))
                    .with_line_cap(canvas::LineCap::Round)
                    .with_color(alpha(Color::BLACK)),
            ),
            Primitive::Polygon(points) => {
                let path = Path::new(|builder| {
                    if let Some(p) = points.first() {
                        builder.move_to(camera.screen(*p, bounds));
                        for p in points.iter().skip(1) {
                            builder.line_to(camera.screen(*p, bounds));
                        }
                        builder.close();
                    }
                });
                frame.fill(&path, alpha(Color::BLACK));
            }
            Primitive::Text {
                position,
                text,
                size,
                color,
                style,
            } => {
                let paths = if let Some(cache) = frame.text_cache {
                    cache
                        .borrow_mut()
                        .get(text, *size, camera.zoom, *color, style)
                } else {
                    std::rc::Rc::new(text_cache::outline(text, *size, camera.zoom, *color, style))
                };
                let position_screen = camera.screen(*position, bounds);
                let visible = paths.bounds.is_some_and(|text_bounds| {
                    Rectangle {
                        x: text_bounds.x + position_screen.x,
                        y: text_bounds.y + position_screen.y,
                        ..text_bounds
                    }
                    .intersects(&Rectangle::with_size(bounds.size()))
                });
                if visible {
                    let transform =
                        iced::widget::canvas::path::lyon_path::math::Transform::translation(
                            position_screen.x,
                            position_screen.y,
                        );
                    for (path, color) in &paths.paths {
                        frame.fill(&path.transform(&transform), alpha(*color));
                    }
                }
                if style.underline {
                    let width = reshiki::style::styled_text_width(text, *size, style);
                    frame.stroke(
                        &Path::line(
                            camera.screen(position.offset(0.0, *size * 0.95), bounds),
                            camera.screen(position.offset(width, *size * 0.95), bounds),
                        ),
                        Stroke::default()
                            .with_width((*size * 0.045 * camera.zoom).max(0.5))
                            .with_color(alpha(rgb(*color))),
                    );
                }
            }
        }
    }
}
