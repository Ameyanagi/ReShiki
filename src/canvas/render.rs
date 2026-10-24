//! Tessellates scene primitives into a layered frame; shared by the paper and every preview program.

use super::{Camera, Document, World, dashes, layered, models3d, rgb, text_cache};
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
    for (i, primitive) in primitives.iter().enumerate() {
        match primitive {
            // Each run of model primitives, a fading one included, is drawn
            // once, from its first, as image tiles.
            p if models3d::is_model(p) => {
                if !i
                    .checked_sub(1)
                    .and_then(|j| primitives.get(j))
                    .is_some_and(models3d::is_model)
                {
                    let rest = primitives.get(i..).unwrap_or_default();
                    let run = rest.iter().take_while(|p| models3d::is_model(p)).count();
                    let run = rest.get(..run).unwrap_or_default();
                    models3d::draw(frame, run, camera, bounds, opacity);
                }
            }
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
                let width = if style.width_pt > 0. {
                    (style.width() * camera.zoom).max(minimum)
                } else {
                    0.
                };
                let Some(path) = visible_path(commands, width, camera, bounds) else {
                    continue;
                };
                if *filled && let Some(c) = style.fill {
                    frame.fill(&path, alpha(rgb(c.rgb())));
                }
                let dashes: Vec<_> = style.dashes().iter().map(|v| v * camera.zoom).collect();
                let stroke = Stroke::default()
                    .with_color(alpha(rgb(style.stroke.rgb())))
                    .with_width(width)
                    .with_line_cap(canvas::LineCap::Round)
                    .with_line_join(canvas::LineJoin::Round);
                if dashes.is_empty() {
                    frame.stroke(&path, stroke);
                } else {
                    frame.stroke(&dashes::dashed(&path, &dashes), stroke);
                }
            }
            Primitive::Halo { radius, primitives } => {
                draw_halo(frame, primitives, camera, bounds, *radius)
            }
            Primitive::Sphere { .. }
            | Primitive::ShadedQuad { .. }
            | Primitive::ShadedPolygon { .. }
            | Primitive::Contour { .. }
            | Primitive::Wire { .. }
            | Primitive::Card { .. } => {}
            Primitive::Line(a, b, width) => {
                let width = (*width * camera.zoom).max(minimum);
                if !off_screen([*a, *b], width, camera, bounds) {
                    frame.stroke(
                        &Path::line(camera.screen(*a, bounds), camera.screen(*b, bounds)),
                        Stroke::default()
                            .with_width(width)
                            .with_line_cap(canvas::LineCap::Round)
                            .with_color(alpha(Color::BLACK)),
                    );
                }
            }
            Primitive::Polygon(points) => {
                if off_screen(points.iter().copied(), 0., camera, bounds) {
                    continue;
                }
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

/// Paper-colored label halos: each label shape filled and stroked `radius` wider.
fn draw_halo(
    frame: &mut layered::Frame<'_>,
    primitives: &[Primitive],
    camera: Camera,
    bounds: Rectangle,
    radius: f32,
) {
    let spread = |width: f32| {
        Stroke::default()
            .with_color(Color::WHITE)
            .with_width((width + 2. * radius) * camera.zoom)
            .with_line_cap(canvas::LineCap::Round)
            .with_line_join(canvas::LineJoin::Round)
    };
    for primitive in primitives {
        match primitive {
            Primitive::Opacity { primitive, .. } => draw_halo(
                frame,
                std::slice::from_ref(primitive.as_ref()),
                camera,
                bounds,
                radius,
            ),
            Primitive::Path {
                commands,
                style,
                filled,
            } => {
                let width = if style.width_pt > 0. {
                    style.width()
                } else {
                    0.
                };
                let stroke = spread(width);
                let Some(path) = visible_path(commands, stroke.width, camera, bounds) else {
                    continue;
                };
                if *filled && style.fill.is_some() {
                    frame.fill(&path, Color::WHITE);
                }
                frame.stroke(&path, stroke);
            }
            Primitive::Line(a, b, width) => {
                let stroke = spread(*width);
                if !off_screen([*a, *b], stroke.width, camera, bounds) {
                    frame.stroke(
                        &Path::line(camera.screen(*a, bounds), camera.screen(*b, bounds)),
                        stroke,
                    );
                }
            }
            Primitive::Polygon(points) => {
                if off_screen(points.iter().copied(), spread(0.).width, camera, bounds) {
                    continue;
                }
                let path = Path::new(|builder| {
                    if let Some(p) = points.first() {
                        builder.move_to(camera.screen(*p, bounds));
                        for p in points.iter().skip(1) {
                            builder.line_to(camera.screen(*p, bounds));
                        }
                        builder.close();
                    }
                });
                frame.fill(&path, Color::WHITE);
                frame.stroke(&path, spread(0.));
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
                let at = camera.screen(*position, bounds);
                let pad = spread(0.).width;
                let visible = paths.bounds.is_some_and(|b| {
                    Rectangle {
                        x: b.x + at.x - pad,
                        y: b.y + at.y - pad,
                        width: b.width + 2. * pad,
                        height: b.height + 2. * pad,
                    }
                    .intersects(&Rectangle::with_size(bounds.size()))
                });
                if !visible && cull() {
                    continue;
                }
                let transform =
                    iced::widget::canvas::path::lyon_path::math::Transform::translation(at.x, at.y);
                for (path, _) in &paths.paths {
                    let path = path.transform(&transform);
                    frame.fill(&path, Color::WHITE);
                    frame.stroke(&path, spread(0.));
                }
            }
            Primitive::Picture(_)
            | Primitive::Halo { .. }
            | Primitive::Sphere { .. }
            | Primitive::ShadedQuad { .. }
            | Primitive::ShadedPolygon { .. }
            | Primitive::Contour { .. }
            | Primitive::Wire { .. }
            | Primitive::Card { .. } => {}
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Tests turn culling off to render the unculled reference.
    pub(super) static CULL: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

fn cull() -> bool {
    #[cfg(test)]
    return CULL.with(std::cell::Cell::get);
    #[cfg(not(test))]
    true
}

/// Whether a shape through `points` (a curve's control points bound it), widened
/// by its stroke `width` and a pixel of antialiasing, misses the viewport.
fn off_screen(
    points: impl IntoIterator<Item = World>,
    width: f32,
    camera: Camera,
    bounds: Rectangle,
) -> bool {
    let (mut lo, mut hi) = (
        World::new(f32::MAX, f32::MAX),
        World::new(f32::MIN, f32::MIN),
    );
    for p in points {
        let p = camera.screen(p, bounds);
        (lo.x, lo.y) = (lo.x.min(p.x), lo.y.min(p.y));
        (hi.x, hi.y) = (hi.x.max(p.x), hi.y.max(p.y));
    }
    let pad = width + 1.;
    cull()
        && (hi.x < -pad || hi.y < -pad || lo.x > bounds.width + pad || lo.y > bounds.height + pad)
}

/// The subpaths that can reach the viewport, in screen space, or `None` when
/// none can. One path may hold a whole drawing's bonds; a dropped subpath only
/// adds winding, a stroke or dashes inside its own bounds, which are off screen.
fn visible_path(
    commands: &[PathCommand],
    width: f32,
    camera: Camera,
    bounds: Rectangle,
) -> Option<Path> {
    let mut visible = false;
    let path = Path::new(|b| {
        let subpaths = commands.chunk_by(|_, c| !matches!(c, PathCommand::Move(_)));
        for subpath in subpaths {
            let points = subpath.iter().flat_map(|c| {
                match *c {
                    PathCommand::Move(p) | PathCommand::Line(p) => [None, None, Some(p)],
                    PathCommand::Cubic(a, z, p) => [Some(a), Some(z), Some(p)],
                    PathCommand::Close => [None; 3],
                }
                .into_iter()
                .flatten()
            });
            // A path that does not begin with a move starts from the screen origin.
            let moves = matches!(subpath.first(), Some(PathCommand::Move(_)));
            if moves && off_screen(points, width, camera, bounds) {
                continue;
            }
            visible = true;
            for c in subpath {
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
        }
    });
    visible.then_some(path)
}
