use super::*;
use crate::chemistry::{depict, stereo::perception::RingKind};

#[test]
fn projected_inchi_retains_captured_stereo_without_guessing_from_xy() -> anyhow::Result<()> {
    for (text, expected) in [
        ("FC=CF", "InChI=1S/C2H2F2/c3-1-2-4/h1-2H"),
        ("F/C=C/F", "InChI=1S/C2H2F2/c3-1-2-4/h1-2H/b2-1+"),
        ("F/C=C\\F", "InChI=1S/C2H2F2/c3-1-2-4/h1-2H/b2-1-"),
        ("FC(Cl)(Br)I", "InChI=1S/CBrClFI/c2-1(3,4)5"),
        ("F[C@](Cl)(Br)I", "InChI=1S/CBrClFI/c2-1(3,4)5/t1-/m1/s1"),
        ("F[C@@](Cl)(Br)I", "InChI=1S/CBrClFI/c2-1(3,4)5/t1-/m0/s1"),
    ] {
        let mut state = smiles::prepare(text)?.state;
        state.rings.kind = RingKind::Symmetric;
        let ranks = state
            .properties
            .atoms
            .iter()
            .map(|atom| atom.cip_rank)
            .collect::<Vec<_>>();
        let conformer = depict::compute(&state, &ranks, None, Default::default())?;
        let molecule = molecular::Molecule {
            rdkit_version: chemistry::RDKIT_VERSION,
            ids: (1..=state.graph.atoms.len() as u64).collect(),
            positions: conformer.positions,
            state,
        };
        let before = serde_json::to_value(&molecule)?;
        let (actual_smiles, input) = prepare_identifiers(&molecule, true)?;
        assert_eq!(
            actual_smiles,
            smiles::write::write(&molecule.state, Default::default())?.text
        );
        let input = input.ok_or_else(|| anyhow::anyhow!("Missing InChI input"))?;
        assert!(input.positions.is_none());
        assert_eq!(
            kernel::generate(&input).map_err(anyhow::Error::msg)?.inchi,
            expected,
            "{text}"
        );
        assert_eq!(serde_json::to_value(&molecule)?, before);
    }
    Ok(())
}

#[tokio::test]
async fn projected_engine_analyze_and_exports_preserve_the_drawing() -> anyhow::Result<()> {
    if std::env::var_os("RESHIKI_INCHI_HELPER").is_none() {
        eprintln!("Set RESHIKI_INCHI_HELPER to run projected real-engine response coverage");
        return Ok(());
    }
    for text in [
        "F/C=C/F",
        "F/C=C\\F",
        "FC=CF",
        "F[C@](Cl)(Br)I",
        "FC(Cl)(Br)I",
    ] {
        let imported = smiles::prepare(text)?;
        let mut state = imported.state;
        state.rings.kind = RingKind::Symmetric;
        let ranks = state
            .properties
            .atoms
            .iter()
            .map(|atom| atom.cip_rank)
            .collect::<Vec<_>>();
        let conformer = depict::compute(&state, &ranks, None, Default::default())?;
        let molecule = molecular::Molecule {
            rdkit_version: chemistry::RDKIT_VERSION,
            ids: (1..=state.graph.atoms.len() as u64).collect(),
            positions: conformer.positions,
            state,
        };
        let expected_smiles = smiles::write::write(&molecule.state, Default::default())?.text;
        let expected_inchi = kernel::generate(&kernel::Molecule::prepare(&molecule.state, None)?)
            .map_err(anyhow::Error::msg)?
            .inchi;
        let drawing = molecular::for_import(&molecule, false, &imported.dummy_labels)?;
        let mut document = drawing.clone().finish(drawing.labels()?)?;
        for (i, atom) in document.atoms.iter_mut().enumerate() {
            // Coincident XY is a legal projection of separated depths and must
            // never be passed to the ordinary wedge-construction path.
            atom.position = crate::document::Point::default();
            atom.depth = (i + 1) as f32 * 28.;
        }
        for bond in &mut document.bonds {
            bond.stereo_authoritative = bond.order == 2;
            bond.z_order = 7;
        }
        let before = serde_json::to_value(&document)?;
        for (operation, format) in [
            ("analyze", None),
            ("export", Some("smiles")),
            ("export", Some("inchi")),
            ("export", Some("mol")),
            ("export", Some("cdxml")),
        ] {
            let mut request = Request::molecule(operation, document.clone());
            request.format = format.map(str::to_owned);
            let response = execute(request).await?;
            let analysis = response
                .analysis
                .ok_or_else(|| anyhow::anyhow!("Missing projected analysis"))?;
            assert_eq!(analysis.smiles, expected_smiles, "{text}/{format:?}");
            assert_eq!(analysis.inchi, expected_inchi, "{text}/{format:?}");
            assert_eq!(
                serde_json::to_value(response.document)?,
                before,
                "Drawing changed: {text}/{operation}/{format:?}"
            );
            if format == Some("mol") {
                let output = response
                    .output
                    .ok_or_else(|| anyhow::anyhow!("Missing MOL"))?;
                let imported = molfile::read(&output)?;
                assert_eq!(
                    smiles::write::write(&imported.molecule.state, Default::default())?.text,
                    expected_smiles,
                    "{text} MOL identity"
                );
            }
        }
        assert_eq!(serde_json::to_value(&document)?, before);
        if let Some(index) = document
            .bonds
            .iter()
            .position(|bond| matches!(bond.display.as_str(), "wedge" | "hash"))
        {
            // Older depth-only wedge paint remains explicitly unsupported in
            // CDXML. New 3D geometry retains the original chemical wedge paint.
            let mut unsupported = document.clone();
            unsupported.bonds[index].projection = true;
            let unsupported_before = serde_json::to_value(&unsupported)?;
            let mut request = Request::molecule("export", unsupported.clone());
            request.format = Some("cdxml".into());
            let error = execute(request)
                .await
                .expect_err("CDXML accepted projected chemical wedge paint");
            assert!(error.to_string().contains("projected wedge styles"));
            assert_eq!(serde_json::to_value(&unsupported)?, unsupported_before);
        }
    }
    Ok(())
}

// Independent literals pin user-visible text rather than following production constants.
const LOSS_ATTACHMENT: &str = "Attachment targets retained. Complex identifiers and coordination-valence analysis are unavailable. Formula, when shown, counts the defined atoms and ligands with charges as entered.";
const LOSS_REAR: &str = "This chemical format does not retain rear opacity. Chemical structure and XYZ are unchanged; use native ReShiki for editable appearance or SVG/PDF/PNG for the figure.";
const LOSS_ANCHOR: &str = "Arrow curves are preserved; editing attachment links are omitted from this external format. Chemistry is unchanged. Use a native ReShiki file to retain links.";
const LOSS_DATIVE_MOL: &str = "MOL retains directed dative chemistry and metal charge, but omits solid/hashed coordination projection appearance. Use native ReShiki or figure export to retain that drawing.";
const LOSS_DATIVE_DRAWING: &str = "CDXML/CDX retains directed coordination chemistry and ReShiki projection attributes. ChemDraw 26 displays dative arrows and removes wedge/hash paint on save. Use native ReShiki or figure export for faithful projection appearance.";
const LOSS_REACTION_ROLES: &str = "This drawing or molecule format does not retain reaction roles. Use RXN/reaction SMILES for reaction data, or .reshiki for the complete scheme.";
const LOSS_UNVALIDATED: &str = "Drawing exported; chemical assignments remain unvalidated.";
const LOSS_REACTION_DRAWING: &str = "Reaction files preserve participants, atom maps and stereo. Save .reshiki to retain captions, arrow appearance and drawing layout.";
const LOSS_REAR_DRAWING_ERROR: &str = "Invalid editable drawing: CDXML/CDX cannot preserve rear opacity. Use native ReShiki for editable appearance or SVG, PDF or PNG for the figure. Set rear opacity to 100% for editable CDXML/CDX.";
const LOSS_ATTACHMENT_IDENTIFIER_ERROR: &str = "Invalid drawing: Multi-center/variable attachment targets require CDXML, CDX, V3000 MOL or native ReShiki export; ordinary molecular identifiers cannot retain them.";
const LOSS_INVALID_VALENCE_ERROR: &str =
    "Properties: Atom 1: Explicit valence 5 is too large for C";

#[derive(Clone, Copy, Debug)]
enum LossNoticeBase {
    Prepared,
    Attachment,
    Unvalidated,
}

#[derive(Clone, Copy, Debug)]
enum LossNoticeDecoration {
    Plain,
    Rear,
    Anchor,
    Dative,
    Reaction,
    ReactionRear,
    ReactionAnchor,
    ReactionAnchorDative,
    ReactionRearDative,
}

impl LossNoticeDecoration {
    fn rear(self) -> bool {
        matches!(
            self,
            Self::Rear | Self::ReactionRear | Self::ReactionRearDative
        )
    }

    fn anchor(self) -> bool {
        matches!(
            self,
            Self::Anchor | Self::ReactionAnchor | Self::ReactionAnchorDative
        )
    }

    fn dative(self) -> bool {
        matches!(
            self,
            Self::Dative | Self::ReactionAnchorDative | Self::ReactionRearDative
        )
    }

    fn reaction(self) -> bool {
        matches!(
            self,
            Self::Reaction
                | Self::ReactionRear
                | Self::ReactionAnchor
                | Self::ReactionAnchorDative
                | Self::ReactionRearDative
        )
    }
}

#[derive(Clone, Copy, Debug)]
enum LossNoticeExpected {
    Analyzed(&'static [&'static str]),
    Unanalyzed(&'static [&'static str]),
    Error(&'static str),
}

fn loss_notice_helper_available() -> bool {
    if std::env::var_os("RESHIKI_INCHI_HELPER").is_some() {
        return true;
    }
    assert!(
        std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none(),
        "RESHIKI_REQUIRE_INCHI_HELPER requires RESHIKI_INCHI_HELPER for export notice coverage"
    );
    eprintln!("Set RESHIKI_INCHI_HELPER to run prepared real-engine export notice coverage");
    false
}

fn loss_notice_document(
    base: LossNoticeBase,
    decoration: LossNoticeDecoration,
) -> anyhow::Result<Document> {
    use crate::{
        arrow_anchors::{self, Pick},
        arrows::{ArrowStyle, Preset},
        document::{Arrow, Point},
        reactions::{Participant, Reaction},
    };

    let mut document = Document::default();
    let carbon = document.add_atom("C", Point::default());
    if matches!(base, LossNoticeBase::Unvalidated) {
        // One neutral carbon carrying five explicit H is drawing-valid but
        // fails chemical sanitization at Properties (carbon's maximum valence is four).
        document.atom_mut(carbon).unwrap().explicit_h = 5;
    } else {
        let second = document.add_atom("C", Point::new(28., 0.));
        document.add_bond(carbon, second, 1, "plain");
        if matches!(base, LossNoticeBase::Attachment) {
            let attachment = crate::attachments::add(
                &mut document,
                &[carbon, second],
                crate::attachments::Kind::MultiCenter,
            )
            .map_err(anyhow::Error::msg)?;
            let metal = document.add_atom("Cu", Point::new(14., 56.));
            document.atom_mut(metal).unwrap().charge = 2;
            // V3000 can retain ALL targets only with one supported contact.
            document.add_bond(attachment, metal, 5, "plain");
        }
    }
    if decoration.dative() {
        let donor = document.add_atom("N", Point::new(112., 0.));
        let metal = document.add_atom("Cu", Point::new(140., 28.));
        document.atom_mut(metal).unwrap().charge = 2;
        document.add_bond(donor, metal, 5, "wedge");
        document.bonds.last_mut().unwrap().projection = true;
    }
    if decoration.anchor() {
        let destination = document
            .atoms
            .get(1)
            .map(|atom| atom.id)
            .unwrap_or_else(|| document.add_atom("O", Point::new(56., 0.)));
        arrow_anchors::create(
            &mut document,
            &Pick::Atom(carbon),
            &Pick::Atom(destination),
            Preset::Curved,
            ArrowStyle::preset(Preset::Curved),
        )
        .map_err(anyhow::Error::msg)?;
    }
    if decoration.reaction() {
        let reactants = document.atoms.iter().map(|atom| atom.id).collect();
        let product = document.add_atom("O", Point::new(252., 0.));
        let arrow = document.next_id();
        document.arrows.push(Arrow::new(
            arrow,
            Point::new(196., 0.),
            Point::new(224., 0.),
            Preset::Forward,
            ArrowStyle::preset(Preset::Forward),
        ));
        document.reactions.push(Reaction {
            arrow,
            reactants: vec![Participant {
                atoms: reactants,
                coefficient: 1,
            }],
            products: vec![Participant {
                atoms: vec![product],
                coefficient: 1,
            }],
            agents: vec![],
            annotations: vec![],
        });
    }
    if decoration.rear() {
        let ids = document
            .atoms
            .iter()
            .map(|atom| atom.id)
            .collect::<Vec<_>>();
        crate::depth_appearance::set_rear_opacity(&mut document, &ids, 0.5)
            .map_err(anyhow::Error::msg)?;
    }
    Ok(document)
}

fn loss_notice_validate_document(
    document: &Document,
    base: LossNoticeBase,
    decoration: LossNoticeDecoration,
    name: &str,
    format: &str,
) {
    document
        .validate()
        .unwrap_or_else(|error| panic!("{name}/{format}: invalid drawing: {error}"));
    assert_eq!(
        crate::attachments::present(document),
        matches!(base, LossNoticeBase::Attachment),
        "{name}/{format}: attachment predicate"
    );
    assert_eq!(
        crate::rear_opacity::present(document),
        decoration.rear(),
        "{name}/{format}: rear opacity predicate"
    );
    assert_eq!(
        crate::arrow_anchors::export_notice(document).is_some(),
        decoration.anchor(),
        "{name}/{format}: arrow-link export predicate"
    );
    assert_eq!(
        document.arrows.iter().any(|arrow| {
            arrow.kind == "curved" && arrow.start_anchor.is_some() && arrow.end_anchor.is_some()
        }),
        decoration.anchor(),
        "{name}/{format}: anchored mechanism arrow predicate"
    );
    assert_eq!(
        document
            .bonds
            .iter()
            .any(|bond| bond.order == 5 && bond.projection),
        decoration.dative(),
        "{name}/{format}: projected dative predicate"
    );
    assert_eq!(
        !document.reactions.is_empty(),
        decoration.reaction(),
        "{name}/{format}: reaction export predicate"
    );
    assert_eq!(
        document.reactions.iter().any(|reaction| reaction.ready()),
        decoration.reaction(),
        "{name}/{format}: reaction role predicate"
    );
    if matches!(base, LossNoticeBase::Unvalidated) {
        assert!(
            matches!(
                molecular::prepare(document),
                Err(molecular::Error::Sanitization(_))
            ),
            "{name}/{format}: fallback fixture must fail sanitization"
        );
    } else if matches!(base, LossNoticeBase::Prepared) {
        molecular::prepare(document)
            .unwrap_or_else(|error| panic!("{name}/{format}: prepared fixture failed: {error}"));
    }
}

fn loss_notice_assert_response<E: std::fmt::Debug + std::fmt::Display>(
    result: Result<Response, E>,
    expected: LossNoticeExpected,
    document: &str,
    format: &str,
) {
    match expected {
        LossNoticeExpected::Error(expected) => {
            let error = match result {
                Err(error) => error,
                Ok(response) => panic!("{document}/{format}: export succeeded: {response:?}"),
            };
            assert_eq!(error.to_string(), expected, "{document}/{format}");
        }
        LossNoticeExpected::Analyzed(warnings) | LossNoticeExpected::Unanalyzed(warnings) => {
            let response = result
                .unwrap_or_else(|error| panic!("{document}/{format}: export failed: {error}"));
            assert_eq!(
                response
                    .warnings
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                warnings,
                "{document}/{format}: ordered warnings"
            );
            assert_eq!(
                response.analysis.is_some(),
                matches!(expected, LossNoticeExpected::Analyzed(_)),
                "{document}/{format}: analysis presence"
            );
            assert!(
                response.output.is_some_and(|output| !output.is_empty()),
                "{document}/{format}: missing export"
            );
        }
    }
}

#[tokio::test]
async fn prepared_export_loss_notices_are_ordered_by_document_and_format() -> anyhow::Result<()> {
    use LossNoticeDecoration::*;
    use LossNoticeExpected::{Analyzed, Error};

    if !loss_notice_helper_available() {
        return Ok(());
    }
    let formats = ["mol", "cdxml", "cdx", "smiles"];
    let cases = [
        (
            "prepared baseline",
            Plain,
            [Analyzed(&[]), Analyzed(&[]), Analyzed(&[]), Analyzed(&[])],
        ),
        // Current behavior: CDXML/CDX rejects rear opacity instead of returning a loss notice.
        (
            "prepared rear opacity",
            Rear,
            [
                Analyzed(&[LOSS_REAR]),
                Error(LOSS_REAR_DRAWING_ERROR),
                Error(LOSS_REAR_DRAWING_ERROR),
                Analyzed(&[LOSS_REAR]),
            ],
        ),
        // Current behavior: MOL/SMILES omit the mechanism-arrow editing-link notice.
        (
            "prepared anchored mechanism arrow",
            Anchor,
            [
                Analyzed(&[]),
                Analyzed(&[LOSS_ANCHOR]),
                Analyzed(&[LOSS_ANCHOR]),
                Analyzed(&[]),
            ],
        ),
        // Current behavior: SMILES emits no projected dative appearance notice.
        (
            "prepared projected dative",
            Dative,
            [
                Analyzed(&[LOSS_DATIVE_MOL]),
                Analyzed(&[LOSS_DATIVE_DRAWING]),
                Analyzed(&[LOSS_DATIVE_DRAWING]),
                Analyzed(&[]),
            ],
        ),
        (
            "prepared reaction roles",
            Reaction,
            [
                Analyzed(&[LOSS_REACTION_ROLES]),
                Analyzed(&[LOSS_REACTION_ROLES]),
                Analyzed(&[LOSS_REACTION_ROLES]),
                Analyzed(&[LOSS_REACTION_ROLES]),
            ],
        ),
        // Current behavior: finish orders reaction roles, arrow links, then dative paint;
        // MOL/SMILES still omit the arrow-link notice and SMILES omits dative paint.
        (
            "prepared reaction with anchored arrow and projected dative",
            ReactionAnchorDative,
            [
                Analyzed(&[LOSS_REACTION_ROLES, LOSS_DATIVE_MOL]),
                Analyzed(&[LOSS_REACTION_ROLES, LOSS_ANCHOR, LOSS_DATIVE_DRAWING]),
                Analyzed(&[LOSS_REACTION_ROLES, LOSS_ANCHOR, LOSS_DATIVE_DRAWING]),
                Analyzed(&[LOSS_REACTION_ROLES]),
            ],
        ),
        // Current behavior: MOL orders reaction roles, rear opacity, then dative paint.
        // CDXML/CDX still rejects the combined drawing and SMILES omits dative paint.
        (
            "prepared reaction with rear opacity and projected dative",
            ReactionRearDative,
            [
                Analyzed(&[LOSS_REACTION_ROLES, LOSS_REAR, LOSS_DATIVE_MOL]),
                Error(LOSS_REAR_DRAWING_ERROR),
                Error(LOSS_REAR_DRAWING_ERROR),
                Analyzed(&[LOSS_REACTION_ROLES, LOSS_REAR]),
            ],
        ),
    ];
    for (name, decoration, expected) in cases {
        let document = loss_notice_document(LossNoticeBase::Prepared, decoration)?;
        for (format, expected) in formats.into_iter().zip(expected) {
            loss_notice_validate_document(
                &document,
                LossNoticeBase::Prepared,
                decoration,
                name,
                format,
            );
            let mut request = Request::molecule("export", document.clone());
            request.format = Some(format.into());
            loss_notice_assert_response(execute(request).await, expected, name, format);
        }
    }
    Ok(())
}

#[tokio::test]
async fn attachment_early_export_loss_notices_are_pinned() -> anyhow::Result<()> {
    use LossNoticeDecoration::*;
    use LossNoticeExpected::{Error, Unanalyzed};

    let formats = ["mol", "cdxml", "cdx"];
    let cases = [
        (
            "attachment targets alone",
            Plain,
            [Unanalyzed(&[LOSS_ATTACHMENT]); 3],
        ),
        // Current behavior: MOL adds rear opacity after the attachment notice;
        // CDXML/CDX rejects rear opacity before returning any notice.
        (
            "attachment targets with rear opacity",
            Rear,
            [
                Unanalyzed(&[LOSS_ATTACHMENT, LOSS_REAR]),
                Error(LOSS_REAR_DRAWING_ERROR),
                Error(LOSS_REAR_DRAWING_ERROR),
            ],
        ),
        // Current behavior: the early attachment route omits the arrow-link notice.
        (
            "attachment targets with anchored mechanism arrow",
            Anchor,
            [Unanalyzed(&[LOSS_ATTACHMENT]); 3],
        ),
        // Current behavior: the early attachment route omits both dative paint notices.
        (
            "attachment targets with projected dative",
            Dative,
            [Unanalyzed(&[LOSS_ATTACHMENT]); 3],
        ),
        // Current behavior: the early attachment route omits the reaction-role notice.
        (
            "attachment targets with reaction roles",
            Reaction,
            [Unanalyzed(&[LOSS_ATTACHMENT]); 3],
        ),
    ];
    for (name, decoration, expected) in cases {
        let document = loss_notice_document(LossNoticeBase::Attachment, decoration)?;
        for (format, expected) in formats.into_iter().zip(expected) {
            loss_notice_validate_document(
                &document,
                LossNoticeBase::Attachment,
                decoration,
                name,
                format,
            );
            let mut request = Request::molecule("export", document.clone());
            request.format = Some(format.into());
            loss_notice_assert_response(execute(request).await, expected, name, format);
        }
    }
    // Current behavior: SMILES bypasses early attachment export and fails preparation.
    let document = loss_notice_document(LossNoticeBase::Attachment, Plain)?;
    loss_notice_validate_document(
        &document,
        LossNoticeBase::Attachment,
        Plain,
        "attachment targets alone",
        "smiles",
    );
    let mut request = Request::molecule("export", document);
    request.format = Some("smiles".into());
    loss_notice_assert_response(
        execute(request).await,
        Error(LOSS_ATTACHMENT_IDENTIFIER_ERROR),
        "attachment targets alone",
        "smiles",
    );
    Ok(())
}

#[tokio::test]
async fn unprepared_drawing_export_loss_notices_are_pinned() -> anyhow::Result<()> {
    use LossNoticeDecoration::*;
    use LossNoticeExpected::{Error, Unanalyzed};

    let formats = ["cdxml", "cdx"];
    let cases = [
        (
            "unsanitizable carbon alone",
            Plain,
            [Unanalyzed(&[LOSS_UNVALIDATED]); 2],
        ),
        // Current behavior: fallback CDXML/CDX rejects rear opacity before returning notices.
        (
            "unsanitizable carbon with rear opacity",
            Rear,
            [Error(LOSS_REAR_DRAWING_ERROR); 2],
        ),
        (
            "unsanitizable carbon with anchored mechanism arrow",
            Anchor,
            [Unanalyzed(&[LOSS_UNVALIDATED, LOSS_ANCHOR]); 2],
        ),
        // Current behavior: the unprepared fallback omits the dative paint notice.
        (
            "unsanitizable carbon with projected dative",
            Dative,
            [Unanalyzed(&[LOSS_UNVALIDATED]); 2],
        ),
        // Current behavior: the unprepared fallback omits the reaction-role notice.
        (
            "unsanitizable carbon with reaction roles",
            Reaction,
            [Unanalyzed(&[LOSS_UNVALIDATED]); 2],
        ),
    ];
    for (name, decoration, expected) in cases {
        let document = loss_notice_document(LossNoticeBase::Unvalidated, decoration)?;
        for (format, expected) in formats.into_iter().zip(expected) {
            loss_notice_validate_document(
                &document,
                LossNoticeBase::Unvalidated,
                decoration,
                name,
                format,
            );
            let mut request = Request::molecule("export", document.clone());
            request.format = Some(format.into());
            loss_notice_assert_response(execute(request).await, expected, name, format);
        }
    }
    // Current behavior: MOL/SMILES have no drawing-preserving sanitization fallback.
    let document = loss_notice_document(LossNoticeBase::Unvalidated, Plain)?;
    for format in ["mol", "smiles"] {
        loss_notice_validate_document(
            &document,
            LossNoticeBase::Unvalidated,
            Plain,
            "unsanitizable carbon alone",
            format,
        );
        let mut request = Request::molecule("export", document.clone());
        request.format = Some(format.into());
        loss_notice_assert_response(
            execute(request).await,
            Error(LOSS_INVALID_VALENCE_ERROR),
            "unsanitizable carbon alone",
            format,
        );
    }
    Ok(())
}

#[tokio::test]
async fn reaction_export_loss_notices_are_ordered() -> anyhow::Result<()> {
    use LossNoticeDecoration::*;
    use LossNoticeExpected::Unanalyzed;

    let cases = [
        (
            "reaction roles alone",
            Reaction,
            Unanalyzed(&[LOSS_REACTION_DRAWING]),
        ),
        (
            "reaction roles with rear opacity",
            ReactionRear,
            Unanalyzed(&[LOSS_REACTION_DRAWING, LOSS_REAR]),
        ),
        // Current behavior: RXN/reaction SMILES emits only its generic drawing
        // warning for an anchored mechanism arrow, omitting the arrow-link notice.
        (
            "reaction roles with anchored mechanism arrow",
            ReactionAnchor,
            Unanalyzed(&[LOSS_REACTION_DRAWING]),
        ),
    ];
    for (name, decoration, expected) in cases {
        let document = loss_notice_document(LossNoticeBase::Prepared, decoration)?;
        for format in ["rxn", "rsmi"] {
            loss_notice_validate_document(
                &document,
                LossNoticeBase::Prepared,
                decoration,
                name,
                format,
            );
            let mut request = Request::molecule("export", document.clone());
            request.format = Some(format.into());
            loss_notice_assert_response(
                crate::engine::LocalEngine::default().request(request).await,
                expected,
                name,
                format,
            );
        }
    }
    Ok(())
}
