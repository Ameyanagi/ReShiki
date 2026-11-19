//! Figure layout templates with panels and placeholder rings to replace.
use super::{Anchor, Template};
use crate::{
    arrows::{ArrowStyle, Preset},
    document::{Arrow, Document, Point},
    graphics::{
        BracketSides, Divider, Graphic, GraphicKind, GraphicStyle, LinePattern, Panel, TitlePlace,
    },
    grouping::Group,
    layouts::{Align, Flow, Layout},
    palette::{Color, Hue, Row},
};
use std::collections::HashMap;

/// The template group of figure layouts.
pub const GROUP: &str = "Figure layouts";
const GUTTER_PT: f32 = 10.;

/// A Kekulé benzene ring centered on `center`, as a placeholder structure.
fn ring(doc: &mut Document, center: Point) -> Vec<u64> {
    let r = crate::style::DEFAULT.bond_length_world;
    let ids: Vec<u64> = (0..6)
        .map(|i| {
            let angle = (30. + 60. * i as f32).to_radians();
            doc.add_atom("C", center.offset(r * angle.cos(), r * angle.sin()))
        })
        .collect();
    for (i, (&a, &b)) in ids.iter().zip(ids.iter().cycle().skip(1)).enumerate() {
        doc.add_bond(a, b, if i % 2 == 0 { 2 } else { 1 }, "plain");
    }
    ids
}

fn arrow(doc: &mut Document, from: Point, length: f32) -> u64 {
    let id = doc.next_id();
    doc.arrows.push(Arrow::new(
        id,
        from,
        from.offset(length, 0.),
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    id
}

/// Ring → arrow → ring (→ arrow → ring) on one line, as a lasting row.
fn scheme(doc: &mut Document, left: Point, steps: usize) -> Vec<u64> {
    let r = crate::style::DEFAULT.bond_length_world;
    let mut ids = ring(doc, left.offset(r, 0.));
    let mut x = left.x + 2. * r;
    for _ in 0..steps {
        ids.push(arrow(doc, Point::new(x + 10., left.y), 2. * r));
        x += 2. * r + 20.;
        ids.extend(ring(doc, Point::new(x + r, left.y)));
        x += 2. * r;
    }
    layout(doc, &ids, Flow::Row);
    ids
}

fn layout(doc: &mut Document, ids: &[u64], flow: Flow) -> u64 {
    let id = doc.next_id();
    doc.groups.push(Group {
        id,
        members: ids.to_vec(),
        integral: false,
        layout: Some(Layout {
            flow,
            gutter_pt: GUTTER_PT,
            align: Align::Center,
            order: vec![],
        }),
    });
    crate::layouts::reflow(doc, id, None, None);
    id
}

fn group(doc: &mut Document, ids: &[u64]) {
    let id = doc.next_id();
    doc.groups.push(Group {
        id,
        members: ids.to_vec(),
        integral: false,
        layout: None,
    });
}

/// A panel fitted around `ids`, grouped with them.
fn panel(doc: &mut Document, ids: &[u64], hue: Hue, panel: Panel) -> u64 {
    let text = doc.drawing_style.text_style();
    let (lo, hi) = crate::scene::selection_bounds(doc, ids).unwrap_or_default();
    let pad = crate::style::DEFAULT.world(10.);
    let id = doc.next_id();
    let mut g = Graphic::dragged(
        id,
        GraphicKind::RoundedRectangle,
        lo.offset(-pad, -pad),
        hi.offset(pad, pad),
        GraphicStyle {
            stroke: Color::Palette(hue, Row::Strong),
            fill: Some(Color::Palette(hue, Row::Tint)),
            width_pt: doc.drawing_style.line_width_pt,
            corner_pt: Some(Panel::ROUNDED_PT),
            ..Default::default()
        },
        BracketSides::Both,
        false,
    );
    g.panel = Some(panel);
    let band = g.panel_bar_height(&text);
    g.origin = g.origin.offset(0., -band);
    g.axis_y = Point::new(0., g.axis_y.y + band);
    doc.graphics.insert(0, g);
    let mut members = ids.to_vec();
    members.push(id);
    group(doc, &members);
    id
}

fn titled(titles: &[&str], place: TitlePlace) -> Panel {
    Panel {
        titles: titles.iter().map(|t| (*t).into()).collect(),
        place,
        ..Default::default()
    }
}

fn template(
    name: &str,
    keywords: &[&str],
    note: &str,
    rings: usize,
    document: Document,
) -> Template {
    Template {
        id: String::new(),
        group: GROUP.into(),
        name: name.into(),
        smiles: vec!["c1ccccc1"; rings].join("."),
        keywords: keywords.iter().map(|k| (*k).into()).collect(),
        low_rank_keywords: vec![],
        note: note.into(),
        locants: HashMap::new(),
        document,
        anchor: Anchor::Auto,
    }
}

pub(super) fn templates() -> Result<Vec<Template>, String> {
    let r = crate::style::DEFAULT.bond_length_world;
    let note = "The rings and arrows are illustrations, so they never form a reaction. Select a panel (Option- or Alt-click picks one row) and choose Clear contents, then paste or draw your scheme.";

    let mut toc = Document::default();
    let row = scheme(&mut toc, Point::default(), 2);
    panel(
        &mut toc,
        &row,
        Hue::Blue,
        titled(&["Graphical abstract"], TitlePlace::Bar),
    );

    let mut compare = Document::default();
    let previous = scheme(&mut compare, Point::default(), 1);
    let ours = scheme(&mut compare, Point::new(0., 5. * r), 1);
    let grey = titled(&["Previous work"], TitlePlace::Border);
    let first = panel(&mut compare, &previous, Hue::Indigo, grey);
    if let Some(g) = compare.graphics.iter_mut().find(|g| g.id == first) {
        // A neutral panel in Ink with a faint see-through fill: light grey on
        // the light canvas, dark grey on the dark one.
        g.style.corner_pt = Some(6.);
        g.style.stroke = Color::Ink;
        g.style.fill = Some(Color::Ink);
        g.style.effects.opacity = Some(0.08);
    }
    panel(
        &mut compare,
        &ours,
        Hue::Blue,
        titled(&["This work"], TitlePlace::Border),
    );
    let (lo, hi) = crate::scene::selection_bounds(&compare, &compare.all_ids()).unwrap_or_default();
    let middle = Point::new(lo.x + 10., 2.5 * r);
    let id = compare.next_id();
    compare.graphics.push(Graphic::dragged(
        id,
        GraphicKind::Line,
        middle,
        Point::new(hi.x - 10., middle.y),
        GraphicStyle {
            stroke: Color::Ink,
            width_pt: compare.drawing_style.line_width_pt,
            pattern: LinePattern::Dashed,
            ..Default::default()
        },
        BracketSides::Both,
        false,
    ));

    let mut evidence = Document::default();
    let mut held = scheme(&mut evidence, Point::default(), 1);
    let (_, hi) = crate::scene::selection_bounds(&evidence, &held).unwrap_or_default();
    let note_id = evidence.next_id();
    evidence.annotations.push(crate::document::Annotation {
        id: note_id,
        position: Point::new(hi.x + 3. * r, -0.6 * r),
        text: "¹H NMR, crystal structure\nor a picture".into(),
        format: Default::default(),
    });
    held.push(note_id);
    let frame = panel(
        &mut evidence,
        &held,
        Hue::Teal,
        titled(&["Scheme", "Evidence"], TitlePlace::Border),
    );
    let split = crate::graphics::panel::divider_position(&evidence, frame, true);
    if let Some(p) = evidence
        .graphics
        .iter_mut()
        .find(|g| g.id == frame)
        .and_then(|g| g.panel.as_mut())
    {
        p.dividers.push(Divider {
            vertical: true,
            at: split,
        });
    }

    let mut rows = Document::default();
    let first = scheme(&mut rows, Point::default(), 1);
    let second = scheme(&mut rows, Point::new(0., 4. * r), 1);
    // Each row sits on a plain panel, so Clear contents empties one row.
    for row in [first, second] {
        panel(&mut rows, &row, Hue::Blue, titled(&[], TitlePlace::Bar));
    }
    let both = rows.all_ids();
    layout(&mut rows, &both, Flow::Column);

    let documents = [
        (
            "Single-row TOC with a title bar",
            &["toc", "graphical abstract", "panel", "row"][..],
            3,
            toc,
        ),
        (
            "Previous work vs this work",
            &[
                "comparison",
                "previous work",
                "this work",
                "panel",
                "divider",
            ][..],
            4,
            compare,
        ),
        (
            "Scheme beside an evidence panel",
            &["evidence", "spectrum", "panel", "divider", "columns"][..],
            2,
            evidence,
        ),
        (
            "Two matched scheme rows",
            &["rows", "column", "scheme", "layout"][..],
            4,
            rows,
        ),
    ];
    documents
        .into_iter()
        .map(|(name, keywords, rings, mut document)| {
            // Placeholders are pictures: a ring → arrow → ring row must never
            // read as a reaction, nor join one drawn over it.
            let ids = document.all_ids();
            crate::purpose::set(&mut document, &ids, crate::purpose::Purpose::Illustration);
            document.require_version(crate::document::DRAWING_ELEMENTS);
            document.validate()?;
            Ok(template(name, keywords, note, rings, document))
        })
        .collect()
}
