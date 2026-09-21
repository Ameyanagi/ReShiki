//! Windows print geometry is passed to the isolated Rust platform crate.
//! The editor remains safe Rust; print geometry comes from the same SVG/font
//! conversion as PDF export, so text is printed as vectors in the selected font.
use crate::{document::Document, scene};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use resvg::{tiny_skia, usvg};
use serde_json::{Value, json};
fn transform(value: usvg::Transform) -> [f32; 6] {
    [value.sx, value.ky, value.kx, value.sy, value.tx, value.ty]
}

fn color(paint: &usvg::Paint, opacity: f32) -> Result<[u8; 4], String> {
    match paint {
        usvg::Paint::Color(c) => Ok([c.red, c.green, c.blue, (opacity * 255.).round() as u8]),
        _ => Err("Unsupported native print paint; export a PDF instead.".into()),
    }
}

fn collect(
    group: &usvg::Group,
    parent: usvg::Transform,
    output: &mut Vec<Value>,
) -> Result<(), String> {
    // Flattened text is a separate subtree: its cached absolute transforms do
    // not include the SVG viewBox. Compose local transforms while traversing.
    let absolute = parent.pre_concat(group.transform());
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => collect(group, absolute, output)?,
            usvg::Node::Text(text) => collect(text.flattened(), absolute, output)?,
            usvg::Node::Path(path) if path.is_visible() => {
                let commands: Vec<Vec<f32>> = path
                    .data()
                    .segments()
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
                    .collect();
                let fill = path
                    .fill()
                    .map(|fill| color(fill.paint(), fill.opacity().get()))
                    .transpose()?;
                let stroke = path.stroke().map(|stroke| -> Result<Value, String> {
                    Ok(json!({
                        "color": color(stroke.paint(), stroke.opacity().get())?,
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
                    "commands": commands, "fill": fill, "stroke": stroke,
                    "even_odd": path.fill().is_some_and(|fill| fill.rule() == usvg::FillRule::EvenOdd)
                }));
            }
            usvg::Node::Image(image) if image.is_visible() => {
                let bytes = match image.kind() {
                    usvg::ImageKind::PNG(bytes) | usvg::ImageKind::JPEG(bytes) => bytes,
                    _ => return Err("Unsupported native print image; export a PDF instead.".into()),
                };
                output.push(json!({
                    "kind": "image", "transform": transform(absolute),
                    "width": image.size().width(), "height": image.size().height(),
                    "data": STANDARD.encode(bytes.as_slice())
                }));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn print_snapshot(doc: &Document) -> Result<Vec<u8>, String> {
    doc.validate()?;
    let layout = doc
        .page_layout
        .as_ref()
        .ok_or("Missing print page layout")?;
    let svg = scene::svg(doc);
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(&svg, &options).map_err(|error| error.to_string())?;
    let mut primitives = Vec::new();
    collect(tree.root(), usvg::Transform::identity(), &mut primitives)?;
    let (drawing_lo, _) = scene::bounds(&scene::primitives(doc));
    let scale = crate::style::DEFAULT.points_per_world();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_print_keeps_vector_text_page_offsets_and_physical_size() {
        let mut doc: Document =
            serde_json::from_str(include_str!("../tests/fixtures/ui-drawn-ethanol.reshiki"))
                .unwrap();
        doc.page_layout = Some(crate::pages::Layout {
            columns: 2,
            ..crate::pages::Layout::around(&doc)
        });
        let result: Value = serde_json::from_slice(&print_snapshot(&doc).unwrap()).unwrap();
        assert_eq!(result["version"], 1);
        assert_eq!(result["pages"].as_array().unwrap().len(), 2);
        assert!(result["width_pt"].as_f64().unwrap() > 500.);
        assert!(
            result["primitives"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["fill"].is_array())
        );
        assert!(
            result["primitives"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["kind"] == "path")
        );
        let first = result["pages"][0][0].as_f64().unwrap();
        let second = result["pages"][1][0].as_f64().unwrap();
        assert!(first - second > 500.);
    }
}
