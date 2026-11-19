use super::*;
use crate::document::Arrow;
use crate::reactions::assign;

/// Atoms keyed by number, `(key, element, x, y)`, bonds by key; with
/// `mapped` the key is also the atom map.
fn molecule(
    doc: &mut Document,
    mapped: bool,
    atoms: &[(u32, &str, f32, f32)],
    bonds: &[(u32, u32, u8)],
) -> HashMap<u32, u64> {
    let ids: HashMap<u32, u64> = atoms
        .iter()
        .map(|&(key, element, x, y)| {
            let id = doc.add_atom(element, Point::new(x, y));
            if mapped {
                doc.atoms.last_mut().unwrap().map_num = key;
            }
            (key, id)
        })
        .collect();
    for &(a, b, order) in bonds {
        doc.add_bond(ids[&a], ids[&b], order, "plain");
    }
    ids
}

struct DielsAlder {
    doc: Document,
    arrow: u64,
    diene: HashMap<u32, u64>,
    dienophile: HashMap<u32, u64>,
    product: HashMap<u32, u64>,
}

/// Butadiene (maps 1–4) and methyl acrylate (5–6, ester 7–10) give methyl
/// cyclohex-3-enecarboxylate, forming C4–C5 and C6–C1.
fn diels_alder(mapped: bool) -> DielsAlder {
    let mut doc = Document::default();
    let m = |n: u32| n;
    let diene = molecule(
        &mut doc,
        mapped,
        &[
            (m(1), "C", 0., 0.),
            (m(2), "C", 0., 40.),
            (m(3), "C", 0., 80.),
            (m(4), "C", 0., 120.),
        ],
        &[(m(1), m(2), 2), (m(2), m(3), 1), (m(3), m(4), 2)],
    );
    let dienophile = molecule(
        &mut doc,
        mapped,
        &[
            (m(5), "C", 80., 120.),
            (m(6), "C", 80., 80.),
            (m(7), "C", 120., 80.),
            (m(8), "O", 140., 50.),
            (m(9), "O", 150., 100.),
            (m(10), "C", 190., 100.),
        ],
        &[
            (m(5), m(6), 2),
            (m(6), m(7), 1),
            (m(7), m(8), 2),
            (m(7), m(9), 1),
            (m(9), m(10), 1),
        ],
    );
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(220., 60.),
        Point::new(320., 60.),
        Default::default(),
        Default::default(),
    ));
    let product = molecule(
        &mut doc,
        mapped,
        &[
            (m(1), "C", 400., 0.),
            (m(2), "C", 365., 20.),
            (m(3), "C", 365., 60.),
            (m(4), "C", 400., 80.),
            (m(5), "C", 435., 60.),
            (m(6), "C", 435., 20.),
            (m(7), "C", 470., 0.),
            (m(8), "O", 470., -40.),
            (m(9), "O", 505., 20.),
            (m(10), "C", 540., 0.),
        ],
        &[
            (m(1), m(2), 1),
            (m(2), m(3), 2),
            (m(3), m(4), 1),
            (m(4), m(5), 1),
            (m(5), m(6), 1),
            (m(6), m(1), 1),
            (m(6), m(7), 1),
            (m(7), m(8), 2),
            (m(7), m(9), 1),
            (m(9), m(10), 1),
        ],
    );
    let first = |x: f32, y: f32| {
        doc.atoms
            .iter()
            .find(|a| a.position == Point::new(x, y))
            .unwrap()
            .id
    };
    let (d, e, p) = (first(0., 0.), first(80., 120.), first(400., 0.));
    assign(&mut doc, arrow, &[d, e], Role::Reactant).unwrap();
    assign(&mut doc, arrow, &[p], Role::Product).unwrap();
    DielsAlder {
        doc,
        arrow,
        diene,
        dienophile,
        product,
    }
}

fn bond(doc: &Document, a: u64, b: u64) -> &crate::document::Bond {
    doc.bonds
        .iter()
        .find(|x| pair(x.a, x.b) == pair(a, b))
        .unwrap()
}

const BLUE: Color = Color::Palette(Hue::Blue, Row::Strong);
const GREEN: Color = Color::Palette(Hue::Green, Row::Strong);
const NEW: Color = Color::Palette(Hue::Red, Row::Tint);

#[test]
fn diels_alder_carries_fragment_colors_and_marks_new_bonds_and_ring() {
    let DielsAlder {
        mut doc,
        arrow,
        diene,
        dienophile,
        product: p,
    } = diels_alder(true);
    paint(&mut doc, arrow).unwrap();

    assert_eq!(bond(&doc, diene[&1], diene[&2]).color, BLUE);
    assert_eq!(bond(&doc, dienophile[&5], dienophile[&6]).color, GREEN);
    // The diene's carbons stay blue in the product, the dienophile's green.
    assert_eq!(bond(&doc, p[&2], p[&3]).color, BLUE);
    assert_eq!(bond(&doc, p[&5], p[&6]).color, GREEN);
    assert_eq!(bond(&doc, p[&7], p[&8]).color, GREEN);
    // The two new bonds keep their ink and are highlighted.
    for (a, b) in [(4, 5), (6, 1)] {
        let new = bond(&doc, p[&a], p[&b]);
        assert_eq!(new.color, Color::Ink);
        assert_eq!(new.highlight, Some(NEW));
    }
    // Changed bond orders are not marked by default.
    assert_eq!(bond(&doc, p[&2], p[&3]).highlight, None);
    // Oxygen labels take their fragment's color.
    let oxygen = doc.atom(p[&8]).unwrap();
    assert_eq!(oxygen.text_style.as_ref().unwrap().color, GREEN);
    // The new ring is filled.
    assert_eq!(doc.ring_fills.len(), 1);
    assert_eq!(doc.ring_fills[0].color, NEW);
    assert_eq!(doc.ring_fills[0].atoms.len(), 6);
    // Lighter than the new bonds' highlight, so the two stay apart.
    assert_eq!(
        doc.ring_fills[0].effect,
        crate::ring_fills::RingEffect::SeeThrough
    );
    let colors = doc.reactions[0].colors.as_ref().unwrap();
    assert_eq!(colors.reactants, vec![Hue::Blue, Hue::Green]);
    assert_eq!(doc.file_version(), crate::document::DRAWING_ELEMENTS);
    let reopened: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(reopened, doc);
}

#[test]
fn a_rerun_follows_edits_and_settings() {
    let DielsAlder {
        mut doc,
        arrow,
        product: p,
        ..
    } = diels_alder(true);
    paint(&mut doc, arrow).unwrap();
    update(&mut doc, arrow, |colors| {
        colors.reactants = vec![Hue::Purple, Hue::Teal];
        colors.formed = Hue::Orange;
        colors.changes = true;
    })
    .unwrap();
    let new = Color::Palette(Hue::Orange, Row::Tint);
    assert_eq!(
        bond(&doc, p[&2], p[&3]).color,
        Color::Palette(Hue::Purple, Row::Strong)
    );
    // No blue, green or red paint is left behind.
    assert!(
        doc.bonds
            .iter()
            .all(|b| b.color != BLUE && b.color != GREEN && b.highlight.is_none_or(|h| h != NEW))
    );
    // Changed orders are now highlighted, and the ring fill changed color.
    assert_eq!(
        bond(&doc, p[&2], p[&3]).highlight,
        Some(Color::Palette(Hue::Amber, Row::Tint))
    );
    assert_eq!(doc.ring_fills.len(), 1);
    assert_eq!(doc.ring_fills[0].color, new);

    // An atom that loses its map partner goes back to ink.
    let purple = Color::Palette(Hue::Purple, Row::Strong);
    assert_eq!(bond(&doc, p[&3], p[&4]).color, purple);
    doc.atoms
        .iter_mut()
        .find(|a| a.id == p[&3])
        .unwrap()
        .map_num = 0;
    paint(&mut doc, arrow).unwrap();
    assert_eq!(bond(&doc, p[&3], p[&4]).color, Color::Ink);
    doc.validate().unwrap();
}

#[test]
fn coloring_needs_roles_and_maps() {
    let DielsAlder { mut doc, arrow, .. } = diels_alder(false);
    assert_eq!(paint(&mut doc, arrow), Err(Refusal::Maps));
    doc.reactions.clear();
    assert_eq!(paint(&mut doc, arrow), Err(Refusal::Roles));
}

#[test]
fn the_legend_keeps_edited_names_and_goes_away_with_the_colors() {
    let DielsAlder { mut doc, arrow, .. } = diels_alder(true);
    paint(&mut doc, arrow).unwrap();
    set_legend(&mut doc, arrow, true, Default::default()).unwrap();
    let id = doc.reactions[0].colors.as_ref().unwrap().legend.unwrap();
    let text = |doc: &Document| doc.annotations.iter().find(|a| a.id == id).unwrap().clone();
    // The colors of the swatch glyphs, in order.
    let swatches = |doc: &Document| -> Vec<Color> {
        let legend = text(doc);
        legend
            .text
            .char_indices()
            .filter(|(_, c)| *c == SWATCH || *c == RING)
            .map(|(i, _)| legend.format.at(i).color)
            .collect()
    };
    assert_eq!(
        text(&doc).text,
        "■ Reactant 1   ■ Reactant 2   ■ New bond   ● New ring"
    );
    let red = Color::Palette(Hue::Red, Row::Strong);
    assert_eq!(swatches(&doc), vec![BLUE, GREEN, red, red]);
    assert!(doc.reactions[0].annotations.contains(&id));

    // Renamed entries survive a rerun.
    let renamed = "■ Diene   ■ Dienophile   ■ New C–C bond   ● New ring";
    let legend = doc.annotations.iter_mut().find(|a| a.id == id).unwrap();
    let before = std::mem::replace(&mut legend.text, renamed.into());
    legend.format.edited(&before, renamed, 0..before.len());
    doc.validate().unwrap();
    update(&mut doc, arrow, |colors| {
        colors.reactants = vec![Hue::Purple, Hue::Teal]
    })
    .unwrap();
    assert_eq!(text(&doc).text, renamed);
    let purple = Color::Palette(Hue::Purple, Row::Strong);
    let teal = Color::Palette(Hue::Teal, Row::Strong);
    assert_eq!(swatches(&doc), vec![purple, teal, red, red]);
    // Turning an entry off keeps the other names.
    update(&mut doc, arrow, |colors| colors.bonds = false).unwrap();
    assert_eq!(text(&doc).text, "■ Diene   ■ Dienophile   ● New ring");
    assert_eq!(swatches(&doc), vec![purple, teal, red]);
    update(&mut doc, arrow, |colors| colors.bonds = true).unwrap();
    assert_eq!(
        text(&doc).text,
        "■ Diene   ■ Dienophile   ■ New bond   ● New ring"
    );
    doc.validate().unwrap();

    assert!(clear_paint(&mut doc, arrow));
    assert!(doc.annotations.iter().all(|a| a.id != id));
    assert!(doc.reactions[0].colors.is_none());
    assert!(doc.ring_fills.is_empty());
    assert!(
        doc.bonds
            .iter()
            .all(|b| b.color == Color::Ink && b.highlight.is_none())
    );
}

#[test]
fn a_later_step_carries_the_colors_it_receives() {
    let DielsAlder {
        mut doc,
        product: p,
        ..
    } = diels_alder(true);
    // Hydrogenate the product: the same maps, the C2=C3 bond now single.
    let second = doc.next_id();
    doc.arrows.push(Arrow::new(
        second,
        Point::new(560., 60.),
        Point::new(640., 60.),
        Default::default(),
        Default::default(),
    ));
    let saturated = molecule(
        &mut doc,
        true,
        &[
            (1, "C", 720., 0.),
            (2, "C", 685., 20.),
            (3, "C", 685., 60.),
            (4, "C", 720., 80.),
            (5, "C", 755., 60.),
            (6, "C", 755., 20.),
        ],
        &[
            (1, 2, 1),
            (2, 3, 1),
            (3, 4, 1),
            (4, 5, 1),
            (5, 6, 1),
            (6, 1, 1),
        ],
    );
    assign(&mut doc, second, &[p[&1]], Role::Reactant).unwrap();
    assign(&mut doc, second, &[saturated[&1]], Role::Product).unwrap();
    paint(&mut doc, second).unwrap();
    assert_eq!(bond(&doc, saturated[&2], saturated[&3]).color, BLUE);
    assert_eq!(bond(&doc, saturated[&5], saturated[&6]).color, GREEN);
    // Both steps carry settings; no third fragment color appears.
    assert!(doc.reactions.iter().all(|r| r.colors.is_some()));
    assert!(
        doc.bonds
            .iter()
            .all(|b| b.color == Color::Ink || b.color == BLUE || b.color == GREEN)
    );

    // Both legends follow a hue change made on the first step.
    let arrow = doc.reactions[0].arrow;
    set_legend(&mut doc, arrow, true, Default::default()).unwrap();
    set_legend(&mut doc, second, true, Default::default()).unwrap();
    update(&mut doc, arrow, |colors| colors.reactants[0] = Hue::Purple).unwrap();
    let purple = Color::Palette(Hue::Purple, Row::Strong);
    assert_eq!(bond(&doc, saturated[&2], saturated[&3]).color, purple);
    for reaction in &doc.reactions {
        let id = reaction.colors.as_ref().unwrap().legend.unwrap();
        let legend = doc.annotations.iter().find(|a| a.id == id).unwrap();
        assert!(legend.text.starts_with("■ Reactant 1"));
        assert_eq!(legend.format.at(0).color, purple);
    }
}

/// The legend's id and the colors of its swatch glyphs, in order.
fn legend_swatches(doc: &Document) -> (u64, Vec<Color>) {
    let id = doc.reactions[0].colors.as_ref().unwrap().legend.unwrap();
    let legend = doc.annotations.iter().find(|a| a.id == id).unwrap();
    let colors = legend
        .text
        .char_indices()
        .filter(|(_, c)| *c == SWATCH || *c == RING)
        .map(|(i, _)| legend.format.at(i).color)
        .collect();
    (id, colors)
}

#[test]
fn retyping_the_whole_legend_keeps_swatch_colors_and_leaves_names_in_ink() {
    let DielsAlder { mut doc, arrow, .. } = diels_alder(true);
    set_legend(&mut doc, arrow, true, Default::default()).unwrap();
    let (id, colors) = legend_swatches(&doc);
    let before = doc.clone();
    // The text editor gives replacing text the style of the first replaced
    // character: here, the first reactant's blue swatch.
    let typed = "■ diene   ■ dienophile   ■ New bond   ● New ring";
    let legend = doc.annotations.iter_mut().find(|a| a.id == id).unwrap();
    let old = std::mem::replace(&mut legend.text, typed.into());
    legend.format.edited(&old, typed, 0..old.len());
    let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
    assert_eq!(legend_swatches(&doc).1, colors);
    let legend = doc.annotations.iter().find(|a| a.id == id).unwrap();
    for (i, c) in legend.text.char_indices() {
        if c != SWATCH && c != RING {
            assert_eq!(legend.format.at(i).color, Color::Ink, "{c:?} at {i}");
        }
    }
}

#[test]
fn a_name_colored_before_the_edit_keeps_its_color() {
    let DielsAlder { mut doc, arrow, .. } = diels_alder(true);
    set_legend(&mut doc, arrow, true, Default::default()).unwrap();
    let (id, colors) = legend_swatches(&doc);
    let legend = doc.annotations.iter_mut().find(|a| a.id == id).unwrap();
    // Color the first name like its swatch, then add a letter to it.
    let end = legend.text.find("   ").unwrap();
    legend.format.apply(
        &legend.text,
        Some(0..end),
        &crate::typography::StyleChange::Color(BLUE),
    );
    let before = doc.clone();
    let legend = doc.annotations.iter_mut().find(|a| a.id == id).unwrap();
    let old = legend.text.clone();
    legend.text.insert(end, 's');
    let new = legend.text.clone();
    legend.format.edited(&old, &new, end..end);
    let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
    assert_eq!(legend_swatches(&doc).1, colors);
    let legend = doc.annotations.iter().find(|a| a.id == id).unwrap();
    let name = SWATCH.len_utf8() + 1;
    assert_eq!(legend.format.at(name).color, BLUE);
}

/// Ink extents of the legend and of each molecule and arrow of the drawing.
fn legend_and_others(doc: &Document) -> ((Point, Point), Vec<(Point, Point)>) {
    let (id, _) = legend_swatches(doc);
    let legend = crate::scene::selection_bounds(doc, &[id]).unwrap();
    let mut objects =
        crate::reactions::molecules(doc, &doc.atoms.iter().map(|a| a.id).collect::<Vec<_>>());
    objects.extend(
        doc.arrows.iter().map(|a| vec![a.id]).chain(
            doc.annotations
                .iter()
                .filter(|a| a.id != id)
                .map(|a| vec![a.id]),
        ),
    );
    let boxes = crate::scene::selections_bounds(doc, &objects)
        .into_iter()
        .flatten()
        .collect();
    (legend, boxes)
}
fn apart((a, b): (Point, Point), (p, q): (Point, Point)) -> bool {
    q.x < a.x || p.x > b.x || q.y < a.y || p.y > b.y
}

#[test]
fn a_new_legend_sits_below_its_reaction_clear_of_other_text() {
    let DielsAlder { mut doc, arrow, .. } = diels_alder(true);
    let mut free = doc.clone();
    set_legend(&mut free, arrow, true, Default::default()).unwrap();
    // Below the reaction when that is free.
    let ((lo, _), others) = legend_and_others(&free);
    let bottom = others.iter().map(|b| b.1.y).fold(f32::MIN, f32::max);
    assert!(lo.y > bottom);

    // A caption already there pushes the legend to a free spot.
    let id = doc.next_id();
    doc.annotations.push(Annotation {
        id,
        position: Point::new(0., bottom + 30.),
        text: "Diels–Alder, 80 °C, 12 h, toluene".into(),
        format: Default::default(),
    });
    set_legend(&mut doc, arrow, true, Default::default()).unwrap();
    let (legend, others) = legend_and_others(&doc);
    assert!(others.iter().all(|b| apart(legend, *b)), "{legend:?}");
}

#[test]
fn a_new_legend_stays_on_the_reactions_page() {
    let DielsAlder { mut doc, arrow, .. } = diels_alder(true);
    let (lo, hi) = crate::scene::selection_bounds(&doc, &doc.all_ids()).unwrap();
    // A page that leaves no room below the reaction, but room above it.
    let pt = crate::style::DEFAULT.points_per_world();
    let pad = 10.;
    doc.page_layout = Some(crate::pages::Layout {
        width_pt: (hi.x - lo.x + 2. * pad) * pt,
        height_pt: (hi.y - lo.y + 310.) * pt,
        margins: crate::pages::Margins {
            top: 0.,
            right: 0.,
            bottom: 0.,
            left: 0.,
        },
        columns: 1,
        rows: 1,
        origin: Point::new(lo.x - pad, lo.y - 300.),
    });
    doc.validate().unwrap();
    set_legend(&mut doc, arrow, true, Default::default()).unwrap();
    let (page_lo, page_hi) = doc.page_layout.as_ref().unwrap().bounds(0).unwrap();
    let (legend, others) = legend_and_others(&doc);
    assert!(
        legend.0.x >= page_lo.x
            && legend.0.y >= page_lo.y
            && legend.1.x <= page_hi.x
            && legend.1.y <= page_hi.y,
        "{legend:?} on {page_lo:?}–{page_hi:?}"
    );
    assert!(others.iter().all(|b| apart(legend, *b)), "{legend:?}");
    assert_eq!(doc.page_layout.as_ref().unwrap().overflow(&doc), 0);
    // A new entry keeps it on the page.
    update(&mut doc, arrow, |colors| colors.changes = true).unwrap();
    let (legend, _) = legend_and_others(&doc);
    assert!(
        legend.1.x <= page_hi.x && legend.1.y <= page_hi.y,
        "{legend:?}"
    );
    assert_eq!(doc.page_layout.as_ref().unwrap().overflow(&doc), 0);
}

#[test]
fn the_reaction_copies_without_its_legend() {
    let DielsAlder { mut doc, arrow, .. } = diels_alder(true);
    set_legend(&mut doc, arrow, true, Default::default()).unwrap();
    let (legend, _) = legend_swatches(&doc);
    let ids: Vec<u64> = doc
        .all_ids()
        .into_iter()
        .filter(|id| *id != legend)
        .collect();
    let selection = crate::editing::selection(&doc, &ids);
    let copied = crate::reactions::copy_reaction(&doc, &selection)
        .unwrap()
        .unwrap();
    assert!(!copied.annotations.contains(&legend));
    assert_eq!(copied.colors.and_then(|c| c.legend), None);
    // With the legend selected too, it still copies.
    let all = crate::editing::selection(&doc, &doc.all_ids());
    assert!(
        crate::reactions::copy_reaction(&doc, &all)
            .unwrap()
            .is_some()
    );
}

#[test]
fn names_stay_with_their_entries_when_swatches_share_a_color() {
    let DielsAlder { mut doc, arrow, .. } = diels_alder(true);
    paint(&mut doc, arrow).unwrap();
    // New bonds and rings in the first reactant's blue.
    update(&mut doc, arrow, |colors| colors.formed = Hue::Blue).unwrap();
    set_legend(&mut doc, arrow, true, Default::default()).unwrap();
    let (id, _) = legend_swatches(&doc);
    let legend = doc.annotations.iter_mut().find(|a| a.id == id).unwrap();
    let renamed = legend.text.replacen("Reactant 1", "Diene", 1);
    let old = std::mem::replace(&mut legend.text, renamed.clone());
    legend.format.edited(&old, &renamed, 0..old.len());
    update(&mut doc, arrow, |colors| colors.rings = false).unwrap();
    let legend = doc.annotations.iter().find(|a| a.id == id).unwrap();
    assert_eq!(legend.text, "■ Diene   ■ Reactant 2   ■ New bond");

    // A name still as generated finds its own entry first.
    let legend = doc.annotations.iter_mut().find(|a| a.id == id).unwrap();
    let renamed = "■ Reactant 1   ■ Reactant 2   ■ Formed C–C".to_string();
    let old = std::mem::replace(&mut legend.text, renamed.clone());
    legend.format.edited(&old, &renamed, 0..old.len());
    update(&mut doc, arrow, |colors| colors.rings = true).unwrap();
    let legend = doc.annotations.iter().find(|a| a.id == id).unwrap();
    assert_eq!(
        legend.text,
        "■ Reactant 1   ■ Reactant 2   ■ Formed C–C   ● New ring"
    );
}
