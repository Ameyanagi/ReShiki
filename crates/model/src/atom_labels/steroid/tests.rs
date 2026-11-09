use super::*;
use crate::{
    atom_labels::{self, refresh::Refresh},
    document::{AtomStereo, Point},
};
use nazuchem_n2s::graph::{Element, StereoReference};

/// Independent name-to-graph oracle. OPSIN retains locants and explicit H
/// references; copy its parity directly, without the steroid display algorithm.
fn named(name: &str) -> (Document, HashMap<String, u64>) {
    // OPSIN supports estradiol's explicit steroid name, rather than its
    // retained trivial synonym. The specified 17β face remains the oracle.
    let source = if name == "estradiol" {
        "estra-1,3,5(10)-triene-3,17beta-diol"
    } else {
        name
    };
    let result = nazuchem_n2s::Parser::new()
        .unwrap()
        .parse(source, &nazuchem_n2s::ParseOptions::strict());
    assert_eq!(
        result.status,
        nazuchem_n2s::Status::Success,
        "{name}: {}",
        result.message
    );
    let structure = result.structure.unwrap();
    let graph = &structure.graph;
    let mut doc = Document::default();
    let mut ids = HashMap::new();
    let mut locants = HashMap::new();
    for &id in &graph.fragment(structure.fragment).atoms {
        let atom = graph.atom(id);
        let new = doc.add_atom(atom.element.symbol(), Point::default());
        ids.insert(id, new);
        let target = doc.atom_mut(new).unwrap();
        target.charge = atom.charge;
        target.isotope = atom.isotope.unwrap_or(0);
        target.no_implicit = true;
        target.explicit_h = atom.explicit_hydrogens;
        if atom.element != Element::H {
            for locant in &atom.locants {
                locants.insert(locant.clone(), new);
            }
        }
    }
    for &bond in &graph.fragment(structure.fragment).bonds {
        let bond = graph.bond(bond);
        doc.add_bond(ids[&bond.from], ids[&bond.to], bond.order, "plain");
    }
    for (&old, &id) in &ids {
        if let Some(parity) = &graph.atom(old).parity {
            let neighbours = parity
                .atom_refs
                .iter()
                .map(|reference| match reference {
                    Some(StereoReference::Atom(atom)) => ids[atom],
                    other => panic!("{name}: unexpected finalized OPSIN reference {other:?}"),
                })
                .collect();
            doc.atom_mut(id).unwrap().stereo = Some(AtomStereo {
                winding: if parity.parity == 1 { "cw" } else { "ccw" }.into(),
                neighbors: neighbours,
            });
        }
    }
    let molecule = crate::chemistry::document::prepare(&doc).unwrap();
    let positions = crate::chemistry::depict::compute(
        &molecule.state,
        &vec![None; doc.atoms.len()],
        None,
        Default::default(),
    )
    .unwrap()
    .positions;
    for (atom, point) in doc.atoms.iter_mut().zip(positions) {
        atom.position = Point::new((point.x * 28.) as f32, (-point.y * 28.) as f32);
    }
    doc.atom_labels.stereo = true;
    doc.atom_labels.steroid_alpha_beta = true;
    (doc, locants)
}

fn refresh(doc: &mut Document) -> Refresh {
    let result = Refresh::calculate(doc, &Refresh::default()).unwrap();
    assert!(result.notice.is_none(), "{:?}", result.notice);
    result.apply(doc);
    result
}

fn implicit_hydrogens(doc: &mut Document) {
    let hydrogens: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|a| a.element == "H")
        .map(|a| a.id)
        .collect();
    doc.atoms.retain(|a| !hydrogens.contains(&a.id));
    doc.bonds
        .retain(|b| !hydrogens.contains(&b.a) && !hydrogens.contains(&b.b));
    for atom in &mut doc.atoms {
        atom.no_implicit = false;
        if let Some(stereo) = &mut atom.stereo {
            if let Some(position) = stereo.neighbors.iter().position(|n| hydrogens.contains(n)) {
                // Moving the now implicit H to the final carrier preserves the
                // source configuration after omitting its explicit vertex.
                if (3 - position) % 2 != 0 {
                    stereo.winding = if stereo.winding == "cw" { "ccw" } else { "cw" }.into();
                }
            }
            stereo.neighbors.retain(|n| !hydrogens.contains(n));
        }
    }
}

fn assert_faces(doc: &Document, locants: &HashMap<String, u64>, expected: &[(u8, char)]) {
    for &(locant, face) in expected {
        let atom = doc.atom(locants[&locant.to_string()]).unwrap();
        assert_eq!(atom.steroid_label, Some(format!("{locant}{face}")));
        assert!(matches!(atom.cip_label.as_deref(), Some("R" | "S")));
        assert_eq!(
            atom_labels::atom_stereo_code(atom, doc),
            atom.steroid_label.as_deref()
        );
    }
}

#[test]
fn opsin_named_faces_pin_alpha_beta_parity_and_natural_junctions() {
    for (name, faces) in [
        (
            "5alpha-androstane-3beta,17beta-diol",
            vec![(3, 'β'), (5, 'α'), (17, 'β')],
        ),
        (
            "5beta-androstane-3alpha,17beta-diol",
            vec![(3, 'α'), (5, 'β'), (17, 'β')],
        ),
        ("cholesterol", vec![(3, 'β'), (17, 'β')]),
        ("estradiol", vec![(17, 'β')]),
    ] {
        let (mut doc, locants) = named(name);
        refresh(&mut doc);
        assert_faces(&doc, &locants, &faces);
        assert_faces(&doc, &locants, &[(8, 'β'), (9, 'α'), (13, 'β'), (14, 'α')]);
        if name != "estradiol" {
            assert_faces(&doc, &locants, &[(10, 'β')]);
        }
        if name == "cholesterol" {
            let outside = doc.atom(locants["20"]).unwrap();
            assert!(outside.steroid_label.is_none());
            assert!(matches!(outside.cip_label.as_deref(), Some("R" | "S")));
            assert_eq!(
                atom_labels::atom_stereo_code(outside, &doc),
                outside.cip_label.as_deref()
            );
        }
    }
}

#[test]
fn steroid_display_is_optional_nonchemical_and_recomputed_after_analysis() {
    let (mut doc, locants) = named("5alpha-androstane-3beta,17beta-diol");
    doc.atom_labels.steroid_alpha_beta = false;
    let before = refresh(&mut doc);
    let cip: Vec<_> = doc.atoms.iter().map(|a| a.cip_label.clone()).collect();
    assert!(doc.atoms.iter().all(|a| a.steroid_label.is_none()));
    doc.atom_labels.steroid_alpha_beta = true;
    let on = Refresh::calculate(&doc, &before).unwrap();
    on.apply(&mut doc);
    assert_faces(&doc, &locants, &[(3, 'β'), (5, 'α'), (17, 'β')]);
    assert_eq!(
        doc.atoms
            .iter()
            .map(|a| a.cip_label.clone())
            .collect::<Vec<_>>(),
        cip
    );
    let mut checked = doc.clone();
    for atom in &mut checked.atoms {
        atom.steroid_label = None;
    }
    atom_labels::copy_computed(&mut doc, &checked);
    assert_faces(&doc, &locants, &[(3, 'β'), (5, 'α'), (17, 'β')]);
    let saved = serde_json::to_value(&doc).unwrap();
    assert_eq!(saved["atom_labels"]["steroid_alpha_beta"], true);
    let loaded: Document = serde_json::from_value(saved).unwrap();
    assert_faces(&loaded, &locants, &[(3, 'β'), (5, 'α'), (17, 'β')]);
    let before = doc.clone();
    doc.atoms[0].steroid_label = Some("1α".into());
    assert!(!crate::transaction::chemistry_changed(&before, &doc));
    doc.atom_labels.steroid_alpha_beta = false;
    let atom = doc.atom(locants["3"]).unwrap();
    assert_eq!(
        atom_labels::atom_stereo_code(atom, &doc),
        atom.cip_label.as_deref()
    );
    let off = Refresh::calculate(&doc, &on).unwrap();
    off.apply(&mut doc);
    assert!(doc.atoms.iter().all(|a| a.steroid_label.is_none()));
    assert!(
        serde_json::to_value(&doc).unwrap()["atom_labels"]
            .get("steroid_alpha_beta")
            .is_none()
    );
    assert!(
        !serde_json::from_str::<atom_labels::Settings>("{}")
            .unwrap()
            .steroid_alpha_beta
    );
}

#[test]
fn labels_ignore_coordinates_and_reverse_for_the_mirror_configuration() {
    let (mut doc, locants) = named("5alpha-androstane-3beta,17beta-diol");
    refresh(&mut doc);
    let expected: Vec<_> = doc.atoms.iter().map(|a| a.steroid_label.clone()).collect();
    for reflect in [false, true] {
        let mut transformed = doc.clone();
        let (sin, cos) = 1.37_f32.sin_cos();
        for atom in &mut transformed.atoms {
            let Point { x, y } = atom.position;
            let x = if reflect { -x } else { x };
            atom.position = Point::new(x * cos - y * sin, x * sin + y * cos);
        }
        refresh(&mut transformed);
        assert_eq!(
            transformed
                .atoms
                .iter()
                .map(|a| a.steroid_label.clone())
                .collect::<Vec<_>>(),
            expected
        );
    }
    for atom in &mut doc.atoms {
        atom.position.x = -atom.position.x;
        if let Some(stereo) = &mut atom.stereo {
            stereo.winding = if stereo.winding == "cw" { "ccw" } else { "cw" }.into();
        }
    }
    refresh(&mut doc);
    assert_faces(
        &doc,
        &locants,
        &[
            (3, 'α'),
            (5, 'β'),
            (8, 'α'),
            (9, 'β'),
            (10, 'α'),
            (13, 'α'),
            (14, 'β'),
            (17, 'α'),
        ],
    );
}

#[test]
fn implicit_h_and_heteroatom_or_aromatic_nuclei_keep_the_same_faces() {
    for (name, replacement) in [
        ("5alpha-androstane-3beta,17beta-diol", "O"),
        ("estradiol", "N"),
    ] {
        let (mut doc, locants) = named(name);
        implicit_hydrogens(&mut doc);
        doc.atom_mut(locants["2"]).unwrap().element = replacement.into();
        refresh(&mut doc);
        assert_faces(
            &doc,
            &locants,
            &[(8, 'β'), (9, 'α'), (13, 'β'), (14, 'α'), (17, 'β')],
        );
        if replacement == "N" {
            assert!(
                crate::chemistry::document::prepare(&doc)
                    .unwrap()
                    .state
                    .graph
                    .atoms
                    .iter()
                    .any(|a| a.aromatic)
            );
        } else {
            assert_faces(&doc, &locants, &[(3, 'β'), (5, 'α'), (10, 'β')]);
        }
    }
}

#[test]
fn every_nucleus_is_labelled_in_separate_and_covalently_joined_components() {
    for joined in [false, true] {
        let (mut doc, first) = named("5alpha-androstane-3beta,17beta-diol");
        let (mut other, mut second) = named("5beta-androstane-3alpha,17beta-diol");
        implicit_hydrogens(&mut doc);
        implicit_hydrogens(&mut other);
        let offset = doc.next_id();
        for atom in &mut other.atoms {
            atom.id += offset;
            atom.position.x += 500.;
            if let Some(stereo) = &mut atom.stereo {
                for neighbour in &mut stereo.neighbors {
                    *neighbour += offset;
                }
            }
        }
        for bond in &mut other.bonds {
            bond.a += offset;
            bond.b += offset;
        }
        for id in second.values_mut() {
            *id += offset;
        }
        doc.atoms.extend(other.atoms);
        doc.bonds.extend(other.bonds);
        if joined {
            let oxygen = |center| {
                doc.bonds
                    .iter()
                    .filter_map(|b| {
                        let id = if b.a == center {
                            b.b
                        } else if b.b == center {
                            b.a
                        } else {
                            return None;
                        };
                        (doc.atom(id).unwrap().element == "O").then_some(id)
                    })
                    .next()
                    .unwrap()
            };
            let (a, b) = (oxygen(first["3"]), oxygen(second["3"]));
            let stereo: Vec<_> = doc.atoms.iter().map(|a| a.stereo.clone()).collect();
            doc.add_bond(a, b, 1, "plain");
            for (atom, stereo) in doc.atoms.iter_mut().zip(stereo) {
                atom.stereo = stereo;
            }
        }
        refresh(&mut doc);
        assert_faces(&doc, &first, &[(3, 'β'), (5, 'α'), (17, 'β')]);
        assert_faces(&doc, &second, &[(3, 'α'), (5, 'β'), (17, 'β')]);
    }
}

#[test]
fn two_external_non_hydrogen_substituents_keep_the_real_cip_descriptor() {
    let (mut doc, locants) = named("estradiol");
    implicit_hydrogens(&mut doc);
    let center = locants["17"];
    let stereo: HashMap<_, _> = doc.atoms.iter().map(|a| (a.id, a.stereo.clone())).collect();
    let origin = doc.atom(center).unwrap().position;
    let a = doc.add_atom("C", origin.offset(35., 10.));
    let b = doc.add_atom("C", origin.offset(70., 20.));
    doc.add_bond(center, a, 1, "plain");
    doc.add_bond(a, b, 3, "plain");
    for atom in &mut doc.atoms {
        atom.stereo = stereo.get(&atom.id).cloned().flatten();
    }
    let mut parity = doc.atom(center).unwrap().stereo.clone().unwrap();
    parity.neighbors.push(a);
    doc.atom_mut(center).unwrap().stereo = Some(parity);
    refresh(&mut doc);
    let atom = doc.atom(center).unwrap();
    assert!(atom.steroid_label.is_none());
    assert!(matches!(atom.cip_label.as_deref(), Some("R" | "S")));
    assert_eq!(
        atom_labels::atom_stereo_code(atom, &doc),
        atom.cip_label.as_deref()
    );
    assert_faces(&doc, &locants, &[(8, 'β'), (9, 'α'), (13, 'β'), (14, 'α')]);
}

#[test]
fn an_opened_nucleus_keeps_r_s_without_steroid_labels() {
    let (mut doc, locants) = named("5alpha-androstane-3beta,17beta-diol");
    implicit_hydrogens(&mut doc);
    let (a, b) = (locants["1"], locants["2"]);
    doc.bonds.retain(|bond| !bond.connects(a, b));
    refresh(&mut doc);
    assert!(doc.atoms.iter().all(|atom| atom.steroid_label.is_none()));
    assert!(doc.atoms.iter().any(|atom| atom.cip_label.is_some()));
}

#[test]
fn no_parentheses_steroid_text_is_unchanged_by_upright_parentheses() {
    let settings = atom_labels::Settings {
        roman_stereo_parentheses: true,
        ..Default::default()
    };
    let style = atom_labels::stereo_style();
    for code in ["3β", "5α", "17β"] {
        assert_eq!(atom_labels::stereo_text(code, &settings), code);
        let format = atom_labels::indicator_format(code, &style, true);
        assert!(format.style.italic);
        assert!(format.spans.is_empty());
    }
}

#[test]
fn reflected_wedge_drawings_preserve_faces_when_depth_markers_are_inverted() {
    let (mut source, locants) = named("5alpha-androstane-3beta,17beta-diol");
    implicit_hydrogens(&mut source);
    let molecule = crate::chemistry::document::prepare(&source).unwrap();
    let drawing = crate::chemistry::document::for_generated(
        &molecule,
        &vec![None; source.atoms.len()],
        Default::default(),
    )
    .unwrap();
    let labels = drawing.labels().unwrap();
    let mut doc = drawing.finish(labels).unwrap();
    doc.atom_labels = source.atom_labels;
    assert!(
        doc.bonds
            .iter()
            .any(|b| matches!(b.display.as_str(), "wedge" | "hash"))
    );
    for atom in &mut doc.atoms {
        atom.stereo = None;
    }
    refresh(&mut doc);
    assert_faces(
        &doc,
        &locants,
        &[
            (3, 'β'),
            (5, 'α'),
            (8, 'β'),
            (9, 'α'),
            (10, 'β'),
            (13, 'β'),
            (14, 'α'),
            (17, 'β'),
        ],
    );
    let expected: Vec<_> = doc.atoms.iter().map(|a| a.steroid_label.clone()).collect();
    for transform in [
        crate::editing::Transform::Rotate(73.),
        crate::editing::Transform::FlipHorizontal,
    ] {
        let mut moved = doc.clone();
        let ids = moved.all_ids();
        crate::editing::transform(&mut moved, &ids, transform);
        refresh(&mut moved);
        assert_eq!(
            moved
                .atoms
                .iter()
                .map(|a| a.steroid_label.clone())
                .collect::<Vec<_>>(),
            expected
        );
    }
    for atom in &mut doc.atoms {
        atom.position.x = -atom.position.x;
    }
    refresh(&mut doc);
    assert_faces(
        &doc,
        &locants,
        &[
            (3, 'α'),
            (5, 'β'),
            (8, 'α'),
            (9, 'β'),
            (10, 'α'),
            (13, 'α'),
            (14, 'β'),
            (17, 'α'),
        ],
    );
}
