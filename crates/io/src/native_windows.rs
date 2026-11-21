//! Windows print geometry is passed to the isolated Rust platform crate.
//! The editor remains safe Rust; print geometry comes from the same SVG/font
//! conversion as PDF export, so text is printed as vectors in the selected font.
use crate::{document::Document, scene};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use resvg::{tiny_skia, usvg};
use serde_json::{Value, json};
use std::collections::HashMap;
fn transform(value: usvg::Transform) -> [f32; 6] {
    [value.sx, value.ky, value.kx, value.sy, value.tx, value.ty]
}

fn color(paint: &usvg::Paint, opacity: f32) -> Result<[u8; 4], String> {
    let stops = match paint {
        usvg::Paint::Color(c) => {
            return Ok([c.red, c.green, c.blue, (opacity * 255.).round() as u8]);
        }
        // A drawing's gradient fill prints flat, in its average color; a 3D
        // model's shading still asks for a PDF.
        usvg::Paint::LinearGradient(g) if g.id().starts_with(scene::GRADIENT_ID) => g.stops(),
        usvg::Paint::RadialGradient(g) if g.id().starts_with(scene::GRADIENT_ID) => g.stops(),
        _ => return Err("Unsupported native print paint; export a PDF instead.".into()),
    };
    let n = stops.len().max(1) as f32;
    let (r, g, b, a) = stops.iter().fold((0., 0., 0., 0.), |(r, g, b, a), stop| {
        let c = stop.color();
        (
            r + f32::from(c.red) / n,
            g + f32::from(c.green) / n,
            b + f32::from(c.blue) / n,
            a + stop.opacity().get() / n,
        )
    });
    Ok([r, g, b, a * opacity * 255.].map(|v| v.round() as u8))
}

fn collect(
    group: &usvg::Group,
    parent: usvg::Transform,
    parent_opacity: f32,
    output: &mut Vec<Value>,
    sources: &HashMap<String, &[u8]>,
    paper: [u8; 3],
) -> Result<(), String> {
    collect_with_clips(group, parent, parent_opacity, output, sources, paper, &[])
}

fn commands(path: &tiny_skia::Path) -> Vec<Vec<f32>> {
    path.segments()
        .map(|segment| {
            use tiny_skia::PathSegment;
            match segment {
                PathSegment::MoveTo(p) => vec![0., p.x, p.y],
                PathSegment::LineTo(p) => vec![1., p.x, p.y],
                PathSegment::QuadTo(a, b) => vec![2., a.x, a.y, b.x, b.y],
                PathSegment::CubicTo(a, b, c) => vec![3., a.x, a.y, b.x, b.y, c.x, c.y],
                PathSegment::Close => vec![4.],
            }
        })
        .collect()
}

/// Native cells use one polygon in world coordinates. Keep the clipping path
/// and its composed transform, so GDI+ records the same vector intersection in
/// both printer and Office metafile snapshots. Refuse unsupported SVG clips
/// instead of dropping a boundary and exposing hidden content.
fn push_clip(
    clip: &usvg::ClipPath,
    absolute: usvg::Transform,
    clips: &mut Vec<Value>,
) -> Result<(), String> {
    let [usvg::Node::Path(path)] = clip.root().children() else {
        return Err("Unsupported native vector clipping; export a PDF instead.".into());
    };
    clips.push(json!({
        "transform": transform(absolute.pre_concat(clip.transform()).pre_concat(clip.root().transform())),
        "commands": commands(path.data()),
        "even_odd": path.fill().is_some_and(|fill| fill.rule() == usvg::FillRule::EvenOdd),
    }));
    if let Some(clip) = clip.clip_path() {
        push_clip(clip, absolute, clips)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn collect_with_clips(
    group: &usvg::Group,
    parent: usvg::Transform,
    parent_opacity: f32,
    output: &mut Vec<Value>,
    sources: &HashMap<String, &[u8]>,
    paper: [u8; 3],
    inherited: &[Value],
) -> Result<(), String> {
    // Flattened text is a separate subtree: its cached absolute transforms do
    // not include the SVG viewBox. Compose local transforms while traversing.
    let absolute = parent.pre_concat(group.transform());
    let opacity = (parent_opacity * group.opacity().get()).clamp(0., 1.);
    let mut clips = inherited.to_vec();
    if let Some(clip) = group.clip_path() {
        push_clip(clip, absolute, &mut clips)?;
    }
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => {
                collect_with_clips(group, absolute, opacity, output, sources, paper, &clips)?
            }
            usvg::Node::Text(text) => collect_with_clips(
                text.flattened(),
                absolute,
                opacity,
                output,
                sources,
                paper,
                &clips,
            )?,
            usvg::Node::Path(path) if path.is_visible() => {
                let commands = commands(path.data());
                let fill = path
                    .fill()
                    .map(|fill| color(fill.paint(), fill.opacity().get() * opacity))
                    .transpose()?;
                let stroke = path.stroke().map(|stroke| -> Result<Value, String> {
                    Ok(json!({
                        "color": color(stroke.paint(), stroke.opacity().get() * opacity)?,
                        "width": stroke.width().get(),
                        "dashes": stroke.dasharray(),
                        "dash_offset": stroke.dashoffset(),
                        "cap": match stroke.linecap() { usvg::LineCap::Butt => "butt", usvg::LineCap::Round => "round", usvg::LineCap::Square => "square" },
                        "join": match stroke.linejoin() { usvg::LineJoin::Round => "round", usvg::LineJoin::Bevel => "bevel", _ => "miter" },
                        "miter": stroke.miterlimit().get()
                    }))
                }).transpose()?;
                output.push(json!({
                    "kind": "path", "transform": transform(absolute),
                    "commands": commands, "fill": fill, "stroke": stroke, "clips": clips,
                    "even_odd": path.fill().is_some_and(|fill| fill.rule() == usvg::FillRule::EvenOdd)
                }));
            }
            usvg::Node::Image(image) if image.is_visible() => {
                // usvg moves an SVG image's ID to its placement group and
                // leaves the contained image unnamed. Keep that association
                // while using the same resolved size and transform as PNG.
                if let Some(bytes) = sources.get(image.id()).or_else(|| sources.get(group.id())) {
                    output.push(json!({
                        "kind": "metafile", "transform": transform(absolute),
                        "width": image.size().width(), "height": image.size().height(),
                        "data": STANDARD.encode(bytes), "clips": clips,
                    }));
                    continue;
                }
                let bytes = match image.kind() {
                    usvg::ImageKind::PNG(bytes) | usvg::ImageKind::JPEG(bytes) => bytes,
                    _ => return Err("Unsupported native print image; export a PDF instead.".into()),
                };
                output.push(json!({
                    "kind": "image", "transform": transform(absolute),
                    "width": image.size().width(), "height": image.size().height(),
                    "data": STANDARD.encode(bytes.as_slice()), "clips": clips,
                }));
            }
            _ => {}
        }
    }
    // EMF has no soft masks. Paint a transparent drawing's atom-label halo
    // knockout in the paper color over the masked ink instead.
    if let Some(mask) = group.mask() {
        for node in mask.root().children() {
            if let usvg::Node::Group(halo) = node
                && halo.id().starts_with("reshiki-label-halo")
            {
                let start = output.len();
                collect_with_clips(halo, absolute, 1., output, sources, paper, &clips)?;
                let [r, g, b] = paper;
                for value in output.iter_mut().skip(start) {
                    if let Some(fill) = value.get_mut("fill").filter(|fill| fill.is_array()) {
                        *fill = json!([r, g, b, 255]);
                    }
                    if let Some(stroke) = value.get_mut("stroke").and_then(Value::as_object_mut) {
                        stroke.insert("color".into(), json!([r, g, b, 255]));
                    }
                }
            }
        }
    }
    Ok(())
}
fn metafile_sources(doc: &Document) -> HashMap<String, &[u8]> {
    doc.graphics
        .iter()
        .filter_map(|graphic| {
            graphic
                .picture
                .as_ref()
                .and_then(|picture| picture.emf())
                .map(|bytes| (format!("reshiki-picture-{}", graphic.id), bytes))
        })
        .collect()
}

pub fn print_snapshot(doc: &Document) -> Result<Vec<u8>, String> {
    doc.validate()?;
    let layout = doc
        .page_layout
        .as_ref()
        .ok_or("Missing print page layout")?;
    let tree = crate::export::parse_svg(scene::svg_with_background(doc))?;
    let (drawing_lo, _) = scene::bounds(&scene::primitives(doc));
    let scale = crate::style::DEFAULT.points_per_world();
    let mut primitives = Vec::new();
    for index in 0..layout.count() {
        let (lo, _) = layout.bounds(index).ok_or("Invalid print page")?;
        let x = (lo.x - drawing_lo.x) * scale;
        let y = (lo.y - drawing_lo.y) * scale;
        let right = x + layout.width_pt;
        let bottom = y + layout.height_pt;
        let [r, g, b] = doc.canvas_theme.background();
        primitives.push(json!({
            "kind": "path", "transform": [4. / 3., 0., 0., 4. / 3., 0., 0.],
            "commands": [[0., x, y], [1., right, y], [1., right, bottom], [1., x, bottom], [4.]],
            "fill": [r, g, b, 255], "stroke": null, "even_odd": false
        }));
    }
    // Printing uses the portable preview. Original metafile playback is only
    // allowed in the disposable EMF export worker.
    collect(
        tree.root(),
        usvg::Transform::identity(),
        1.,
        &mut primitives,
        &HashMap::new(),
        doc.canvas_theme.background(),
    )?;
    let pages: Result<Vec<_>, String> = (0..layout.count())
        .map(|index| {
            let (lo, _) = layout.bounds(index).ok_or("Invalid print page")?;
            Ok([(drawing_lo.x - lo.x) * scale, (drawing_lo.y - lo.y) * scale])
        })
        .collect();
    let bytes = serde_json::to_vec(&json!({
        "version": 1, "width_pt": layout.width_pt, "height_pt": layout.height_pt,
        "pages": pages?, "primitives": primitives
    }))
    .map_err(|error| error.to_string())?;
    if bytes.len() > 128 * 1024 * 1024 {
        return Err("The Windows print snapshot exceeds 128 MB. Print a smaller selection.".into());
    }
    Ok(bytes)
}

pub fn office_metafile(
    doc: &Document,
    background: crate::export::Background,
) -> Result<Vec<u8>, String> {
    doc.validate()?;
    let (doc, paper) = background.apply(doc);
    let tree = crate::export::parse_svg(scene::svg_on(&doc, paper))?;
    metafile(&tree, &doc)
}

/// Use the figure renderer's resolved geometry, fonts, background and bounds.
pub(crate) fn metafile(tree: &usvg::Tree, doc: &Document) -> Result<Vec<u8>, String> {
    record_metafile(tree, doc, false)
}

pub(crate) fn file_metafile(tree: &usvg::Tree, doc: &Document) -> Result<Vec<u8>, String> {
    record_metafile(tree, doc, true)
}

fn partial_transparency(group: &usvg::Group) -> bool {
    (0. ..1.).contains(&group.opacity().get())
        || group.children().iter().any(|node| match node {
            usvg::Node::Group(group) => partial_transparency(group),
            usvg::Node::Text(text) => partial_transparency(text.flattened()),
            usvg::Node::Path(path) => {
                path.fill()
                    .is_some_and(|fill| (0. ..1.).contains(&fill.opacity().get()))
                    || path
                        .stroke()
                        .is_some_and(|stroke| (0. ..1.).contains(&stroke.opacity().get()))
            }
            _ => false,
        })
}

fn record_metafile(tree: &usvg::Tree, doc: &Document, file: bool) -> Result<Vec<u8>, String> {
    if tree.size().width() * 0.75 > 2880. || tree.size().height() * 0.75 > 2880. {
        return Err("Drawing exceeds EMF's supported 40-inch dimensions; use SVG or PDF.".into());
    }
    let mut primitives = Vec::new();
    let sources = metafile_sources(doc);
    collect(
        tree.root(),
        usvg::Transform::identity(),
        1.,
        &mut primitives,
        &sources,
        doc.canvas_theme.background(),
    )?;
    if partial_transparency(tree.root()) {
        return Err("EMF for Office cannot reliably preserve partial transparency. Use SVG, PDF or PNG, or set rear opacity to 0% or 100%.".into());
    }
    let bytes = serde_json::to_vec(&json!({
        "version": 1, "width_pt": tree.size().width() * 0.75,
        "height_pt": tree.size().height() * 0.75,
        "pages": [[0., 0.]], "primitives": primitives
    }))
    .map_err(|e| e.to_string())?;
    if !sources.is_empty() {
        reshiki_windows::isolated_metafile(&bytes, file)
    } else if file {
        reshiki_windows::file_metafile(&bytes)
    } else {
        reshiki_windows::metafile(&bytes)
    }
}

#[cfg(test)]
mod tests;
