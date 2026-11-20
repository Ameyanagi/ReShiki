use super::*;
use crate::{document::Document, graphics::PathCommand::*, palette::Color};

fn p(x: f32, y: f32) -> Point {
    Point::new(x, y)
}

fn flask() -> Icon {
    // A line, and a filled dot drawn with relative arcs.
    Icon::parse("flask", "M3 3l18 18|*M12 8a1 1 0 1 0 2 0a1 1 0 1 0 -2 0").unwrap()
}

#[test]
fn the_library_form_reads_lines_arcs_and_filled_dots() {
    let icon = flask();
    assert_eq!(icon.name, "flask");
    let [line, dot] = icon.paths.as_slice() else {
        panic!("two paths");
    };
    assert_eq!(line.commands, [Move(p(3., 3.)), Line(p(21., 21.))]);
    assert!(!line.filled);
    assert!(line.stroked);
    assert!(dot.filled);
    assert!(dot.stroked);
    assert_eq!(dot.commands[0], Move(p(12., 8.)));
    assert!(dot.commands[1..].iter().all(|c| matches!(c, Cubic(..))));
    assert!(icon.validate().is_ok());
    assert!(Icon::parse("bad", "M3 3l18 18|Q").is_err());
    let far = Icon::parse("far", "M3 3L90 3").unwrap();
    assert!(far.validate().is_err());
    // A file cannot make one icon arbitrarily costly to draw.
    let huge = Icon::parse("huge", &"M3 3l1 1".repeat(1001)).unwrap();
    assert!(huge.validate().is_err());
}

#[test]
fn fill_and_stroke_are_independent_for_each_path() {
    let icon = Icon::parse("details", "M3 3l18 18|*M8 8h2v2z|*!M12 3v9h9z|!M4 4l1 1").unwrap();
    assert!(icon.validate().is_ok());
    assert_eq!(
        icon.paths
            .iter()
            .map(|path| (path.filled, path.stroked))
            .collect::<Vec<_>>(),
        [(false, true), (true, true), (true, false), (false, false)]
    );
    let style = GraphicStyle {
        stroke: Color::Custom([30, 60, 90]),
        width_pt: 2.5,
        ..GraphicStyle::default()
    };
    let parts = parts(&icon, &style);
    assert_eq!(parts.len(), 4);
    for (part, path) in parts.iter().zip(&icon.paths) {
        assert_eq!(part.filled, path.filled);
        assert_eq!(part.style.fill, path.filled.then_some(style.stroke));
        assert_eq!(part.style.width_pt, if path.stroked { 2.5 } else { 0. });
    }
}

#[test]
fn saved_paths_without_a_stroke_field_keep_their_stroke() {
    let old = flask();
    let saved = serde_json::to_value(&old).unwrap();
    for path in saved["paths"].as_array().unwrap() {
        assert!(path.get("stroked").is_none());
    }
    let back: Icon = serde_json::from_value(saved).unwrap();
    assert_eq!(back, old);
    assert!(back.paths.iter().all(|path| path.stroked));

    let no_stroke = Icon::parse("wedge", "*!M12 3v9h9z").unwrap();
    let saved = serde_json::to_value(&no_stroke).unwrap();
    assert_eq!(saved["paths"][0]["stroked"], false);
    assert_eq!(serde_json::from_value::<Icon>(saved).unwrap(), no_stroke);
}

#[test]
fn no_stroke_details_have_no_outline_in_the_export_tree() {
    let mut doc = Document::default();
    doc.graphics.push(Graphic::line_icon(
        1,
        Icon::parse("wedges", "*M3 3v6h6z|*!M12 3v9h9z").unwrap(),
        p(0., 0.),
        24.,
        GraphicStyle {
            width_pt: 6.,
            ..GraphicStyle::default()
        },
    ));
    let svg = crate::scene::svg(&doc);
    // PNG and PDF exports both parse this SVG through usvg. A zero-width
    // part must become no stroke, rather than a PDF hairline.
    let tree = resvg::usvg::Tree::from_str(&svg, &Default::default()).unwrap();
    fn collect(group: &resvg::usvg::Group, paths: &mut Vec<(bool, bool)>) {
        for node in group.children() {
            match node {
                resvg::usvg::Node::Group(group) => collect(group, paths),
                resvg::usvg::Node::Path(path) => {
                    paths.push((path.fill().is_some(), path.stroke().is_some()));
                }
                _ => {}
            }
        }
    }
    let mut paths = Vec::new();
    collect(tree.root(), &mut paths);
    // The paper disc, an inherited-stroke detail, and a fill-only detail.
    assert_eq!(paths, [(true, false), (true, true), (true, false)]);
}

#[test]
fn a_placed_icon_takes_the_line_style_on_a_paper_disc() {
    let style = GraphicStyle {
        stroke: Color::Custom([200, 30, 30]),
        width_pt: 1.,
        ..GraphicStyle::default()
    };
    let g = Graphic::line_icon(7, flask(), p(100., 50.), 24., style);
    assert_eq!(g.kind, GraphicKind::Icon);
    assert!(g.layer > 0, "in front of the structures");
    assert!(g.is_icon());
    assert!(g.validate().is_ok());
    let parts = g.parts();
    let [disc, line, dot] = parts.as_slice() else {
        panic!("a disc and two paths");
    };
    assert!(disc.filled && disc.style.fill == Some(Color::Paper));
    // The grid's (3, 3) is 9 units up and left of its middle.
    assert_eq!(line.commands[0], Move(p(91., 41.)));
    assert_eq!(line.style.stroke, Color::Custom([200, 30, 30]));
    assert_eq!(line.style.width_pt, 1.);
    assert!(!line.filled);
    assert!(dot.filled && dot.style.fill == Some(Color::Custom([200, 30, 30])));
    // An icon keeps its drawing when someone tries to edit its points.
    let mut edited = g.clone();
    assert!(edited.edit_point(0, p(0., 0.)).is_err());
    assert_eq!(edited, g);
}

#[test]
fn an_icon_saves_with_its_drawing_at_the_drawing_release_version() {
    let mut doc = Document::default();
    let mut icon = flask();
    icon.paths[1].stroked = false;
    doc.graphics.push(Graphic::line_icon(
        1,
        icon,
        p(0., 0.),
        20.,
        GraphicStyle::default(),
    ));
    assert_eq!(doc.file_version(), crate::document::DRAWING_ELEMENTS);
    let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(back.version, crate::document::DRAWING_ELEMENTS);
    assert_eq!(back.graphics, doc.graphics);
    // Kind and drawing go together.
    let mut broken = doc.graphics[0].clone();
    broken.icon = None;
    assert!(broken.validate().is_err());
}
