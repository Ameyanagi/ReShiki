//! Captured-baseline parity for reconstruct through every entry point:
//! for_drawing, for_import, for_import_with_attachments and for_import_scene,
//! each finished draft, the warm for_drawing heap figures and the input
//! errors. Run with RESHIKI_MODEL_PARITY set; see crate::parity_tests.
use super::{Drawing, for_drawing, for_import, for_import_scene, for_import_with_attachments};
use crate::{
    chemistry::document::{Error, prepare},
    document::{Document, Point},
    parity_tests::{check_baseline, measured},
};
use std::fmt::Write as _;

macro_rules! fixture {
    ($path:literal) => {
        (
            $path,
            include_bytes!(concat!("../../../../../../", $path)).as_slice(),
        )
    };
}

const FIXTURES: [(&str, &[u8]); 6] = [
    fixture!("assets/examples/shortcut-examples.rsk"),
    fixture!("tests/fixtures/coordination-layout.rsk"),
    fixture!("tests/fixtures/tilted-fused-double-bonds.rsk"),
    fixture!("tests/fixtures/geometry/adamantane-projection.rsk"),
    fixture!("tests/fixtures/chemdraw-captions/source.rsk"),
    fixture!("tests/fixtures/chemdraw-arrows/reaction-source.rsk"),
];

/// Record one draft, then its finished document when the draft succeeded.
/// The Drawing Debug pins the pre-finish document, including the stereo
/// fields that finish later overwrites.
fn draft(text: &mut String, case: &str, drawing: Result<Drawing, Error>) {
    writeln!(text, "{case}: {drawing:?}").unwrap();
    if let Ok(drawing) = drawing {
        let finished = drawing
            .labels()
            .and_then(|labels| drawing.clone().finish(labels));
        writeln!(text, "{case} finish: {finished:?}").unwrap();
    }
}

fn record(text: &mut String, name: &str, doc: &Document) {
    let molecule = match prepare(doc) {
        Ok(molecule) => molecule,
        Err(error) => {
            writeln!(text, "{name} prepare: {error:?}").unwrap();
            return;
        }
    };
    let n = molecule.ids.len();
    let none = vec![None; n];
    draft(
        text,
        &format!("{name} for_drawing"),
        for_drawing(&molecule, doc),
    );
    for is_3d in [false, true] {
        draft(
            text,
            &format!("{name} for_import is_3d={is_3d}"),
            for_import(&molecule, is_3d, &none),
        );
    }
    // Every atom carries a label; only dummy atoms may use it.
    let labeled = (0..n).map(|i| Some(format!("R{i}"))).collect::<Vec<_>>();
    draft(
        text,
        &format!("{name} for_import labeled"),
        for_import(&molecule, false, &labeled),
    );
    let mut attachments = vec![false; n];
    if let Some(first) = attachments.first_mut() {
        *first = true;
    }
    draft(
        text,
        &format!("{name} for_import_with_attachments first"),
        for_import_with_attachments(&molecule, false, &none, attachments),
    );
    draft(
        text,
        &format!("{name} for_import_scene"),
        for_import_scene(&molecule, false),
    );
    // for_drawing ran above, so this measures a warm call.
    let (repeat, allocations) = measured(|| for_drawing(&molecule, doc));
    drop(repeat);
    writeln!(text, "{name} for_drawing alloc {allocations}").unwrap();
}

/// One drawing with a wedge stereocenter, an E double bond, an order-4
/// aromatic ring with a dummy substituent and a mapped atom, a dative bond
/// and a charged isotope radical.
fn synthetic() -> Document {
    let mut doc = Document::default();
    // C(F)(Cl)Br with a wedge from the stereocenter to F.
    let center = doc.add_atom("C", Point::new(0., 0.));
    let f = doc.add_atom("F", Point::new(0., -42.));
    let cl = doc.add_atom("Cl", Point::new(36.373, 21.));
    let br = doc.add_atom("Br", Point::new(-36.373, 21.));
    doc.add_bond(center, f, 1, "wedge");
    doc.add_bond(center, cl, 1, "plain");
    doc.add_bond(center, br, 1, "plain");
    // C–C=C–C with E stereo on the double bond.
    let a = doc.add_atom("C", Point::new(100., 21.));
    let b = doc.add_atom("C", Point::new(136.373, 0.));
    let c = doc.add_atom("C", Point::new(178.373, 0.));
    let d = doc.add_atom("C", Point::new(214.746, -21.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 2, "plain");
    doc.add_bond(c, d, 1, "plain");
    let double = &mut doc.bonds[4];
    double.stereo = Some("e".into());
    double.stereo_atoms = vec![a, d];
    // Benzene drawn with order-4 bonds, a dummy substituent and a mapped atom.
    let ring = [
        (362., 0.),
        (341., 36.373),
        (299., 36.373),
        (278., 0.),
        (299., -36.373),
        (341., -36.373),
    ]
    .map(|(x, y)| doc.add_atom("C", Point::new(x, y)));
    for (i, &atom) in ring.iter().enumerate() {
        doc.add_bond(atom, ring[(i + 1) % 6], 4, "plain");
    }
    let dummy = doc.add_atom("*", Point::new(404., 0.));
    doc.add_bond(ring[0], dummy, 1, "plain");
    doc.atom_mut(ring[2]).unwrap().map_num = 3;
    // Ammonia donating to copper through a dative bond.
    let n = doc.add_atom("N", Point::new(500., 0.));
    let cu = doc.add_atom("Cu", Point::new(542., 0.));
    doc.add_bond(n, cu, 5, "plain");
    // An isolated [13CH2+] radical.
    let radical = doc.add_atom("C", Point::new(640., 0.));
    let atom = doc.atom_mut(radical).unwrap();
    atom.isotope = 13;
    atom.charge = 1;
    atom.explicit_h = 2;
    atom.no_implicit = true;
    atom.radical_electrons = 1;
    doc.validate().unwrap();
    doc
}

/// Two atoms that share no bond, so re-pointing a bond to them changes the
/// drawing's endpoints without making the base invalid.
fn unbonded_pair(doc: &Document) -> (u64, u64) {
    let bonded = |x: u64, y: u64| {
        doc.bonds
            .iter()
            .any(|bond| (bond.a, bond.b) == (x, y) || (bond.a, bond.b) == (y, x))
    };
    doc.atoms
        .iter()
        .flat_map(|x| doc.atoms.iter().map(move |y| (x.id, y.id)))
        .find(|&(x, y)| x != y && !bonded(x, y))
        .unwrap()
}

fn errors(text: &mut String, doc: &Document) {
    let molecule = prepare(doc).unwrap();
    let mut repointed = doc.clone();
    let (x, y) = unbonded_pair(doc);
    let bond = &mut repointed.bonds[2];
    bond.a = x;
    bond.b = y;
    repointed.validate().unwrap();
    writeln!(
        text,
        "error repointed bond: {:?}",
        for_drawing(&molecule, &repointed)
    )
    .unwrap();
    let mut extra = doc.clone();
    extra.add_atom("C", Point::new(800., 80.));
    writeln!(
        text,
        "error extra atom: {:?}",
        for_drawing(&molecule, &extra)
    )
    .unwrap();
    let labels = vec![None; molecule.ids.len() + 1];
    writeln!(
        text,
        "error dummy label length: {:?}",
        for_import(&molecule, false, &labels)
    )
    .unwrap();
}

#[test]
#[ignore = "Captured-baseline parity; set RESHIKI_MODEL_PARITY"]
fn reconstruct_matches_captured_baseline() {
    let mut text = String::new();
    for (path, bytes) in FIXTURES {
        let doc = Document::from_json(bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        record(&mut text, path, &doc);
    }
    let doc = synthetic();
    record(&mut text, "synthetic", &doc);
    errors(&mut text, &doc);
    check_baseline("reconstruct", &text);
}
