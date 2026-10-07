//! Pins assemble_cdxml's stage order, the IDs and object bindings it assigns,
//! its atom patches, and the checks it applies to a prepared snapshot. Every
//! expectation was observed on the assembly before its stage split.
//!
//! Faults are injected immediately before the last `</page>` of the prepared
//! expanded XML, which keeps every existing ordinal and fragment binding valid.
//! Only integers, strings, booleans and exact Display text are pinned, never
//! positions, so the pins hold on every CI platform.
use super::super::prepare_cdxml;
use super::*;
use crate::palette::Color;
use std::ops::RangeInclusive;

const ATOM_LABELS: &str =
    include_str!("../../../../../../tests/fixtures/atom-labels-chemdraw.cdxml");
const ATTACHED_SYMBOLS: &str =
    include_str!("../../../../../../tests/fixtures/attached-symbols-chemdraw.cdxml");
const LABEL_INK: &str = include_str!(
    "../../../../../../tests/fixtures/structure-highlights/native-independent-label-ink.cdxml"
);
const CONTRACTED: &str =
    include_str!("../../../../../../tests/fixtures/structure-highlights/native-contracted.cdxml");
const GROUPED: &str =
    include_str!("../../../../../../tests/fixtures/grouped-aspirin-chemdraw.cdxml");
const REACTION: &str =
    include_str!("../../../../../../tests/fixtures/chemdraw-arrows/reaction.cdxml");
const GRAPHICS: &str = include_str!("../../../../../../tests/fixtures/graphics-chemdraw.cdxml");
const SYMBOLS: &str = include_str!("../../../../../../tests/fixtures/symbols-chemdraw.cdxml");
const GALLERY: &str =
    include_str!("../../../../../../tests/fixtures/chemdraw-captions/gallery.cdxml");
const AROMATIC_CIRCLE: &str =
    include_str!("../../../../../../tests/fixtures/aromatic-circle-native.cdxml");
const EMPTY: &str = "<CDXML><page></page></CDXML>";

/// Arrow attributes copied from chemdraw-arrows/reaction.cdxml.
const ARROW_38: &str = "<arrow id='38' BoundingBox='202.48 382.59 231.28 385.59' Z='33' FillType='None' ArrowheadHead='Full' ArrowheadType='Solid' HeadSize='600' ArrowheadCenterSize='525' ArrowheadWidth='150' Head3D='231.28 384.59 0' Tail3D='202.48 384.59 0' Center3D='216.88 384.59 0' MajorAxisEnd3D='245.68 384.59 0' MinorAxisEnd3D='216.88 413.39 0'/>";
const ARROW_39: &str = "<arrow id='39' BoundingBox='288.38 382.59 317.18 385.59' Z='34' FillType='None' ArrowheadHead='Full' ArrowheadType='Solid' HeadSize='600' ArrowheadCenterSize='525' ArrowheadWidth='150' Head3D='317.18 384.59 0' Tail3D='288.38 384.59 0' Center3D='302.78 384.59 0' MajorAxisEnd3D='331.58 384.59 0' MinorAxisEnd3D='302.78 413.39 0'/>";

const MIXED_FONTS: &str = "Mixed fonts within a CDXML atom label are not supported yet";
const MIXED_HYDROGEN_COLORS: &str = "Mixed colors within attached hydrogen text are not supported";
const LIMIT: &str = "CDXML scene exceeds its ID, object, or work limit";
const IDS_CHANGED: &str = "Prepared CDXML atom IDs changed";
const NOTHING: &str = "No supported drawing objects found";

/// The original label of CONTRACTED's abbreviation anchor. Expansion replaces
/// it with the abbreviation label, so only the saved presentation keeps it.
const ANCHOR_LABEL: &str = r#"<s font="21" size="10" color="0" face="96">O</s>"#;
/// The same label as OH2, with H and 2 in two different colors.
const ANCHOR_LABEL_MIXED_HYDROGEN: &str = r#"<s font="21" size="10" color="0" face="96">O</s><s font="21" size="10" color="4" face="96">H</s><s font="21" size="10" color="5" face="96">2</s>"#;
/// The same label as OH2, with all of H2 in red (color 4).
const ANCHOR_LABEL_RED_HYDROGEN: &str = r#"<s font="21" size="10" color="0" face="96">O</s><s font="21" size="10" color="4" face="96">H2</s>"#;

/// `text` with `from` replaced; `from` must occur once.
fn edit(text: &str, from: &str, to: &str) -> String {
    assert_eq!(text.matches(from).count(), 1, "{from}");
    text.replacen(from, to, 1)
}

/// Insert `element` immediately before the last `</page>`.
fn inject(prepared: &mut PreparedCdxml, element: &str) {
    let at = prepared.expanded_xml.rfind("</page>").unwrap();
    prepared.expanded_xml.insert_str(at, element);
}

/// The exact Display of the assembly error, or `ok`.
fn outcome(prepared: &PreparedCdxml) -> String {
    assemble_cdxml(prepared).map_or_else(|error| error.to_string(), |_| "ok".to_owned())
}

fn scene(text: &str) -> CdxmlScene {
    assemble_cdxml(&prepare_cdxml(text).unwrap()).unwrap()
}

enum Fault {
    /// Inserted into the prepared expanded XML.
    Inject(&'static str),
    /// Applied to the source before preparation.
    Source(&'static str, &'static str),
    /// Clear the prepared atom IDs, so no supported drawing object remains.
    NoObjects,
}

/// One fault per assembly stage, in stage order, with the exact error each
/// raises alone on CONTRACTED.
const STAGES: [(Fault, &str); 11] = [
    // Captions and arrows: a caption with one coordinate.
    (
        Fault::Inject("<t p='0'><s>c</s></t>"),
        "Invalid CDXML coordinates",
    ),
    // Styled atom labels.
    (
        Fault::Inject("<n p='0 0'><t><s size='10'>N</s><s size='12'>O</s></t></n>"),
        MIXED_FONTS,
    ),
    // Highlights.
    (
        Fault::Inject("<n p='0 0' highlightColor='999'/>"),
        "Invalid CDXML highlight color",
    ),
    // Marks.
    (
        Fault::Inject("<graphic><represent attribute='Charge' object='999'/></graphic>"),
        "Invalid molecular CDXML: Chemical symbol refers to a missing atom",
    ),
    // Labels: an atom with no prepared atom at its position.
    (
        Fault::Inject("<n p='999 999'/>"),
        "Invalid molecular CDXML: Could not safely associate atom labels with their atom",
    ),
    // Circle removal parses every closed curve when the molecule has a conformer.
    (
        Fault::Inject("<curve Closed='yes' CurvePoints='x'/>"),
        "could not convert string to float: 'x'",
    ),
    // Ring fills.
    (
        Fault::Inject("<ColoredMolecularArea/>"),
        "Invalid native ring fill ownership",
    ),
    // Graphics.
    (
        Fault::Inject("<graphic/>"),
        "Graphic is missing its two defining points",
    ),
    // Groups and abbreviations: abbreviation association uses the last `n`
    // with the anchor's id. This copy has the anchor's p and Element, so every
    // earlier stage associates it, but its label alignment is unknown.
    (
        Fault::Inject(
            "<n id='6' p='275.61 359.89' Element='8'><t LabelJustification='Bogus'><s>O</s></t></n>",
        ),
        "Invalid molecular CDXML: Unsupported abbreviation label alignment",
    ),
    // Abbreviation presentations, reached from the source.
    (
        Fault::Source(ANCHOR_LABEL, ANCHOR_LABEL_MIXED_HYDROGEN),
        MIXED_HYDROGEN_COLORS,
    ),
    // The empty-drawing check.
    (Fault::NoObjects, NOTHING),
];

/// Assemble CONTRACTED with the faults of `stages`. Later stages' elements are
/// injected first, so they precede earlier stages' elements in document order.
fn staged(stages: &[usize]) -> String {
    let mut text = CONTRACTED.to_owned();
    for &stage in stages {
        if let Fault::Source(from, to) = STAGES[stage].0 {
            text = edit(&text, from, to);
        }
    }
    let mut prepared = prepare_cdxml(&text).unwrap();
    for &stage in stages.iter().rev() {
        match STAGES[stage].0 {
            Fault::Inject(element) => inject(&mut prepared, element),
            Fault::Source(..) => {}
            Fault::NoObjects => prepared.molecule.ids.clear(),
        }
    }
    outcome(&prepared)
}

#[test]
fn each_stage_fault_wins_alone_and_over_every_later_stage() {
    assert_eq!(staged(&[]), "ok");
    let mut failures = Vec::new();
    for (earlier, (_, expected)) in STAGES.iter().enumerate() {
        let cases = std::iter::once(vec![earlier])
            .chain((earlier + 1..STAGES.len()).map(|later| vec![earlier, later]));
        for stages in cases {
            let actual = staged(&stages);
            if actual != *expected {
                failures.push(format!("{stages:?}: {actual:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn faults_on_an_empty_drawing_precede_the_empty_check() {
    let empty = prepare_cdxml(EMPTY).unwrap();
    assert_eq!(outcome(&empty), NOTHING);
    let mut failures = Vec::new();
    for (stage, (fault, expected)) in STAGES.iter().enumerate() {
        let Fault::Inject(element) = fault else {
            continue;
        };
        // Without a conformer, circle removal is skipped and the graphics
        // reader rejects the curve. The anchor copy has no prepared atom.
        let expected = match stage {
            5 => "Invalid CDXML Bézier control-point sequence",
            8 => STAGES[4].1,
            _ => expected,
        };
        let mut prepared = empty.clone();
        inject(&mut prepared, element);
        let actual = outcome(&prepared);
        if actual != expected {
            failures.push(format!("{stage}: {actual:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn styled_label_diagnostics() {
    let prepared = prepare_cdxml(CONTRACTED).unwrap();
    for (label, expected) in [
        // H and 2 in two different colors.
        (
            "<s>O</s><s color='4'>H</s><s color='5'>2</s>",
            MIXED_HYDROGEN_COLORS,
        ),
        // A colored H while its count keeps the label color.
        ("<s>O</s><s color='4'>H</s><s>2</s>", MIXED_HYDROGEN_COLORS),
        // A colored run outside the attached hydrogen.
        ("<s>O</s><s color='4'>Me</s>", MIXED_FONTS),
        // A valid colored hydrogen, but no prepared atom at the position.
        (
            "<s>O</s><s color='4'>H2</s>",
            "Invalid molecular CDXML: Could not safely associate CDXML text style with its atom",
        ),
    ] {
        let mut faulty = prepared.clone();
        inject(&mut faulty, &format!("<n p='0 0'><t>{label}</t></n>"));
        assert_eq!(outcome(&faulty), expected, "{label}");
    }
}

/// `(source, [id])` for consecutive IDs from `first`, one per source.
fn singles(sources: impl IntoIterator<Item = usize>, first: u64) -> Vec<(usize, Vec<u64>)> {
    sources
        .into_iter()
        .zip(first..)
        .map(|(source, id)| (source, vec![id]))
        .collect()
}

/// A fragment's bound IDs followed by the same IDs from its atom children.
fn twice(ids: RangeInclusive<u64>) -> Vec<u64> {
    ids.clone().chain(ids).collect()
}

/// The IDs a scene assigns and its object map, in output order.
#[derive(Debug, PartialEq)]
struct Ids {
    annotations: Vec<u64>,
    arrows: Vec<u64>,
    graphics: Vec<u64>,
    groups: Vec<(u64, Vec<u64>)>,
    objects: Vec<(usize, Vec<u64>)>,
    removed_sources: Vec<usize>,
}
fn ids(scene: &CdxmlScene) -> Ids {
    Ids {
        annotations: scene.base.annotations.iter().map(|a| a.id).collect(),
        arrows: scene.base.arrows.iter().map(|a| a.id).collect(),
        graphics: scene.base.graphics.iter().map(|g| g.id).collect(),
        groups: scene
            .base
            .groups
            .iter()
            .map(|g| (g.id, g.members.clone()))
            .collect(),
        objects: scene
            .objects
            .iter()
            .map(|o| (o.source, o.atoms.clone()))
            .collect(),
        removed_sources: scene.removed_sources.clone(),
    }
}

#[test]
fn caption_and_arrow_ids_interleave_in_document_order() {
    let caption = |y| format!("<t p='0 {y}'><s>c</s></t>");
    let page = |parts: [&str; 4]| format!("<CDXML><page>{}</page></CDXML>", parts.concat());
    let (first, second) = (caption(0), caption(20));
    // Ordinals: CDXML 0, page 1, then each caption adds its `s`.
    assert_eq!(
        ids(&scene(&page([&first, ARROW_38, &second, ARROW_39]))),
        Ids {
            annotations: vec![1, 3],
            arrows: vec![2, 4],
            graphics: vec![],
            groups: vec![],
            objects: singles([2, 4, 5, 7], 1),
            removed_sources: vec![],
        }
    );
    assert_eq!(
        ids(&scene(&page([ARROW_38, &first, ARROW_39, &second]))),
        Ids {
            annotations: vec![2, 4],
            arrows: vec![1, 3],
            graphics: vec![],
            groups: vec![],
            objects: singles([2, 3, 5, 6], 1),
            removed_sources: vec![],
        }
    );
}

#[test]
fn graphic_and_group_ids_follow_caption_ids() {
    // A grouped graphic is read before the later page-level one, and the group
    // ID follows both graphic IDs.
    let text = "<CDXML><page><t p='0 0'><s>a</s></t><group><t p='0 20'><s>b</s></t><graphic GraphicType='Line' BoundingBox='0 0 10 0'/></group><graphic GraphicType='Line' BoundingBox='0 10 10 10'/></page></CDXML>";
    assert_eq!(
        ids(&scene(text)),
        Ids {
            annotations: vec![1, 2],
            arrows: vec![],
            graphics: vec![3, 4],
            groups: vec![(5, vec![2, 3])],
            objects: singles([2, 5, 7, 8], 1),
            removed_sources: vec![],
        }
    );
}

#[test]
fn fixture_ids_bindings_and_removed_sources() {
    assert_eq!(
        ids(&scene(GROUPED)),
        Ids {
            annotations: vec![27, 28],
            arrows: vec![],
            graphics: vec![],
            groups: vec![
                (29, (14..=26).chain([28]).collect()),
                (30, (1..=13).chain([27]).collect()),
            ],
            objects: [
                vec![
                    (8, twice(1..=13)),
                    (46, twice(14..=26)),
                    (43, vec![27]),
                    (81, vec![28])
                ],
                singles([9..=11, 14..=14, 17..=24, 27..=27].into_iter().flatten(), 1),
                singles(
                    [47..=49, 52..=52, 55..=62, 65..=65].into_iter().flatten(),
                    14
                ),
            ]
            .concat(),
            removed_sources: vec![],
        }
    );
    assert_eq!(
        ids(&scene(REACTION)),
        Ids {
            annotations: vec![],
            arrows: vec![10, 11],
            graphics: vec![],
            groups: vec![],
            objects: [
                vec![(9, twice(1..=9)), (35, vec![10]), (36, vec![11])],
                singles([10..=12, 15..=17, 21..=23].into_iter().flatten(), 1),
            ]
            .concat(),
            removed_sources: vec![],
        }
    );
    // The fragment binding also takes the graphic it contains (source 53).
    let fragment = twice(1..=13).into_iter().chain([17]).collect();
    assert_eq!(
        ids(&scene(GRAPHICS)),
        Ids {
            annotations: vec![],
            arrows: vec![],
            graphics: vec![14, 15, 16, 17, 18],
            groups: vec![],
            objects: [
                vec![(18, fragment)],
                singles(
                    [19..=21, 24..=24, 27..=34, 37..=37].into_iter().flatten(),
                    1
                ),
                singles([10, 13, 17, 53, 54], 14),
            ]
            .concat(),
            removed_sources: vec![],
        }
    );
    assert_eq!(
        ids(&scene(SYMBOLS)),
        Ids {
            annotations: vec![],
            arrows: vec![],
            graphics: vec![1, 2],
            groups: vec![],
            objects: singles([13, 14], 1),
            removed_sources: vec![],
        }
    );
    let atoms = [
        10..=16,
        19..=41,
        44..=44,
        47..=62,
        112..=112,
        115..=130,
        149..=149,
        152..=167,
        186..=186,
        189..=189,
        192..=207,
        227..=227,
        230..=230,
        233..=248,
    ];
    assert_eq!(
        ids(&scene(GALLERY)),
        Ids {
            annotations: (118..=129).collect(),
            arrows: vec![],
            graphics: vec![],
            groups: vec![],
            objects: [
                vec![(9, twice(1..=117))],
                singles((268..=290).step_by(2), 118),
                singles(atoms.into_iter().flatten(), 1),
            ]
            .concat(),
            removed_sources: vec![],
        }
    );
    assert_eq!(
        ids(&scene(AROMATIC_CIRCLE)),
        Ids {
            annotations: vec![],
            arrows: vec![],
            graphics: vec![],
            groups: vec![],
            objects: [vec![(7, twice(1..=6))], singles(8..=13, 1)].concat(),
            removed_sources: vec![20],
        }
    );
}

#[test]
fn objects_after_a_removed_circle_keep_their_source_ordinals() {
    // The circle (ordinal 20) is the last element, so a graphic inserted after
    // it in the fragment (21) and the elements injected at the page end
    // (22-29) all shift down by one in the filtered XML. The arrow precedes a
    // graphic, so an untranslated claim would hide that graphic.
    let mut prepared = prepare_cdxml(AROMATIC_CIRCLE).unwrap();
    prepared.expanded_xml = edit(
        &prepared.expanded_xml,
        "</graphic></fragment>",
        "</graphic><graphic GraphicType='Line' BoundingBox='40 60 50 60'></graphic></fragment>",
    );
    inject(
        &mut prepared,
        &format!(
            "{ARROW_38}<graphic GraphicType='Line' BoundingBox='0 30 10 30'/><t p='0 0'><s>a</s></t><group><graphic GraphicType='Line' BoundingBox='0 10 10 10'/><t p='0 20'><s>b</s></t></group>"
        ),
    );
    assert_eq!(
        ids(&assemble_cdxml(&prepared).unwrap()),
        Ids {
            annotations: vec![8, 9],
            arrows: vec![7],
            graphics: vec![10, 11, 12],
            groups: vec![(13, vec![9, 12])],
            objects: [
                vec![(7, twice(1..=6).into_iter().chain([10]).collect())],
                singles([22, 24, 28], 7),
                singles(8..=13, 1),
                singles([21, 23, 27], 10),
            ]
            .concat(),
            removed_sources: vec![20],
        }
    );
}

/// Family, bold and exact color of a native text style.
type Style<'a> = (&'a str, bool, [u8; 3]);
fn style(style: &NativeTextStyle) -> Style<'_> {
    (
        &style.family,
        style.bold,
        style.color.into_document().unwrap(),
    )
}
/// id, highlight, text_style, hydrogen_color, color_override, marks.len()
/// and display.is_some() of one base atom patch.
type Patch<'a> = (
    u64,
    Option<[u8; 3]>,
    Option<Style<'a>>,
    Option<[u8; 3]>,
    bool,
    usize,
    bool,
);
fn patches(scene: &CdxmlScene) -> Vec<Patch<'_>> {
    scene
        .base
        .atoms
        .iter()
        .map(|atom| {
            (
                atom.id,
                atom.highlight,
                atom.text_style.as_ref().map(style),
                atom.hydrogen_color,
                atom.color_override,
                atom.marks.len(),
                atom.display.is_some(),
            )
        })
        .collect()
}
/// The show flag of each base bond's indicator.
fn indicators(scene: &CdxmlScene) -> Vec<Option<bool>> {
    scene
        .base
        .bonds
        .iter()
        .map(|bond| bond.indicator.as_ref().map(|i| i.show))
        .collect()
}

#[test]
fn atom_patches_and_bond_indicators() {
    // Every atom has a display patch only, and every bond an indicator.
    let labels = scene(ATOM_LABELS);
    let displays: Vec<Patch<'_>> = (1..=12)
        .map(|id| (id, None, None, None, false, 0, true))
        .collect();
    assert_eq!(patches(&labels), displays);
    assert_eq!(indicators(&labels), vec![Some(true); 12]);
    let symbols = scene(ATTACHED_SYMBOLS);
    assert_eq!(
        patches(&symbols),
        vec![(1, None, None, None, false, 1, true)]
    );
    assert_eq!(indicators(&symbols), vec![]);
    assert_eq!(
        ids(&symbols).objects,
        vec![(7, vec![1, 1, 1]), (11, vec![1]), (8, vec![1])]
    );
    // The styled anchor patch comes first, then highlight-only and
    // display-only atoms in the order they are first patched.
    let ink = scene(LABEL_INK);
    assert_eq!(
        patches(&ink),
        vec![
            (
                2,
                Some([255, 255, 255]),
                Some(("Helvetica", false, [0, 0, 0])),
                None,
                false,
                0,
                true
            ),
            (3, Some([129, 229, 255]), None, None, false, 0, true),
            (1, None, None, None, false, 0, true),
        ]
    );
    assert_eq!(indicators(&ink), vec![Some(false); 2]);
    assert_eq!(
        ids(&ink).objects,
        [vec![(10, twice(1..=3))], singles([11, 13, 16], 1)].concat()
    );
}

/// anchor, label, label_style (family, bold, color) and label_color_override.
type Contracted<'a> = (u64, &'a str, Option<(&'a str, bool, Color)>, bool);
fn abbreviations(scene: &CdxmlScene) -> Vec<Contracted<'_>> {
    scene
        .base
        .abbreviations
        .iter()
        .map(|a| {
            (
                a.anchor,
                a.label.as_str(),
                a.label_style
                    .as_ref()
                    .map(|s| (s.family.as_str(), s.bold, s.color)),
                a.label_color_override,
            )
        })
        .collect()
}

#[test]
fn abbreviation_presentations_restore_the_anchor_style() {
    // The anchor's own color is explicit, so its patch keeps the override,
    // and the label keeps its own font.
    let contracted = scene(CONTRACTED);
    assert_eq!(
        abbreviations(&contracted),
        vec![(2, "OMe", Some(("Geneva", false, Color::Ink)), false)]
    );
    assert_eq!(
        patches(&contracted),
        vec![
            (
                2,
                Some([223, 71, 62]),
                Some(("Helvetica", false, [0, 0, 0])),
                None,
                true,
                0,
                true
            ),
            (3, Some([129, 229, 255]), None, None, false, 0, true),
            (1, None, None, None, false, 0, true),
        ]
    );
    assert_eq!(indicators(&contracted), vec![Some(false); 2]);
    assert_eq!(
        ids(&contracted).objects,
        [vec![(17, twice(1..=3))], singles([18, 20, 23], 1)].concat()
    );
    // Only the label color is explicit (LABEL_INK's patches are pinned above).
    assert_eq!(
        abbreviations(&scene(LABEL_INK)),
        vec![(
            2,
            "OMe",
            Some(("Helvetica", false, Color::Custom([124, 124, 124]))),
            true
        )]
    );
    // Equal automatic presentations keep no label style.
    assert_eq!(
        abbreviations(&scene(GALLERY)),
        vec![
            (48, "TBDPS", None, false),
            (65, "TBDPS", None, false),
            (82, "OTBDPS", None, false),
            (100, "OTBDPS", None, false),
        ]
    );
}

#[test]
fn colored_attached_hydrogen_reaches_the_atom_patch() {
    let hydrogen_colors = |assembled: CdxmlScene| -> Vec<(u64, Option<Color>)> {
        let document = assembled.into_document().unwrap().document;
        document
            .atoms
            .iter()
            .map(|atom| (atom.id, atom.display.hydrogen_color))
            .collect()
    };
    // A styled label: a copy of the methyl carbon (atom 3) with a red H3.
    let mut prepared = prepare_cdxml(CONTRACTED).unwrap();
    inject(
        &mut prepared,
        "<n p='303.61 359.89'><t><s>C</s><s color='4'>H3</s></t></n>",
    );
    let styled = assemble_cdxml(&prepared).unwrap();
    assert_eq!(
        patches(&styled),
        vec![
            (
                2,
                Some([223, 71, 62]),
                Some(("Helvetica", false, [0, 0, 0])),
                None,
                true,
                0,
                true
            ),
            (
                3,
                Some([129, 229, 255]),
                Some(("Helvetica", false, [0, 0, 0])),
                Some([255, 0, 0]),
                false,
                0,
                true
            ),
            (1, None, None, None, false, 0, true),
        ]
    );
    assert_eq!(
        hydrogen_colors(styled),
        vec![(1, None), (2, None), (3, Some(Color::Custom([255, 0, 0])))]
    );
    // A restored abbreviation anchor with a red H2.
    let restored = scene(&edit(CONTRACTED, ANCHOR_LABEL, ANCHOR_LABEL_RED_HYDROGEN));
    assert_eq!(
        patches(&restored),
        vec![
            (
                2,
                Some([223, 71, 62]),
                Some(("Helvetica", false, [0, 0, 0])),
                Some([255, 0, 0]),
                true,
                0,
                true
            ),
            (3, Some([129, 229, 255]), None, None, false, 0, true),
            (1, None, None, None, false, 0, true),
        ]
    );
    assert_eq!(
        hydrogen_colors(restored),
        vec![(1, None), (2, Some(Color::Custom([255, 0, 0]))), (3, None)]
    );
}

#[test]
fn duplicate_atom_patches_send_updates_to_the_first_and_keep_the_last() {
    // Two styled copies of the first labelled atom (N, id 3) at its p and
    // Element: white, then bold with a highlight. A lone pair names id 3,
    // which resolves to the last copy. All three resolve to atom 1.
    let mut prepared = prepare_cdxml(ATOM_LABELS).unwrap();
    inject(
        &mut prepared,
        "<fragment><n id='3' p='58.77 30' Element='7'><t><s font='60' size='10' color='1' face='96'>N</s></t></n><n id='3' p='58.77 30' Element='7' highlightColor='1'><t><s font='60' size='10' color='0' face='97'>N</s></t></n></fragment><graphic BoundingBox='58.77 25 58.77 28' GraphicType='Symbol' SymbolType='LonePair'><represent attribute='Radical' object='3'/></graphic>",
    );
    let scene = assemble_cdxml(&prepared).unwrap();
    let mut expected: Vec<Patch<'_>> = vec![
        (
            1,
            Some([255, 255, 255]),
            Some(("Arial", false, [255, 255, 255])),
            None,
            false,
            1,
            true,
        ),
        (
            1,
            None,
            Some(("Arial", true, [0, 0, 0])),
            None,
            false,
            0,
            false,
        ),
    ];
    expected.extend((2..=12).map(|id| (id, None, None, None, false, 0, true)));
    assert_eq!(patches(&scene), expected);
    // The mark object precedes both copies' label objects.
    assert_eq!(
        ids(&scene).objects,
        [
            vec![(7, twice(1..=12)), (84, vec![1])],
            singles([8, 14, 21, 25, 29, 33, 37, 41, 45, 49, 53, 59], 1),
            vec![(78, vec![1]), (81, vec![1])],
        ]
        .concat()
    );
    // Reconstruction keeps only the last patch, so the first patch's
    // highlight, mark and display are dropped.
    let document = scene.into_document().unwrap().document;
    let atom = &document.atoms[0];
    let text_style = atom.text_style.as_ref().unwrap();
    assert_eq!(
        (
            atom.id,
            text_style.family.as_str(),
            text_style.bold,
            text_style.color
        ),
        (1, "Arial", true, Color::Ink)
    );
    assert_eq!(atom.display.highlight, None);
    assert_eq!(atom.display.number, None);
    assert!(atom.marks.is_empty());
}

#[test]
fn abbreviation_restoration_patches_the_last_duplicate() {
    // A red Helvetica copy of the abbreviation anchor (atom 2), without the
    // anchor's id. Highlight and display updates go to the first patch, but
    // the restored anchor style (black, explicit) replaces the copy's.
    let mut prepared = prepare_cdxml(CONTRACTED).unwrap();
    inject(
        &mut prepared,
        "<n p='275.61 359.89' Element='8'><t><s font='21' size='10' color='4' face='96'>O</s></t></n>",
    );
    let scene = assemble_cdxml(&prepared).unwrap();
    assert_eq!(
        patches(&scene),
        vec![
            (
                2,
                Some([223, 71, 62]),
                Some(("Geneva", false, [0, 0, 0])),
                None,
                false,
                0,
                true
            ),
            (
                2,
                None,
                Some(("Helvetica", false, [0, 0, 0])),
                None,
                true,
                0,
                false
            ),
            (3, Some([129, 229, 255]), None, None, false, 0, true),
            (1, None, None, None, false, 0, true),
        ]
    );
    assert_eq!(
        abbreviations(&scene),
        vec![(2, "OMe", Some(("Geneva", false, Color::Ink)), false)]
    );
    assert_eq!(
        ids(&scene).objects,
        [
            vec![(17, twice(1..=3))],
            singles([18, 20, 23], 1),
            vec![(25, vec![2])],
        ]
        .concat()
    );
    // Reconstruction keeps the restored last patch, without the highlight.
    let document = scene.into_document().unwrap().document;
    let atom = &document.atoms[1];
    let text_style = atom.text_style.as_ref().unwrap();
    assert_eq!(
        (
            atom.id,
            text_style.family.as_str(),
            text_style.color,
            atom.display.highlight
        ),
        (2, "Helvetica", Color::Ink, None)
    );
}

/// A change to a valid prepared snapshot through its public fields.
type Mutation<'a> = &'a dyn Fn(&mut PreparedCdxml);

#[test]
fn prepared_snapshot_checks_and_precedence() {
    let base = prepare_cdxml(CONTRACTED).unwrap();
    let mutated = |change: Mutation<'_>| {
        let mut prepared = base.clone();
        change(&mut prepared);
        outcome(&prepared)
    };
    let many_ids = |p: &mut PreparedCdxml| p.molecule.ids = (1..=100_001).collect();
    let unclosed = |p: &mut PreparedCdxml| p.expanded_xml = "<CDXML><page>".into();
    let nan_color = |p: &mut PreparedCdxml| {
        let at = p.expanded_xml.rfind("</CDXML>").unwrap();
        p.expanded_xml
            .insert_str(at, "<colortable><color r='nan'/></colortable>");
    };
    let changed_id = |p: &mut PreparedCdxml| p.molecule.ids[0] = 99;
    let far_binding = |p: &mut PreparedCdxml| p.fragment_bindings[0].source = usize::MAX;
    let caption = |p: &mut PreparedCdxml| inject(p, "<t p='x 0'><s>c</s></t>");
    let label = |text: &'static str| {
        move |p: &mut PreparedCdxml| {
            p.abbreviations[0].presentation.as_mut().unwrap().label.text = Some(text.into());
        }
    };
    let cases: [(Mutation<'_>, &str); 13] = [
        (&many_ids, LIMIT),
        (
            &unclosed,
            "Invalid CDXML presentation XML: the root node was opened but never closed",
        ),
        // (a) Limits before XML.
        (
            &|p| {
                many_ids(p);
                unclosed(p);
            },
            LIMIT,
        ),
        (&nan_color, "cannot convert float NaN to integer"),
        (&changed_id, IDS_CHANGED),
        // (b) The text reader's palette before the ID check.
        (
            &|p| {
                nan_color(p);
                changed_id(p);
            },
            "cannot convert float NaN to integer",
        ),
        (&far_binding, LIMIT),
        // (c) The ID check before binding validation.
        (
            &|p| {
                changed_id(p);
                far_binding(p);
            },
            IDS_CHANGED,
        ),
        (&caption, "could not convert string to float: 'x'"),
        // (d) Binding validation before captions.
        (
            &|p| {
                far_binding(p);
                caption(p);
            },
            LIMIT,
        ),
        // (e) Saved abbreviation presentations: a root that is not text, and
        // malformed XML.
        (&label("<s/>"), "Invalid abbreviation text presentation"),
        (
            &label("<t"),
            "Invalid CDXML presentation XML: the document does not have a root node",
        ),
        (&label("<t><s>OMe</s></t>"), "ok"),
    ];
    let failures: Vec<_> = cases
        .iter()
        .enumerate()
        .filter_map(|(i, (change, expected))| {
            let actual = mutated(*change);
            (actual != *expected).then(|| format!("{i}: {actual:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
