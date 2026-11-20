//! Bounded SVG import as ordinary, editable vector graphics.
//! This deliberately does not read the editable-drawing payload in an SVG.
use crate::{
    document::{Document, Point},
    graphics::{Graphic, GraphicKind, GraphicStyle, PathCommand},
    palette::Color,
    style::DrawingStyle,
};
use quick_xml::{
    Reader, Writer,
    events::{BytesStart, Event},
};
use resvg::{tiny_skia, usvg};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Read,
};

mod css;
mod references;

pub const MAX_BYTES: usize = 8 << 20;
pub const MAX_ELEMENTS: usize = 50_000;
pub const MAX_DEPTH: usize = 64;
pub const MAX_COMMANDS: usize = 20_000;

pub struct Imported {
    pub drawing: Document,
    pub notice: String,
}

#[derive(Default)]
struct Notices(BTreeMap<String, usize>);
impl Notices {
    fn add(&mut self, kind: &str) {
        *self.0.entry(kind.into()).or_default() += 1;
    }
    fn text(&self) -> String {
        let removed = self
            .0
            .iter()
            .map(|(kind, n)| format!("{n} {kind}"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut text = String::from(
            "SVG strokes are editable outlines. Filled paths have a 0.1 pt edge in the same color.",
        );
        if !removed.is_empty() {
            text.push_str(" Changed or left out: ");
            text.push_str(&removed);
            text.push('.');
        }
        if self.0.contains_key("text elements") {
            text.push_str(" Outline text in the drawing program before importing it.");
        }
        text
    }
}

/// Import vectors at their physical SVG size in the destination drawing's style.
pub fn read(bytes: &[u8], style: DrawingStyle) -> Result<Imported, String> {
    if bytes.len() > MAX_BYTES {
        return Err("The SVG file is larger than 8 MB.".into());
    }
    let mut inflated = Vec::new();
    let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
        flate2::read::GzDecoder::new(bytes)
            .take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut inflated)
            .map_err(|e| format!("Could not decompress SVG: {e}"))?;
        if inflated.len() > MAX_BYTES {
            return Err("The decompressed SVG is larger than 8 MB.".into());
        }
        inflated.as_slice()
    } else {
        bytes
    };
    let mut notices = Notices::default();
    let clean = sanitise(bytes, &mut notices)?;
    let xml = roxmltree::Document::parse(&clean).map_err(|e| format!("Could not read SVG: {e}"))?;
    check_uses(&xml)?;
    let options = usvg::Options {
        resources_dir: None,
        conversion_limits: Some(usvg::ConversionLimits {
            max_elements: MAX_ELEMENTS,
            max_path_segments: MAX_COMMANDS,
            max_depth: MAX_DEPTH,
        }),
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree =
        usvg::Tree::from_str(&clean, &options).map_err(|e| match e {
            usvg::Error::ConversionLimitReached => "The SVG expands beyond 50,000 elements, 20,000 path points or 64 levels. Simplify it before importing.".into(),
            _ => format!("Could not read SVG: {e}"),
        })?;
    let mut drawing = Document {
        drawing_style: style,
        ..Default::default()
    };
    let scale = drawing.drawing_style.world(0.75); // 96 SVG px/in, 72 pt/in.
    let mut commands = 0;
    collect(
        tree.root(),
        1.,
        scale,
        &mut drawing,
        &mut commands,
        &mut notices,
    )?;
    if drawing.graphics.is_empty() {
        return Err(format!(
            "The SVG has no supported drawing. {}",
            notices.text()
        ));
    }
    let ids = drawing.all_ids();
    if ids.len() > 1 {
        drawing.group_selection(&ids)?;
    }
    drawing.validate()?;
    Ok(Imported {
        drawing,
        notice: notices.text(),
    })
}

fn removed(name: &[u8]) -> Option<&'static str> {
    match name {
        b"script" => Some("scripts"),
        b"foreignObject" => Some("foreign objects"),
        b"animate" | b"animateMotion" | b"animateTransform" | b"set" => Some("animations"),
        b"filter" => Some("filters"),
        b"mask" => Some("masks"),
        b"clipPath" => Some("clip paths"),
        b"pattern" => Some("patterns"),
        b"image" | b"feImage" => Some("images"),
        b"text" => Some("text elements"),
        _ => None,
    }
}

/// Streaming limits run before either tree parser. Removed subtrees never reach usvg.
fn sanitise(bytes: &[u8], notices: &mut Notices) -> Result<String, String> {
    let mut reader = Reader::from_reader(bytes);
    let mut writer = Writer::new(Vec::new());
    let (mut depth, mut count, mut skipped) = (0usize, 0usize, 0usize);
    loop {
        let event = reader
            .read_event()
            .map_err(|e| format!("Could not read SVG: {e}"))?;
        match event {
            Event::DocType(_) => {
                notices.add("document type declarations");
            }
            Event::Start(ref e) | Event::Empty(ref e) => {
                count += 1;
                if count > MAX_ELEMENTS {
                    return Err("The SVG has more than 50,000 elements.".into());
                }
                let empty = matches!(event, Event::Empty(_));
                if depth + 1 > MAX_DEPTH {
                    return Err("The SVG is nested more than 64 levels deep.".into());
                }
                if !empty {
                    depth += 1;
                }
                if skipped > 0 {
                    if !empty {
                        skipped += 1;
                    }
                    continue;
                }
                if let Some(kind) = removed(e.local_name().as_ref()) {
                    notices.add(kind);
                    if !empty {
                        skipped = 1;
                    }
                    continue;
                }
                let name = std::str::from_utf8(e.name().as_ref())
                    .map_err(|_| "SVG names must be UTF-8")?
                    .to_owned();
                let mut start = BytesStart::new(name);
                for attr in e.attributes() {
                    let attr = attr.map_err(|e| e.to_string())?;
                    let local = attr.key.local_name();
                    if matches!(local.as_ref(), b"filter" | b"mask" | b"clip-path") {
                        continue;
                    }
                    if local.as_ref() == b"href" && !attr.value.starts_with(b"#") {
                        notices.add("external references");
                        continue;
                    }
                    start.push_attribute(attr);
                }
                writer
                    .write_event(if empty {
                        Event::Empty(start)
                    } else {
                        Event::Start(start)
                    })
                    .map_err(|e| e.to_string())?;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if skipped > 0 {
                    skipped -= 1;
                    continue;
                }
                writer.write_event(event).map_err(|e| e.to_string())?;
            }
            Event::Eof => break,
            _ if skipped == 0 => {
                writer.write_event(event).map_err(|e| e.to_string())?;
            }
            _ => {}
        }
    }
    let clean = String::from_utf8(writer.into_inner()).map_err(|_| "SVG must be UTF-8 text.")?;
    let clean = css::sanitise(&clean, notices)?;
    references::remove_unused_use_links(&clean)
}

fn check_uses(xml: &roxmltree::Document<'_>) -> Result<(), String> {
    let ids = references::ids(xml);
    fn size<'a, 'input>(
        node: roxmltree::Node<'a, 'input>,
        ids: &HashMap<&'a str, roxmltree::Node<'a, 'input>>,
        memo: &mut HashMap<roxmltree::NodeId, (usize, usize)>,
        active: &mut HashSet<roxmltree::NodeId>,
        depth: usize,
    ) -> Result<(usize, usize), String> {
        // usvg parses retained definitions and hidden branches before its
        // visibility checks, so the budget includes them as well as live uses.
        if depth > MAX_DEPTH || !active.insert(node.id()) {
            return Err("The SVG has a recursive or deeply nested use reference.".into());
        }
        if let Some(n) = memo.get(&node.id()) {
            active.remove(&node.id());
            if depth + n.1 > MAX_DEPTH {
                return Err("The SVG has deeply nested use references.".into());
            }
            return Ok(*n);
        }
        let mut total = 1usize;
        let mut height = 0usize;
        for child in node.children().filter(|n| n.is_element()) {
            let (n, h) = size(child, ids, memo, active, depth + 1)?;
            total = total.saturating_add(n);
            height = height.max(h + 1);
        }
        if node.has_tag_name("use")
            && let Some(target) = references::href(node).and_then(|id| ids.get(id))
        {
            let (n, h) = size(*target, ids, memo, active, depth + 1)?;
            total = total.saturating_add(n);
            height = height.max(h + 1);
        }
        active.remove(&node.id());
        if total > MAX_ELEMENTS {
            return Err("The SVG expands to more than 50,000 elements.".into());
        }
        if depth + height > MAX_DEPTH {
            return Err("The SVG has deeply nested use references.".into());
        }
        memo.insert(node.id(), (total, height));
        Ok((total, height))
    }
    size(
        xml.root_element(),
        &ids,
        &mut HashMap::new(),
        &mut HashSet::new(),
        0,
    )
    .map(|_| ())
}

fn paint(paint: &usvg::Paint, alpha: f32, notices: &mut Notices) -> Option<([u8; 3], f32)> {
    let (rgb, opacity) = match paint {
        usvg::Paint::Color(c) => ([c.red, c.green, c.blue], 1.),
        usvg::Paint::LinearGradient(g) => midpoint(g.stops(), notices)?,
        usvg::Paint::RadialGradient(g) => midpoint(g.stops(), notices)?,
        usvg::Paint::Pattern(_) => {
            notices.add("pattern paints");
            return None;
        }
    };
    let alpha = (alpha * opacity).clamp(0., 1.);
    if alpha <= 0. {
        return None;
    }
    // Native graphics retain paint alpha, so overlapping paths composite
    // against the drawing and one another rather than an assumed white page.
    let alpha = if alpha < 0.01 {
        notices.add("paint opacities below 1% rounded to 1%");
        0.01
    } else if alpha > 0.99 && alpha < 1. {
        notices.add("paint opacities above 99% rounded to opaque");
        1.
    } else {
        alpha
    };
    Some((rgb, alpha))
}
fn midpoint(stops: &[usvg::Stop], notices: &mut Notices) -> Option<([u8; 3], f32)> {
    notices.add("gradients replaced by their middle color");
    let left = stops
        .iter()
        .rev()
        .find(|s| s.offset().get() <= 0.5)
        .or_else(|| stops.first())?;
    let right = stops
        .iter()
        .find(|s| s.offset().get() > 0.5)
        .unwrap_or(left);
    let gap = right.offset().get() - left.offset().get();
    let t = if gap > 0. {
        ((0.5 - left.offset().get()) / gap).clamp(0., 1.)
    } else {
        0.
    };
    let a = left.color();
    let b = right.color();
    let rgb = [a.red, a.green, a.blue]
        .into_iter()
        .zip([b.red, b.green, b.blue])
        .map(|(a, b)| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8)
        .collect::<Vec<_>>();
    Some((
        [*rgb.first()?, *rgb.get(1)?, *rgb.get(2)?],
        left.opacity().get() + (right.opacity().get() - left.opacity().get()) * t,
    ))
}

fn collect(
    group: &usvg::Group,
    parent_alpha: f32,
    scale: f32,
    drawing: &mut Document,
    commands: &mut usize,
    notices: &mut Notices,
) -> Result<(), String> {
    if group.blend_mode() != usvg::BlendMode::Normal {
        notices.add("blend modes replaced by normal compositing");
    }
    let opacity = group.opacity().get();
    if opacity > 0. && opacity < 1. {
        notices.add("groups with opacity applied to each path (overlaps may look darker)");
    }
    let alpha = parent_alpha * opacity;
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => collect(g, alpha, scale, drawing, commands, notices)?,
            usvg::Node::Path(path) if path.is_visible() => {
                if path.data().segments().take(MAX_COMMANDS + 1).count() > MAX_COMMANDS {
                    return Err(
                        "The SVG has more than 20,000 path points. Simplify it before importing."
                            .into(),
                    );
                }
                let transform =
                    usvg::Transform::from_scale(scale, scale).pre_concat(path.abs_transform());
                let fill = |drawing: &mut Document,
                            commands: &mut usize,
                            notices: &mut Notices|
                 -> Result<(), String> {
                    if let Some(fill) = path.fill()
                        && let Some((rgb, opacity)) =
                            paint(fill.paint(), alpha * fill.opacity().get(), notices)
                    {
                        let data = path
                            .data()
                            .clone()
                            .transform(transform)
                            .ok_or("The SVG has invalid coordinates.")?;
                        let data = if fill.rule() == usvg::FillRule::EvenOdd {
                            even_odd(&data, notices)?
                        } else {
                            Some(data)
                        };
                        if let Some(data) = data {
                            add(&data, rgb, opacity, drawing, commands)?;
                        }
                    }
                    Ok(())
                };
                let stroke = |drawing: &mut Document,
                              commands: &mut usize,
                              notices: &mut Notices|
                 -> Result<(), String> {
                    if let Some(stroke) = path.stroke()
                        && let Some((rgb, opacity)) =
                            paint(stroke.paint(), alpha * stroke.opacity().get(), notices)
                    {
                        let style = stroke.to_tiny_skia();
                        if let Some(dashes) = stroke.dasharray() {
                            let period: f64 = dashes.iter().map(|v| f64::from(*v)).sum();
                            let mut at = tiny_skia::Point::from_xy(0., 0.);
                            let mut length = 0f64;
                            for segment in path.data().segments() {
                                let points = match segment {
                                    tiny_skia::PathSegment::MoveTo(p) => {
                                        at = p;
                                        vec![]
                                    }
                                    tiny_skia::PathSegment::LineTo(p) => vec![p],
                                    tiny_skia::PathSegment::QuadTo(a, p) => vec![a, p],
                                    tiny_skia::PathSegment::CubicTo(a, b, p) => vec![a, b, p],
                                    tiny_skia::PathSegment::Close => vec![],
                                };
                                for p in points {
                                    length += f64::from((p.x - at.x).hypot(p.y - at.y));
                                    at = p;
                                }
                            }
                            if period <= 0.
                                || length / period * dashes.len() as f64 > MAX_COMMANDS as f64 / 8.
                            {
                                return Err("The SVG dashes produce too many path points. Use wider dashes before importing.".into());
                            }
                        }
                        let dashed = match &style.dash {
                            Some(dash) => Some(
                                path.data()
                                    .dash(dash, 1.)
                                    .ok_or("Could not draw SVG dashes.")?,
                            ),
                            None => None,
                        };
                        let source = dashed.as_ref().unwrap_or(path.data());
                        if let Some(outline) = source
                            .stroke(&style, 1.)
                            .and_then(|p| p.transform(transform))
                        {
                            add(&outline, rgb, opacity, drawing, commands)?;
                        }
                    }
                    Ok(())
                };
                if path.paint_order() == usvg::PaintOrder::StrokeAndFill {
                    stroke(drawing, commands, notices)?;
                    fill(drawing, commands, notices)?;
                } else {
                    fill(drawing, commands, notices)?;
                    stroke(drawing, commands, notices)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

// Keep curves for ordinary fills; only even-odd fills need winding normalization.
fn even_odd(
    path: &tiny_skia::Path,
    notices: &mut Notices,
) -> Result<Option<tiny_skia::Path>, String> {
    use crate::equipment::geometry::{Seg, flatten};
    use i_overlay::{
        core::{fill_rule::FillRule, overlay_rule::OverlayRule},
        float::overlay::FloatOverlay,
    };
    notices.add("even-odd fills converted to polygon paths");
    let mut current = [0., 0.];
    let mut first = current;
    let segs: Vec<_> = path
        .segments()
        .map(|segment| {
            let xy = |p: tiny_skia::Point| [f64::from(p.x), f64::from(p.y)];
            let segment = match segment {
                tiny_skia::PathSegment::MoveTo(p) => {
                    first = xy(p);
                    Seg::Move(first)
                }
                tiny_skia::PathSegment::LineTo(p) => Seg::Line(xy(p)),
                tiny_skia::PathSegment::QuadTo(a, p) => {
                    let a = xy(a);
                    let p = xy(p);
                    let towards = |from: [f64; 2]| {
                        [
                            from[0] + (a[0] - from[0]) * 2. / 3.,
                            from[1] + (a[1] - from[1]) * 2. / 3.,
                        ]
                    };
                    Seg::Cubic(towards(current), towards(p), p)
                }
                tiny_skia::PathSegment::CubicTo(a, b, p) => Seg::Cubic(xy(a), xy(b), xy(p)),
                tiny_skia::PathSegment::Close => Seg::Close,
            };
            current = match segment {
                Seg::Move(p) | Seg::Line(p) | Seg::Cubic(_, _, p) => p,
                Seg::Close => first,
            };
            segment
        })
        .collect();
    let contours: Vec<Vec<[f64; 2]>> = flatten(&segs).into_iter().map(|(p, _)| p).collect();
    let shapes =
        FloatOverlay::with_subj(&contours).overlay(OverlayRule::Subject, FillRule::EvenOdd);
    let mut builder = tiny_skia::PathBuilder::new();
    for contour in shapes.into_iter().flatten() {
        if let Some(first) = contour.first() {
            builder.move_to(first[0] as f32, first[1] as f32);
            for p in contour.iter().skip(1) {
                builder.line_to(p[0] as f32, p[1] as f32);
            }
            builder.close();
        }
    }
    Ok(builder.finish())
}

fn add(
    path: &tiny_skia::Path,
    rgb: [u8; 3],
    opacity: f32,
    drawing: &mut Document,
    commands: &mut usize,
) -> Result<(), String> {
    let p = |p: tiny_skia::Point| Point::new(p.x, p.y);
    let mut source = Point::default();
    let mut first = source;
    let mut list = Vec::new();
    for segment in path.segments() {
        let command = match segment {
            tiny_skia::PathSegment::MoveTo(a) => {
                source = p(a);
                first = source;
                PathCommand::Move(source)
            }
            tiny_skia::PathSegment::LineTo(a) => {
                source = p(a);
                PathCommand::Line(source)
            }
            tiny_skia::PathSegment::QuadTo(a, b) => {
                let a = p(a);
                let b = p(b);
                let c = PathCommand::Cubic(source.lerp(a, 2. / 3.), b.lerp(a, 2. / 3.), b);
                source = b;
                c
            }
            tiny_skia::PathSegment::CubicTo(a, b, c) => {
                source = p(c);
                PathCommand::Cubic(p(a), p(b), source)
            }
            tiny_skia::PathSegment::Close => {
                source = first;
                PathCommand::Close
            }
        };
        *commands += 1;
        if *commands > MAX_COMMANDS {
            return Err(
                "The SVG drawing has more than 20,000 path points. Simplify it before importing."
                    .into(),
            );
        }
        list.push(command);
    }
    let mut graphic = Graphic::dragged(
        drawing.next_id(),
        GraphicKind::Path,
        Point::default(),
        Point::new(1., 1.),
        GraphicStyle {
            stroke: Color::Custom(rgb),
            fill: Some(Color::Custom(rgb)),
            width_pt: 0.1,
            effects: crate::graphics::Effects {
                opacity: (opacity < 1.).then_some(opacity),
                ..Default::default()
            },
            ..Default::default()
        },
        Default::default(),
        false,
    );
    graphic.origin = Point::default();
    graphic.axis_x = Point::new(1., 0.);
    graphic.axis_y = Point::new(0., 1.);
    graphic.path = list;
    graphic.node_types.clear();
    graphic.validate()?;
    drawing.graphics.push(graphic);
    Ok(())
}

#[cfg(test)]
mod tests;
