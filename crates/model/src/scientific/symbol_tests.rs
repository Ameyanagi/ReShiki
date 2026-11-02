use super::*;
use crate::{document::Document, editing};

fn partial_tool(kind: SymbolKind) -> Drawing {
    Drawing {
        kind: GraphicKind::Symbol(kind),
        style: GraphicStyle::default(),
        phase: Phase::Solid,
        flipped: false,
        attach: true,
        snap_orbitals: true,
    }
}

#[test]
fn singlet_keeps_the_diradical_glyph_and_single_electron_symbols_clear_it() {
    let mut doc = Document::default();
    let style = doc.drawing_style.clone();
    let id = doc.add_atom("C", p(0., 0.));
    let atom = doc.atom_mut(id).unwrap();
    attach(atom, SymbolKind::Diradical, p(10., -10.)).unwrap();
    assert_eq!(atom.radical_electrons, 2);
    assert!(!atom.singlet);
    let triplet = styled_mark_parts(atom, &style);
    let atom = doc.atom_mut(id).unwrap();
    atom.singlet = true;
    assert_eq!(
        format!("{:?}", styled_mark_parts(atom, &style)),
        format!("{triplet:?}")
    );
    attach(atom, SymbolKind::Diradical, p(10., -10.)).unwrap();
    assert!(atom.singlet);
    let source = doc.clone();
    let copied = editing::append(&mut doc, &source, p(100., 0.));
    assert!(doc.atom(copied[0]).unwrap().singlet);
    for kind in [
        SymbolKind::Radical,
        SymbolKind::RadicalCation,
        SymbolKind::RadicalAnion,
    ] {
        let mut changed = source.clone();
        let atom = changed.atom_mut(id).unwrap();
        attach(atom, kind, p(10., -10.)).unwrap();
        assert_eq!(atom.radical_electrons, 1);
        assert!(!atom.singlet);
        changed.validate().unwrap();
    }
}

#[test]
fn partial_charges_are_annotations_and_keep_the_atom_chemistry_and_label() {
    for kind in [SymbolKind::PartialPositive, SymbolKind::PartialNegative] {
        let mut doc = Document::default();
        let id = doc.add_atom("N", p(0., 0.));
        let atom = doc.atom_mut(id).unwrap();
        atom.charge = 1;
        atom.radical_electrons = 1;
        atom.explicit_h = 2;
        atom.no_implicit = true;
        atom.label_h = 2;
        atom.display.hide_charge = true;
        let before = doc.clone();
        partial_tool(kind)
            .place(&mut doc, p(0., 0.), p(0., 0.), false, 10.)
            .unwrap();
        let atom = doc.atom(id).unwrap();
        let mark = atom.marks.first().unwrap();
        assert!(!mark.kind.charge() && !mark.kind.radical());
        assert!(
            mark_drawn(&doc, atom, mark),
            "Hide charge applies only to formal charges"
        );
        assert!(!styled_mark_parts(atom, &doc.drawing_style).is_empty());
        assert!(!crate::transaction::chemistry_changed(&before, &doc));
        let mut actual = atom.clone();
        actual.marks.clear();
        actual.mark_serial = before.atoms[0].mark_serial;
        assert_eq!(actual, before.atoms[0]);
        doc.validate().unwrap();
        assert_eq!(
            Document::from_json(&doc.file_json().unwrap()).unwrap(),
            doc.current()
        );
    }
}

#[test]
fn partial_charge_marks_follow_move_copy_and_rotation_while_staying_upright() {
    let mut doc = Document::default();
    let id = doc.add_atom("O", p(10., 20.));
    attach(
        doc.atom_mut(id).unwrap(),
        SymbolKind::PartialNegative,
        p(20., -20.),
    )
    .unwrap();
    doc.translate(&[id], 40., 10.);
    assert_eq!(doc.atoms[0].marks[0].offset, p(20., -20.));
    let source = doc.clone();
    let copied = editing::append(&mut doc, &source, p(100., 0.));
    let pivot = doc.atoms[1].position;
    editing::transform_about(&mut doc, &copied, pivot, 1., 90.);
    let mark = &doc.atoms[1].marks[0];
    assert!(mark.offset.distance(p(20., 20.)) < 0.001);
    assert_eq!(mark.angle, 0.);
    assert_eq!(mark.kind, MarkKind::PartialNegative);
    assert_eq!(doc.atoms[1].charge, 0);
    doc.validate().unwrap();
}

#[test]
fn partial_charge_glyphs_use_delta_and_the_unicode_minus() {
    for (kind, text) in [
        (SymbolKind::PartialPositive, "δ+"),
        (SymbolKind::PartialNegative, "δ−"),
    ] {
        assert_eq!(kind.partial_charge_text(), Some(text));
        let parts = symbol_parts(kind, 1, &GraphicStyle::default());
        assert_eq!(parts.len(), 1);
        assert!(parts[0].filled);
        let points: Vec<_> = parts[0]
            .commands
            .iter()
            .flat_map(PathCommand::iter_points)
            .collect();
        assert!(points.len() > 20, "Delta must retain its full outline");
        let width = points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max)
            - points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        assert!(width > 0.5 && width < 1.5);
    }
}

#[test]
fn diradical_dots_are_well_separated_and_lone_pair_spacing_is_unchanged() {
    for (kind, gap) in [(SymbolKind::Diradical, 0.9), (SymbolKind::LonePair, 0.4)] {
        let parts = symbol_parts(kind, 1, &GraphicStyle::default());
        assert_eq!(parts.len(), 2);
        let centers: Vec<_> = parts
            .iter()
            .map(|part| {
                assert!(part.filled);
                let points: Vec<_> = part
                    .commands
                    .iter()
                    .flat_map(PathCommand::iter_points)
                    .collect();
                let lo = points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
                let hi = points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
                assert!((hi - lo - 0.2).abs() < 1e-6, "Dot diameter stays unchanged");
                (lo + hi) * 0.5
            })
            .collect();
        assert!((centers[1] - centers[0] - gap).abs() < 1e-6);
    }
}
