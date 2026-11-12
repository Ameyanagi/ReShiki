use super::*;
use crate::{
    abbreviations::{Abbreviation as DrawingAbbreviation, LabelAlignment},
    document::{Document, Point},
    engine::{LocalEngine, Request},
    generic::{Crossing, RepeatCount, RepeatPattern, RepeatUnit},
};

fn abbreviation(
    label: &str,
    anchor: u64,
    exit: Option<u64>,
    members: &[u64],
) -> DrawingAbbreviation {
    DrawingAbbreviation {
        label: label.into(),
        reverse_label: String::new(),
        anchor,
        exit,
        members: members.to_vec(),
        alignment: LabelAlignment::Auto,
        label_style: None,
        label_color_override: false,
        highlight: None,
    }
}

fn ether() -> Document {
    let mut document = Document::default();
    for (index, element) in ["C", "O", "C"].into_iter().enumerate() {
        document.add_atom(element, Point::new(index as f32 * 42., 0.));
    }
    document.add_bond(1, 2, 1, "plain");
    document.add_bond(2, 3, 1, "plain");
    document
        .abbreviations
        .push(abbreviation("OMe", 2, None, &[2, 3]));
    document
}

// Exercise the public drawing writer for V2000. The lower-level drawing
// writer exposes the version option needed to exercise the same SUP adapter
// in V3000 without adding unrelated query or polymer semantics to a molecule.
fn export(document: &Document, force_v3000: bool) -> anyhow::Result<String> {
    if !force_v3000 {
        return Ok(write_document(document)?);
    }
    let molecule = document::prepare(document)?;
    let (output, fallback) = write_drawing_part(
        &molecule,
        document,
        Options { force_v3000 },
        false,
        &[],
        true,
    )?;
    assert!(!fallback);
    let output = generic::restore_output(output, document)?;
    Ok(superatoms::restore_output(output, document, &molecule)?)
}

fn identity(document: &Document) -> anyhow::Result<String> {
    Ok(crate::chemistry::smiles::write::write(
        &document::prepare(document)?.state,
        Default::default(),
    )?
    .text)
}

fn assert_positions_preserved(before: &Document, after: &Document) {
    assert_eq!(before.atoms.len(), after.atoms.len());
    let before_origin = before.atoms[0].position;
    let after_origin = after.atoms[0].position;
    for (before, after) in before.atoms.iter().zip(&after.atoms) {
        assert_eq!(before.element, after.element);
        let dx = (before.position.x - before_origin.x) - (after.position.x - after_origin.x);
        let dy = (before.position.y - before_origin.y) - (after.position.y - after_origin.y);
        assert!(
            dx.hypot(dy) < 0.01,
            "atom {} moved by ({dx}, {dy})",
            before.id
        );
    }
}

#[tokio::test]
async fn boc_ala_gly_ome_retains_all_four_superatoms_in_both_versions() -> anyhow::Result<()> {
    use crate::chemistry::abbreviations::{self, AttachmentPolicy};

    let engine = LocalEngine::in_process();
    let mut source = engine
        .request(Request::import_smiles(
            "C[C@H](NC(=O)OC(C)(C)C)C(=O)NCC(=O)OC",
        ))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .unwrap();
    // Storage order must not determine either attachment's role.
    source.bonds.reverse();
    for label in ["Boc", "Ala", "Gly", "OMe"] {
        source = abbreviations::find_with_policy(
            &source,
            &document::prepare(&source)?,
            &[],
            Some(label),
            AttachmentPolicy::SharedAnchor,
        )?;
    }
    assert_eq!(source.abbreviations.len(), 4);
    source.validate().map_err(anyhow::Error::msg)?;
    let expected = identity(&source)?;
    let prepared = document::prepare(&source)?;
    let snapshot = source.clone();
    for force_v3000 in [false, true] {
        let output = export(&source, force_v3000)?;
        let version = if force_v3000 { "V3000" } else { "V2000" };
        assert!(
            output.lines().nth(3).unwrap().ends_with(version),
            "{output}"
        );
        let imported = read(&output)?;
        let remap: HashMap<_, _> = prepared
            .ids
            .iter()
            .copied()
            .zip(imported.molecule.ids.iter().copied())
            .collect();
        for before in &source.abbreviations {
            let annotation = imported
                .annotations
                .superatoms
                .iter()
                .find(|group| group.label == before.label)
                .unwrap();
            let mut expected_members: Vec<_> = before.members.iter().map(|id| remap[id]).collect();
            let mut actual_members = annotation.members.clone();
            expected_members.sort_unstable();
            actual_members.sort_unstable();
            assert_eq!(
                actual_members, expected_members,
                "{version}: {} file members",
                before.label
            );
            let expected_attachments = match before.exit {
                Some(exit) => vec![
                    (remap[&before.anchor], "Al".into()),
                    (remap[&exit], "Br".into()),
                ],
                None => vec![(remap[&before.anchor], "1".into())],
            };
            assert_eq!(
                annotation.attachments, expected_attachments,
                "{version}: {} SAP",
                before.label
            );
            assert!(!annotation.expanded);
        }
        let (reopened, _, notices) = imported.display_document()?;
        assert!(notices.is_empty(), "{version}: {notices:?}");
        assert_eq!(reopened.abbreviations.len(), 4, "{version}: {output}");
        for label in ["Boc", "Ala", "Gly", "OMe"] {
            let before = source
                .abbreviations
                .iter()
                .find(|group| group.label == label)
                .unwrap();
            let after = reopened
                .abbreviations
                .iter()
                .find(|group| group.label == label)
                .unwrap();
            let mut expected_members: Vec<_> = before.members.iter().map(|id| remap[id]).collect();
            let mut actual_members = after.members.clone();
            expected_members.sort_unstable();
            actual_members.sort_unstable();
            assert_eq!(
                actual_members, expected_members,
                "{version}: {label} members"
            );
            assert_eq!(
                after.anchor, remap[&before.anchor],
                "{version}: {label} anchor"
            );
            assert_eq!(
                after.exit,
                before.exit.map(|id| remap[&id]),
                "{version}: {label} exit"
            );
        }
        assert_eq!(identity(&reopened)?, expected, "{version} stereochemistry");
        assert_eq!(reopened.bonds.len(), source.bonds.len());
        assert_positions_preserved(&source, &reopened);
        reopened.validate().map_err(anyhow::Error::msg)?;
    }
    assert_eq!(
        source, snapshot,
        "drawing export must leave the native view intact"
    );
    Ok(())
}

#[test]
fn one_sided_ome_retains_its_single_attachment_in_both_versions() -> anyhow::Result<()> {
    let source = ether();
    for force_v3000 in [false, true] {
        let output = export(&source, force_v3000)?;
        let (reopened, _, notices) = read(&output)?.display_document()?;
        assert!(notices.is_empty(), "{notices:?}");
        let [group] = reopened.abbreviations.as_slice() else {
            panic!("Expected OMe SUP: {output}");
        };
        assert_eq!(group.label, "OMe");
        assert_eq!(group.exit, None);
        assert_eq!(reopened.atom(group.anchor).unwrap().element, "O");
        assert_eq!(group.members.len(), 2);
        assert_eq!(identity(&reopened)?, identity(&source)?);
        assert_positions_preserved(&source, &reopened);
    }
    Ok(())
}

#[tokio::test]
async fn shared_engine_import_restores_sup_before_and_after_sdf_properties() -> anyhow::Result<()> {
    let engine = LocalEngine::in_process();
    for force_v3000 in [false, true] {
        let source = export(&ether(), force_v3000)?;
        let plain = engine
            .request(Request::import("mol", &source))
            .await
            .map_err(anyhow::Error::msg)?;
        let sdf = format!("{source}\n>  <SOURCE>\nSUP regression\n\n$$$$\n");
        let with_properties = engine
            .request(Request::import("mol", &sdf))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(with_properties.warnings, plain.warnings);
        assert!(plain.warnings.is_empty(), "{:?}", plain.warnings);
        let before = plain.document.unwrap();
        let after = with_properties.document.unwrap();
        assert_eq!(before.abbreviations.len(), 1);
        assert_eq!(before.abbreviations[0].label, "OMe");
        assert_eq!(after.abbreviations, before.abbreviations);
        assert_eq!(identity(&after)?, identity(&before)?);
        assert_positions_preserved(&before, &after);
    }
    Ok(())
}

fn v2000(atoms: &[(&str, f64, f64)], bonds: &[(usize, usize)], groups: &str) -> String {
    let mut lines = vec![
        String::new(),
        "     RDKit          2D".into(),
        String::new(),
        format!(
            "{:3}{:3}  0  0  0  0  0  0  0  0999 V2000",
            atoms.len(),
            bonds.len()
        ),
    ];
    lines.extend(atoms.iter().map(|(element, x, y)| {
        format!(
            "{x:10.4}{y:10.4}{:10.4} {element:<3} 0  0  0  0  0  0  0  0  0  0  0  0",
            0.
        )
    }));
    lines.extend(bonds.iter().map(|(a, b)| format!("{a:3}{b:3}  1  0")));
    lines.push(groups.into());
    lines.push("M  END\n".into());
    lines.join("\n")
}

fn v3000(
    atoms: &[(&str, f64, f64)],
    bonds: &[(usize, usize)],
    groups: &str,
    count: usize,
) -> String {
    let mut lines = vec![
        String::new(),
        "     RDKit          2D".into(),
        String::new(),
        "  0  0  0  0  0  0  0  0  0  0999 V3000".into(),
        "M  V30 BEGIN CTAB".into(),
        format!("M  V30 COUNTS {} {} {count} 0 0", atoms.len(), bonds.len()),
        "M  V30 BEGIN ATOM".into(),
    ];
    lines.extend(
        atoms
            .iter()
            .enumerate()
            .map(|(index, (element, x, y))| format!("M  V30 {} {element} {x} {y} 0 0", index + 1)),
    );
    lines.push("M  V30 END ATOM".into());
    lines.push("M  V30 BEGIN BOND".into());
    lines.extend(
        bonds
            .iter()
            .enumerate()
            .map(|(index, (a, b))| format!("M  V30 {} 1 {a} {b}", index + 1)),
    );
    lines.push("M  V30 END BOND".into());
    lines.push("M  V30 BEGIN SGROUP".into());
    lines.push(groups.into());
    lines.push("M  V30 END SGROUP".into());
    lines.push("M  V30 END CTAB".into());
    lines.push("M  END\n".into());
    lines.join("\n")
}

#[test]
fn three_attachment_superatom_stays_expanded_with_an_import_notice() -> anyhow::Result<()> {
    let atoms = [
        ("C", 0., 0.),
        ("C", 1., 0.),
        ("C", 2., 0.),
        ("C", 3., 0.),
        ("C", 4., 0.),
        ("C", 2., 1.),
    ];
    let bonds = [(1, 2), (2, 3), (3, 4), (4, 5), (3, 6)];
    let sources = [
        v2000(
            &atoms,
            &bonds,
            "M  STY  1   1 SUP\nM  SAL   1  3   2   3   4\nM  SBL   1  3   1   4   5\nM  SMT   1 X\nM  SAP   1  3   2   1  1   3   6  2   4   5  3",
        ),
        v3000(
            &atoms,
            &bonds,
            "M  V30 1 SUP 0 ATOMS=(3 2 3 4) XBONDS=(3 1 4 5) LABEL=X SAP=(3 2 1 1) SAP=(3 3 6 2) SAP=(3 4 5 3)",
            1,
        ),
    ];
    for source in sources {
        let (document, _, notices) = read(&source)?.display_document()?;
        assert!(document.abbreviations.is_empty());
        assert_eq!(document.atoms.len(), atoms.len());
        assert_eq!(document.bonds.len(), bonds.len());
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(notices[0].contains("at most two atoms"), "{notices:?}");
    }
    Ok(())
}

#[test]
fn explicitly_expanded_superatoms_stay_expanded_without_a_notice() -> anyhow::Result<()> {
    let atoms = [("C", 0., 0.), ("O", 1.5, 0.), ("C", 3., 0.)];
    let bonds = [(1, 2), (2, 3)];
    for source in [
        v2000(
            &atoms,
            &bonds,
            "M  STY  1   1 SUP\nM  SAL   1  2   2   3\nM  SBL   1  1   1\nM  SMT   1 OMe\nM  SDS EXP  1   1",
        ),
        v3000(
            &atoms,
            &bonds,
            "M  V30 1 SUP 0 ATOMS=(2 2 3) XBONDS=(1 1) LABEL=OMe ESTATE=E",
            1,
        ),
    ] {
        let (document, _, notices) = read(&source)?.display_document()?;
        assert!(document.abbreviations.is_empty());
        assert!(notices.is_empty(), "{notices:?}");
        assert_eq!(document.atoms.len(), 3);
        assert_eq!(identity(&document)?, "COC");
    }
    Ok(())
}

#[test]
fn ordinary_member_hydrogen_folds_before_superatom_restoration() -> anyhow::Result<()> {
    let atoms = [("C", 0., 0.), ("H", 3., 1.), ("O", 1.5, 0.), ("C", 3., 0.)];
    let bonds = [(1, 3), (3, 4), (4, 2)];
    for source in [
        v2000(
            &atoms,
            &bonds,
            "M  STY  1   1 SUP\nM  SAL   1  3   3   4   2\nM  SBL   1  1   1\nM  SMT   1 OMe\nM  SAP   1  1   3   1  1",
        ),
        v3000(
            &atoms,
            &bonds,
            "M  V30 1 SUP 0 ATOMS=(3 3 4 2) XBONDS=(1 1) LABEL=OMe SAP=(3 3 1 1)",
            1,
        ),
    ] {
        let imported = read(&source)?;
        assert_eq!(imported.molecule.ids.len(), 4);
        let (document, molecule, notices) = imported.display_document()?;
        assert_eq!(molecule.ids, [1, 3, 4]);
        assert_eq!(document.atoms.len(), 3);
        assert!(document.atoms.iter().all(|atom| atom.element != "H"));
        assert!(notices.is_empty(), "{notices:?}");
        let [group] = document.abbreviations.as_slice() else {
            panic!("Expected OMe after folding its ordinary H");
        };
        assert_eq!(group.anchor, 3);
        assert_eq!(group.members, [3, 4]);
        assert_eq!(identity(&document)?, "COC");
    }
    Ok(())
}

#[test]
fn folded_sap_hydrogen_keeps_the_group_expanded_with_a_notice() -> anyhow::Result<()> {
    let atoms = [("C", 0., 0.), ("H", 3., 1.), ("O", 1.5, 0.), ("C", 3., 0.)];
    let bonds = [(1, 3), (3, 4), (4, 2)];
    for source in [
        v2000(
            &atoms,
            &bonds,
            "M  STY  1   1 SUP\nM  SAL   1  3   3   4   2\nM  SBL   1  1   1\nM  SMT   1 OMe\nM  SAP   1  1   2   0  1",
        ),
        v3000(
            &atoms,
            &bonds,
            "M  V30 1 SUP 0 ATOMS=(3 3 4 2) XBONDS=(1 1) LABEL=OMe SAP=(3 2 0 1)",
            1,
        ),
    ] {
        let (document, molecule, notices) = read(&source)?.display_document()?;
        assert_eq!(molecule.ids, [1, 3, 4]);
        assert_eq!(document.atoms.len(), 3);
        assert!(document.abbreviations.is_empty());
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(
            notices[0].contains("attachment points do not match"),
            "{notices:?}"
        );
        assert_eq!(identity(&document)?, "COC");
    }
    Ok(())
}

#[test]
fn invalid_display_groups_report_a_notice_and_keep_the_full_graph() -> anyhow::Result<()> {
    let atoms = [("C", 0., 0.), ("O", 1.5, 0.), ("C", 3., 0.)];
    let bonds = [(1, 2), (2, 3)];
    let long_label = "X".repeat(33);
    for (case, v2, v3, group_count, retained, reason) in [
        (
            "missing label",
            "M  STY  1   1 SUP\nM  SAL   1  2   2   3".to_owned(),
            "M  V30 1 SUP 0 ATOMS=(2 2 3)".to_owned(),
            1,
            0,
            "label",
        ),
        (
            "long label",
            format!("M  STY  1   1 SUP\nM  SAL   1  2   2   3\nM  SMT   1 {long_label}"),
            format!("M  V30 1 SUP 0 ATOMS=(2 2 3) LABEL={long_label}"),
            1,
            0,
            "32 characters",
        ),
        (
            "disconnected members",
            "M  STY  1   1 SUP\nM  SAL   1  2   1   3\nM  SMT   1 X".to_owned(),
            "M  V30 1 SUP 0 ATOMS=(2 1 3) LABEL=X".to_owned(),
            1,
            0,
            "connected",
        ),
        (
            "overlapping groups",
            "M  STY  1   1 SUP\nM  SAL   1  2   2   3\nM  SMT   1 OMe\nM  STY  1   2 SUP\nM  SAL   2  2   2   3\nM  SMT   2 Other".to_owned(),
            "M  V30 1 SUP 0 ATOMS=(2 2 3) LABEL=OMe\nM  V30 2 SUP 0 ATOMS=(2 2 3) LABEL=Other".to_owned(),
            2,
            1,
            "overlap",
        ),
    ] {
        for source in [
            v2000(&atoms, &bonds, &v2),
            v3000(&atoms, &bonds, &v3, group_count),
        ] {
            let (document, _, notices) = read(&source)?.display_document()?;
            assert_eq!(document.atoms.len(), atoms.len(), "{case}");
            assert_eq!(document.bonds.len(), bonds.len(), "{case}");
            assert_eq!(document.abbreviations.len(), retained, "{case}");
            assert_eq!(notices.len(), 1, "{case}: {notices:?}");
            assert!(
                notices[0].to_lowercase().contains(reason),
                "{case}: {notices:?}"
            );
            assert_eq!(identity(&document)?, "COC", "{case}");
        }
    }
    Ok(())
}

#[test]
fn standalone_superatom_labels_preserve_punctuation_spaces_quotes_and_unicode() -> anyhow::Result<()>
{
    for label in ["X-", "O Me", "O\"Me", "'Me", "保護基"] {
        let mut source = ether();
        source.abbreviations = vec![abbreviation(label, 1, None, &[1, 2, 3])];
        for force_v3000 in [false, true] {
            let output = export(&source, force_v3000)?;
            let imported = read(&output)?;
            assert_eq!(imported.annotations.superatoms[0].label, label, "{output}");
            assert_eq!(
                imported.annotations.superatoms[0].attachments,
                [(1, "1".into())]
            );
            let (reopened, _, notices) = imported.display_document()?;
            assert!(notices.is_empty(), "{label}: {notices:?}");
            assert_eq!(reopened.abbreviations[0].label, label);
            assert_eq!(reopened.abbreviations[0].members, [1, 2, 3]);
            assert_eq!(identity(&reopened)?, identity(&source)?);
        }
    }
    Ok(())
}

#[test]
fn a_single_sap_chooses_the_anchor_of_a_standalone_group() -> anyhow::Result<()> {
    let atoms = [("C", 0., 0.), ("O", 1.5, 0.), ("C", 3., 0.)];
    let bonds = [(1, 2), (2, 3)];
    for source in [
        v2000(
            &atoms,
            &bonds,
            "M  STY  1   1 SUP\nM  SAL   1  3   1   2   3\nM  SMT   1 X\nM  SAP   1  1   2   0  1",
        ),
        v3000(
            &atoms,
            &bonds,
            "M  V30 1 SUP 0 ATOMS=(3 1 2 3) LABEL=X SAP=(3 2 0 1)",
            1,
        ),
    ] {
        let (document, _, notices) = read(&source)?.display_document()?;
        assert!(notices.is_empty(), "{notices:?}");
        assert_eq!(document.abbreviations[0].anchor, 2);
        assert_eq!(document.abbreviations[0].exit, None);
    }
    Ok(())
}

#[test]
fn saps_that_do_not_match_the_crossing_bonds_keep_the_group_expanded() -> anyhow::Result<()> {
    let atoms = [
        ("C", 0., 0.),
        ("C", 1., 0.),
        ("C", 2., 0.),
        ("C", 3., 0.),
        ("C", 4., 0.),
    ];
    let bonds = [(1, 2), (2, 3), (3, 4), (4, 5)];
    for source in [
        v2000(
            &atoms,
            &bonds,
            "M  STY  1   1 SUP\nM  SAL   1  3   2   3   4\nM  SBL   1  2   1   4\nM  SMT   1 X\nM  SAP   1  2   2   0 Al   3   0 Br",
        ),
        v3000(
            &atoms,
            &bonds,
            "M  V30 1 SUP 0 ATOMS=(3 2 3 4) XBONDS=(2 1 4) LABEL=X SAP=(3 2 0 Al) SAP=(3 3 0 Br)",
            1,
        ),
    ] {
        let (document, _, notices) = read(&source)?.display_document()?;
        assert!(document.abbreviations.is_empty());
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(
            notices[0].contains("attachment points do not match"),
            "{notices:?}"
        );
    }
    Ok(())
}

#[test]
fn sap_order_overrides_geometry_and_crossing_bond_storage_order() -> anyhow::Result<()> {
    let atoms = [
        ("Br", -3., 0.),
        ("C", -2., 0.),
        ("C", -1., 0.),
        ("Al", 0., 0.),
    ];
    let bonds = [(1, 2), (2, 3), (3, 4)];
    let sources = [
        v2000(
            &atoms,
            &bonds,
            "M  STY  1   1 SUP\nM  SAL   1  2   2   3\nM  SBL   1  2   1   3\nM  SMT   1 X\nM  SAP   1  2   3   4  1   2   1  2",
        ),
        v3000(
            &atoms,
            &bonds,
            "M  V30 1 SUP 0 ATOMS=(2 2 3) XBONDS=(2 1 3) LABEL=X SAP=(3 3 4 1) SAP=(3 2 1 2)",
            1,
        ),
        // HELM IDs determine the roles even when Br precedes Al in the file.
        v2000(
            &atoms,
            &bonds,
            "M  STY  1   1 SUP\nM  SAL   1  2   2   3\nM  SBL   1  2   1   3\nM  SMT   1 X\nM  SAP   1  2   2   1 Br   3   4 Al",
        ),
        v3000(
            &atoms,
            &bonds,
            "M  V30 1 SUP 0 ATOMS=(2 2 3) XBONDS=(2 1 3) LABEL=X SAP=(3 2 1 Br) SAP=(3 3 4 Al)",
            1,
        ),
    ];
    for source in sources {
        let (document, _, notices) = read(&source)?.display_document()?;
        assert!(notices.is_empty(), "{notices:?}");
        let [group] = document.abbreviations.as_slice() else {
            panic!("Expected a two-sided SUP: {source}");
        };
        assert_eq!(group.anchor, 3);
        assert_eq!(group.exit, Some(2));
        assert!(
            document.atom(group.anchor).unwrap().position.x
                > document.atom(group.exit.unwrap()).unwrap().position.x
        );
        for force_v3000 in [false, true] {
            let output = export(&document, force_v3000)?;
            let (reopened, _, notices) = read(&output)?.display_document()?;
            assert!(notices.is_empty(), "{notices:?}");
            assert_eq!(reopened.abbreviations[0].anchor, 3);
            assert_eq!(reopened.abbreviations[0].exit, Some(2));
            assert_eq!(identity(&reopened)?, identity(&document)?);
        }
    }
    Ok(())
}

#[test]
fn drawing_without_abbreviations_keeps_the_existing_bytes() -> anyhow::Result<()> {
    let mut source = ether();
    source.abbreviations.clear();
    let molecule = document::prepare(&source)?;
    for force_v3000 in [false, true] {
        let (expected, _) = write_drawing_absolute(&molecule, &source, Options { force_v3000 })?;
        assert_eq!(export(&source, force_v3000)?, expected);
    }
    Ok(())
}

#[test]
fn export_maps_members_by_prepared_ids_and_skips_incomplete_groups() -> anyhow::Result<()> {
    let source = ether();
    let mut reordered = source.clone();
    reordered.atoms.reverse();
    let molecule = document::prepare(&reordered)?;
    assert_eq!(molecule.ids, [3, 2, 1]);
    for force_v3000 in [false, true] {
        let base = write_absolute(&molecule, Options { force_v3000 })?;
        let output = superatoms::restore_output(base.clone(), &source, &molecule)?;
        let imported = read(&output)?;
        let [group] = imported.annotations.superatoms.as_slice() else {
            panic!("Expected mapped SUP: {output}");
        };
        assert_eq!(group.members, [2, 1]);
        assert_eq!(group.attachments, [(2, "1".into())]);
        let (reopened, _, notices) = imported.display_document()?;
        assert!(notices.is_empty(), "{notices:?}");
        assert_eq!(reopened.abbreviations[0].anchor, 2);
        let mut members = reopened.abbreviations[0].members.clone();
        members.sort_unstable();
        assert_eq!(members, [1, 2]);
        assert_eq!(identity(&reopened)?, identity(&source)?);

        let mut incomplete = source.clone();
        incomplete.abbreviations[0].members.push(999);
        assert_eq!(
            superatoms::restore_output(base.clone(), &incomplete, &molecule)?,
            base,
            "A partial export cannot retain a label that conceals missing members"
        );
    }
    Ok(())
}

#[test]
fn large_superatom_chunks_v2000_members_and_wraps_v3000_unicode() -> anyhow::Result<()> {
    let mut source = Document::default();
    for index in 0..24 {
        source.add_atom("C", Point::new(index as f32 * 42., 0.));
    }
    for id in 1..24 {
        source.add_bond(id, id + 1, 1, "plain");
    }
    let members: Vec<_> = (1..=24).collect();
    let label = "保護".repeat(16);
    source
        .abbreviations
        .push(abbreviation(&label, 1, None, &members));
    for force_v3000 in [false, true] {
        let output = export(&source, force_v3000)?;
        if force_v3000 {
            assert!(output.lines().any(|line| line.ends_with('-')), "{output}");
        } else {
            let chunks: Vec<_> = output
                .lines()
                .filter(|line| line.starts_with("M  SAL "))
                .map(|line| line.split_whitespace().nth(3).unwrap().parse::<usize>())
                .collect::<Result<_, _>>()?;
            assert_eq!(chunks, [15, 9]);
        }
        let imported = read(&output)?;
        assert_eq!(imported.annotations.superatoms[0].label, label);
        assert_eq!(imported.annotations.superatoms[0].members, members);
        let (reopened, _, notices) = imported.display_document()?;
        assert!(notices.is_empty(), "{notices:?}");
        assert_eq!(reopened.abbreviations[0].label, label);
        assert_eq!(reopened.abbreviations[0].members, members);
        assert_eq!(identity(&reopened)?, identity(&source)?);
        assert_positions_preserved(&source, &reopened);
    }
    Ok(())
}

#[test]
fn sup_and_sru_share_one_numbered_v3000_sgroup_block() -> anyhow::Result<()> {
    let mut source = Document::default();
    for (index, element) in ["C", "O", "C", "C", "N", "O", "C"].into_iter().enumerate() {
        source.add_atom(element, Point::new(index as f32 * 42., 0.));
    }
    source.add_bond(1, 2, 1, "plain");
    source.add_bond(2, 3, 1, "plain");
    source.add_bond(7, 4, 1, "plain");
    source.add_bond(4, 5, 1, "plain");
    source.add_bond(5, 6, 1, "plain");
    source
        .abbreviations
        .push(abbreviation("OMe", 2, None, &[2, 3]));
    source.generic.repeats.push(RepeatUnit {
        bracket: None,
        bracket_objects: vec![],
        members: vec![4, 5],
        crossings: vec![
            Crossing {
                inside: 4,
                outside: 7,
            },
            Crossing {
                inside: 5,
                outside: 6,
            },
        ],
        usage: "SRU".into(),
        crossing_brackets: vec![],
        head_count: Some(1),
        pattern: RepeatPattern::HeadToTail,
        count: RepeatCount::Indefinite { label: "n".into() },
        label: "n".into(),
        flip: "Unspecified".into(),
        label_offset: None,
    });
    let output = write_document(&source)?;
    assert!(output.contains("V3000"));
    assert_eq!(output.matches("M  V30 BEGIN SGROUP").count(), 1, "{output}");
    assert_eq!(output.matches("M  V30 END SGROUP").count(), 1, "{output}");
    let counts = output
        .lines()
        .find_map(|line| line.strip_prefix("M  V30 COUNTS "))
        .unwrap();
    assert_eq!(counts.split_whitespace().nth(2), Some("2"));
    let mut in_groups = false;
    let mut group_ids = vec![];
    for line in output.lines() {
        if line == "M  V30 BEGIN SGROUP" {
            in_groups = true;
            continue;
        }
        if line == "M  V30 END SGROUP" {
            in_groups = false;
        }
        if in_groups {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields
                .get(3)
                .is_some_and(|kind| matches!(*kind, "SUP" | "SRU"))
            {
                group_ids.push(fields[2].parse::<usize>()?);
            }
        }
    }
    group_ids.sort_unstable();
    assert_eq!(group_ids, [1, 2]);
    let (reopened, _, notices) = read(&output)?.display_document()?;
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(reopened.abbreviations.len(), 1);
    assert_eq!(reopened.abbreviations[0].label, "OMe");
    assert_eq!(reopened.generic.repeats.len(), 1);
    assert_eq!(
        reopened.generic.repeats[0].members,
        source.generic.repeats[0].members
    );
    assert_eq!(
        reopened.generic.repeats[0].crossings,
        source.generic.repeats[0].crossings
    );
    assert_eq!(
        reopened.generic.repeats[0].count,
        source.generic.repeats[0].count
    );
    Ok(())
}
