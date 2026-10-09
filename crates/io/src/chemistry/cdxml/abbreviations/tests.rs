//! Pins flatten_one's exact diagnostics and which one wins when a wrapper
//! holds several faults, the exact output of accepted expansions, and the
//! Tree work each expansion spends. Every expectation was observed on the
//! expansion before its stage split.
//!
//! Accepted documents use only exact arithmetic so the pins hold on every CI
//! platform: each connection vector points the same axis-aligned direction as
//! its neighbor vector (rotation exactly 0, so cos 1 and sin 0), lengths are
//! axis-aligned, and scales are powers of two.
use super::*;

/// One external bond: the connection point 7 lies at -15 0 and the neighbor
/// 2 at -30 0, so the scale is exactly 2. Member 6 exercises both branches of
/// `significant`.
const SINGLE: &str = r"<CDXML><page><fragment id='1'><n id='2' p='-30 0'/><n id='3' p='0 0' NodeType='Fragment'><fragment id='4'><n id='5' p='0 0' Element='8'/><n id='6' p='0.00001 123456789'/><n id='7' p='-15 0' NodeType='ExternalConnectionPoint'/><b id='8' B='7' E='5'/><b id='9' B='5' E='6'/></fragment><t><s>OMe</s></t></n><b id='10' B='2' E='3'/></fragment></page></CDXML>";

/// Two external bonds. Connection 11 pairs with bond 5 to atom 2 (rotation
/// pi minus pi, scale 2); connection 12 pairs with bond 6 to atom 3
/// (rotation 0, scale 1). Placement uses the first pair after reordering.
const ASYMMETRIC_TWO_CONNECTIONS: &str = r"<CDXML><page><fragment id='1'><n id='2' p='-20 0'/><n id='3' p='10 0'/><n id='4' p='0 0' NodeType='Fragment' BondOrdering='5 6'><fragment id='10' ConnectionOrder='11 12'><n id='11' p='-10 0' NodeType='ExternalConnectionPoint'/><n id='12' p='10 0' NodeType='ExternalConnectionPoint'/><n id='13' p='0 0'/><n id='14' p='0 -10' Element='17'/><b id='16' B='11' E='13'/><b id='17' B='12' E='13'/><b id='18' B='13' E='14'/></fragment><t><s>CCl</s></t></n><b id='5' B='2' E='4'/><b id='6' B='4' E='3'/></fragment></page></CDXML>";

/// The wrapper's highlight against members and bonds with and without their
/// own; scale 2.
const HIGHLIGHTED: &str = r"<CDXML><page><fragment id='1'><n id='2' p='-20 0'/><n id='3' p='0 0' NodeType='Fragment' highlightColor='3'><fragment id='4'><n id='5' p='0 0'/><n id='6' p='10 0' highlightColor='5'/><n id='7' p='0 10'/><n id='8' p='-10 0' NodeType='ExternalConnectionPoint'/><b id='9' B='8' E='5'/><b id='10' B='5' E='6' highlightColor='5'/><b id='11' B='5' E='7'/></fragment><t><s>Et</s></t></n><b id='12' B='2' E='3'/></fragment></page></CDXML>";

/// No connection point, so the anchor is the first node. The anchor carries
/// its own text and label attributes; the label is the reverse spelling.
const LABELED: &str = r"<CDXML><page><fragment id='1'><n id='2' p='5 5' NodeType='Nickname' LabelFont='3' LabelSize='10' Element='6'><fragment id='3'><n id='4' p='1 1' Element='8' LabelFace='96'><t p='0 0' LabelJustification='Left'><s>O</s></t></n><n id='5' p='3 1'/><b id='6' B='4' E='5'/></fragment><t p='4 5' LabelJustification='Left'><s font='3'>MeO</s></t></n></fragment></page></CDXML>";

/// `base` with each `(from, to)` applied in turn; `from` must occur once.
fn edit(base: &str, edits: &[(&str, &str)]) -> String {
    let mut text = base.to_owned();
    for &(from, to) in edits {
        assert_eq!(text.matches(from).count(), 1, "{from}");
        text = text.replacen(from, to, 1);
    }
    text
}

/// The exact Display of the error, or `ok`.
fn outcome(text: &str) -> String {
    flatten_abbreviations(text).map_or_else(|error| error.to_string(), |_| "ok".to_owned())
}

/// Every case's outcome, reporting all mismatches at once.
fn check(base: &str, cases: &[(&[(&str, &str)], &str)]) {
    let failures: Vec<_> = cases
        .iter()
        .filter_map(|&(edits, expected)| {
            let actual = outcome(&edit(base, edits));
            (actual != expected).then(|| format!("{edits:?}: {actual:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const OUTSIDE: &str = "Invalid molecular CDXML: Abbreviation outside a molecular fragment";
const DEFINITION: &str =
    "Invalid molecular CDXML: An abbreviation needs its explicit chemical definition";
const NESTED: &str = "Invalid molecular CDXML: Nested abbreviations must be expanded before import";
const UNSUPPORTED: &str =
    "Invalid molecular CDXML: Unsupported objects within an abbreviation definition";
const DUPLICATE_IDS: &str = "CDXML contains unsupported chemistry: duplicate abbreviation atom IDs";
const MULTIPLE: &str =
    "Invalid molecular CDXML: Multiple abbreviation attachments are not supported yet";
const MULTIPLE_BONDS: &str =
    "Invalid molecular CDXML: Multiple abbreviation bonds are not supported yet";
const POSITION: &str = "Invalid molecular CDXML: Invalid abbreviation position";
const ORDERING: &str = "Invalid molecular CDXML: Invalid internal abbreviation attachment ordering";
const MISSING: &str = "Invalid molecular CDXML: Missing internal abbreviation attachment";
const DUPLICATE: &str = "Invalid molecular CDXML: Duplicate internal abbreviation attachment";
const CONNECTION: &str = "Invalid molecular CDXML: Invalid abbreviation connection point";
const MISSING_ANCHOR: &str = "Invalid molecular CDXML: Missing internal abbreviation anchor";
const SHARE: &str =
    "Invalid molecular CDXML: Internal abbreviation attachments must share one atom";
const ORDER: &str =
    "Invalid molecular CDXML: Abbreviation attachment bond order conflicts with its definition";
const ATTACHMENT_POINT: &str = "Invalid molecular CDXML: Missing abbreviation attachment point";
const EMPTY: &str = "Invalid molecular CDXML: Empty abbreviation definition";
const ATOM_POSITION: &str = "Invalid molecular CDXML: Missing abbreviation atom position";
const GEOMETRY: &str = "Invalid molecular CDXML: Invalid abbreviation attachment geometry";
const NEIGHBOR: &str = "Invalid molecular CDXML: Missing abbreviation neighbor";
const MEMBER_POSITION: &str = "Invalid molecular CDXML: Invalid abbreviation atom position";
const ATTACHMENT_ID: &str = "Invalid molecular CDXML: Missing abbreviation attachment atom ID";
const DISCARDED: &str = "CDXML contains unsupported chemistry: discarded abbreviation chemistry";
const LIMIT: &str = "CDXML exceeds the size, nesting, or molecular work limit";

/// Wrap the abbreviation in a `group` instead of a molecular fragment.
const GROUP_PARENT: [(&str, &str); 2] = [
    ("<fragment id='1'>", "<group id='1'>"),
    ("</fragment></page>", "</group></page>"),
];

/// Both orders reversed, so the first pair is connection 12 with bond 6.
const REVERSED: [(&str, &str); 2] = [("'5 6'", "'6 5'"), ("'11 12'", "'12 11'")];

/// ConnectionOrder removed, so connections fall back to node order.
const UNORDERED: [(&str, &str); 1] = [(" ConnectionOrder='11 12'", "")];

#[test]
fn single_attachment_diagnostics_and_precedence() {
    let inner = "<fragment id='4'>";
    let nickname = "<fragment id='4'><n id='20' p='0 0' NodeType='Nickname'/>";
    let graphic = "<fragment id='4'><graphic id='21'/>";
    let graphic_first =
        "<fragment id='4'><graphic id='21'/><n id='20' p='0 0' NodeType='Nickname'/>";
    let bond = "<b id='8' B='7' E='5'/>";
    let outer = "<n id='3' p='0 0'";
    let member = "p='0.00001 123456789'";
    check(
        SINGLE,
        &[
            (&[], "ok"),
            // A wrapper that is a direct child of an earlier wrapper, beside its
            // definition, is detached by that expansion and fails the ancestor walk.
            (
                &[("</t></n>", "</t><n id='20' NodeType='Nickname'/></n>")],
                OUTSIDE,
            ),
            // A plain atom there is discarded with the wrapper.
            (&[("</t></n>", "</t><n id='20' p='0 0'/></n>")], DISCARDED),
            (&[("<t><s>OMe</s></t>", "")], DEFINITION),
            (&[("<t>", "<fragment id='20'/><t>")], DEFINITION),
            (
                &[(inner, "<group id='4'>"), ("</fragment><t>", "</group><t>")],
                DEFINITION,
            ),
            (&[(inner, nickname)], NESTED),
            (&[(inner, graphic)], UNSUPPORTED),
            // The nested-wrapper loop runs before the unsupported-object loop.
            (&[(inner, graphic_first)], NESTED),
            (
                &[(
                    inner,
                    "<fragment id='4'><fragment id='20'><n id='21' p='0 0'/></fragment>",
                )],
                DISCARDED,
            ),
            (&[("<n id='6'", "<n id='5'")], DUPLICATE_IDS),
            // Two absent IDs are duplicates too.
            (
                &[("<n id='5' ", "<n "), ("<n id='6' ", "<n ")],
                DUPLICATE_IDS,
            ),
            (&GROUP_PARENT, OUTSIDE),
            // Duplicate IDs win over the non-fragment parent.
            (
                &[GROUP_PARENT[0], GROUP_PARENT[1], ("<n id='6'", "<n id='5'")],
                DUPLICATE_IDS,
            ),
            (
                &[(
                    "<b id='10' B='2' E='3'/>",
                    "<b id='10' B='2' E='3'/><b id='11' B='3' E='2'/>",
                )],
                MULTIPLE_BONDS,
            ),
            (&[(outer, "<n id='3'")], POSITION),
            (&[(outer, "<n id='3' p='0 0 0'")], POSITION),
            (&[(outer, "<n id='3' p='nan 0'")], POSITION),
            (&[(outer, "<n id='3' p='0 inf'")], POSITION),
            // The connection point needs exactly one internal bond.
            (&[(bond, "")], CONNECTION),
            (
                &[("<b id='9'", "<b id='11' B='6' E='7'/><b id='9'")],
                CONNECTION,
            ),
            (&[(bond, "<b id='8' B='7' E='5' Order='2'/>")], ORDER),
            // The single path checks the bond order before the missing anchor.
            (&[(bond, "<b id='8' B='7' E='99' Order='2'/>")], ORDER),
            // No connection point, but an external bond.
            (
                &[(" NodeType='ExternalConnectionPoint'", "")],
                ATTACHMENT_POINT,
            ),
            (&[("B='7' E='5'", "B='7' E='99'")], EMPTY),
            // The connection bond leads back to the connection point.
            (&[("B='7' E='5'", "B='7' E='7'")], CONNECTION),
            (&[("<n id='5' p='0 0'", "<n id='5'")], ATOM_POSITION),
            (&[("p='-15 0'", "p='-15'")], GEOMETRY),
            (&[("B='2' E='3'", "B='99' E='3'")], NEIGHBOR),
            (&[("<n id='2' p='-30 0'/>", "<n id='2'/>")], GEOMETRY),
            // Members fail on a missing and on a non-finite position.
            (&[(member, "")], MEMBER_POSITION),
            (&[(member, "p='nan 0'")], MEMBER_POSITION),
            (&[(member, "p='0 -inf'")], MEMBER_POSITION),
            // With no anchor ID, the bond end without E resolves to it.
            (&[("<n id='5' ", "<n "), (" E='5'", "")], ATTACHMENT_ID),
        ],
    );
    // No nodes, no connection point and no external bond.
    let empty = "<CDXML><page><fragment id='1'><n id='3' p='0 0' NodeType='Fragment'><fragment id='4'/><t><s>Me</s></t></n></fragment></page></CDXML>";
    assert_eq!(outcome(empty), EMPTY);
}

#[test]
fn multiple_attachment_diagnostics_and_precedence() {
    let bond_16 = "<b id='16' B='11' E='13'/>";
    let bond_17 = "<b id='17' B='12' E='13'/>";
    let pair_faults: [(&str, &str); 2] = [
        ("B='12' E='13'", "B='12' E='14'"),
        (bond_16, "<b id='16' B='11' E='13' Order='2'/>"),
    ];
    check(
        ASYMMETRIC_TWO_CONNECTIONS,
        &[
            (&[], "ok"),
            (&REVERSED, "ok"),
            (&UNORDERED, "ok"),
            (&[(" BondOrdering='5 6'", "")], MULTIPLE),
            // The multiple-connection check wins over the non-fragment parent.
            (
                &[
                    GROUP_PARENT[0],
                    GROUP_PARENT[1],
                    (" BondOrdering='5 6'", ""),
                ],
                MULTIPLE,
            ),
            // Two connection points but one external bond.
            (&[("<b id='6' B='4' E='3'/>", "")], MULTIPLE),
            (&[("'5 6'", "'5'")], ORDERING),
            (&[("'11 12'", "'11'")], ORDERING),
            (&[("'5 6'", "'5 99'")], MISSING),
            (&[("'11 12'", "'11 99'")], MISSING),
            (&[("'5 6'", "'5 5'")], DUPLICATE),
            (&[("'11 12'", "'12 12'")], DUPLICATE),
            // BondOrdering resolves before ConnectionOrder.
            (&[("'5 6'", "'5 99'"), ("'11 12'", "'12 12'")], MISSING),
            (&[("'5 6'", "'6 6'"), ("'11 12'", "'11 99'")], DUPLICATE),
            // Each connection point needs exactly one internal bond.
            (&[(bond_17, "")], CONNECTION),
            (
                &[("<b id='18'", "<b id='19' B='11' E='14'/><b id='18'")],
                CONNECTION,
            ),
            (&[("B='12' E='13'", "B='12' E='99'")], MISSING_ANCHOR),
            (&[("B='12' E='13'", "B='12' E='14'")], SHARE),
            (&[("B='11' E='13'", "B='11' E='12'")], SHARE),
            (&[(bond_17, "<b id='17' B='12' E='13' Order='2'/>")], ORDER),
            // The multi path checks the anchor before the bond order.
            (
                &[(bond_17, "<b id='17' B='12' E='99' Order='2'/>")],
                MISSING_ANCHOR,
            ),
            // Pairs are checked in the reordered sequence.
            (&pair_faults, ORDER),
            (
                &[pair_faults[0], pair_faults[1], REVERSED[0], REVERSED[1]],
                SHARE,
            ),
        ],
    );
}

/// Flatten an accepted document: the exact XML and abbreviations Debug,
/// including each presentation.
fn accepted(text: &str) -> (String, String) {
    let flat = flatten_abbreviations(text).unwrap();
    (flat.xml, format!("{:?}", flat.abbreviations))
}

fn pair(xml: &str, abbreviations: &str) -> (String, String) {
    (xml.to_owned(), abbreviations.to_owned())
}

#[test]
fn multiple_attachments_place_by_the_first_reordered_pair() {
    let abbreviations = r#"[Abbreviation { label: "CCl", reverse_label: "", anchor: Some("13"), members: [Some("13"), Some("14")], highlight: None, presentation: Some(AbbreviationPresentation { label: AtomPresentation { attributes: {}, text: Some("<t><s>CCl</s></t>") }, anchor: AtomPresentation { attributes: {}, text: None } }) }]"#;
    // (i) Connection 11 with bond 5 to atom 2: scale 2 moves atom 14 to 0 -20.
    let written = accepted(ASYMMETRIC_TWO_CONNECTIONS);
    let xml = r#"<CDXML><page><fragment id="1"><n id="2" p="-20 0"></n><n id="3" p="10 0"></n><b id="5" B="2" E="13"></b><b id="6" B="13" E="3"></b><n id="13" p="0 0"><t p="0 0" LabelJustification="Auto"><s>CCl</s></t></n><n id="14" p="0 -20" Element="17"></n><b id="18" B="13" E="14"></b></fragment></page></CDXML>"#;
    assert_eq!(written, pair(xml, abbreviations));
    // The moved label's saved line arrangement is recomputed from geometry.
    assert!(
        written
            .0
            .contains(r#"<t p="0 0" LabelJustification="Auto">"#)
    );
    // (ii) Connection 12 with bond 6 to atom 3: scale 1 leaves atom 14 at 0 -10.
    let xml = r#"<CDXML><page><fragment id="1"><n id="2" p="-20 0"></n><n id="3" p="10 0"></n><b id="5" B="2" E="13"></b><b id="6" B="13" E="3"></b><n id="13" p="0 0"><t p="0 0" LabelJustification="Auto"><s>CCl</s></t></n><n id="14" p="0 -10" Element="17"></n><b id="18" B="13" E="14"></b></fragment></page></CDXML>"#;
    let reversed = edit(ASYMMETRIC_TWO_CONNECTIONS, &REVERSED);
    assert_eq!(accepted(&reversed), pair(xml, abbreviations));
    // (iii) An absent ConnectionOrder falls back to node order, giving (i).
    let unordered = edit(ASYMMETRIC_TWO_CONNECTIONS, &UNORDERED);
    assert_eq!(accepted(&unordered), written);
}

#[test]
fn numbered_attachments_follow_explicit_ids_instead_of_numeric_sorting() {
    let numbered = edit(
        ASYMMETRIC_TWO_CONNECTIONS,
        &[
            (
                "id='11' p='-10 0'",
                "id='11' p='-10 0' ExternalConnectionNum='3'",
            ),
            (
                "id='12' p='10 0'",
                "id='12' p='10 0' ExternalConnectionNum='1'",
            ),
        ],
    );
    // Sparse identifiers are retained by the codec and consumed only after
    // explicit pairing. Sorting them would select the other placement pair.
    assert_eq!(accepted(&numbered), accepted(ASYMMETRIC_TWO_CONNECTIONS));
    let reversed = edit(&numbered, &REVERSED);
    assert_eq!(
        accepted(&reversed),
        accepted(&edit(ASYMMETRIC_TWO_CONNECTIONS, &REVERSED))
    );
    for (edits, message) in [
        (
            vec![("ExternalConnectionNum='3'", "ExternalConnectionNum='1'")],
            "Duplicate external connection numbers",
        ),
        (vec![(" ExternalConnectionNum='3'", "")], "complete numbers"),
        (
            vec![(" ConnectionOrder='11 12'", "")],
            "explicit ConnectionOrder",
        ),
        (
            vec![("ExternalConnectionNum='3'", "ExternalConnectionNum='0'")],
            "positive signed byte",
        ),
        (
            vec![(
                "id='13' p='0 0'",
                "id='13' p='0 0' ExternalConnectionNum='1'",
            )],
            "requires an external connection point",
        ),
        (
            vec![("B='12' E='13'", "B='12' E='14'")],
            "must share one atom",
        ),
    ] {
        let error = flatten_abbreviations(&edit(&numbered, &edits))
            .unwrap_err()
            .to_string();
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn accepted_single_attachments_are_exact() {
    // Scale 2 with both `significant` spellings; OMe keeps its spelling.
    let xml = r#"<CDXML><page><fragment id="1"><n id="2" p="-30 0"></n><b id="10" B="2" E="5"></b><n id="5" p="0 0" Element="8"><t p="0 0"><s>OMe</s></t></n><n id="6" p="2e-05 2.4691358e+08"></n><b id="9" B="5" E="6"></b></fragment></page></CDXML>"#;
    let abbreviations = r#"[Abbreviation { label: "OMe", reverse_label: "MeO", anchor: Some("5"), members: [Some("5"), Some("6")], highlight: None, presentation: Some(AbbreviationPresentation { label: AtomPresentation { attributes: {}, text: Some("<t><s>OMe</s></t>") }, anchor: AtomPresentation { attributes: {}, text: None } }) }]"#;
    assert_eq!(accepted(SINGLE), pair(xml, abbreviations));
    // The wrapper's highlight reaches only atoms and bonds without their own.
    let xml = r#"<CDXML><page><fragment id="1"><n id="2" p="-20 0"></n><b id="12" B="2" E="5"></b><n id="5" p="0 0" highlightColor="3"><t p="0 0"><s>Et</s></t></n><n id="6" p="20 0" highlightColor="5"></n><n id="7" p="0 20" highlightColor="3"></n><b id="10" B="5" E="6" highlightColor="5"></b><b id="11" B="5" E="7" highlightColor="3"></b></fragment></page></CDXML>"#;
    let abbreviations = r#"[Abbreviation { label: "Et", reverse_label: "", anchor: Some("5"), members: [Some("5"), Some("6"), Some("7")], highlight: Some("3"), presentation: Some(AbbreviationPresentation { label: AtomPresentation { attributes: {}, text: Some("<t><s>Et</s></t>") }, anchor: AtomPresentation { attributes: {}, text: None } }) }]"#;
    assert_eq!(accepted(HIGHLIGHTED), pair(xml, abbreviations));
    // With no connection point the first node anchors. Its text is replaced
    // by a clone of the label at the anchor's p, after the presentation
    // captured it, and MeO maps to OMe.
    let xml = r#"<CDXML><page><fragment id="1"><n id="4" p="5 5" Element="8" LabelFace="96"><t p="5 5" LabelJustification="Left"><s font="3">MeO</s></t></n><n id="5" p="7 5"></n><b id="6" B="4" E="5"></b></fragment></page></CDXML>"#;
    let abbreviations = r#"[Abbreviation { label: "OMe", reverse_label: "MeO", anchor: Some("4"), members: [Some("4"), Some("5")], highlight: None, presentation: Some(AbbreviationPresentation { label: AtomPresentation { attributes: {"LabelFont": "3", "LabelSize": "10"}, text: Some("<t p=\"4 5\" LabelJustification=\"Left\"><s font=\"3\">MeO</s></t>") }, anchor: AtomPresentation { attributes: {"LabelFace": "96"}, text: Some("<t p=\"5 5\" LabelJustification=\"Left\"><s>O</s></t>") } }) }]"#;
    assert_eq!(accepted(LABELED), pair(xml, abbreviations));
}

/// Expand each wrapper in document order, as flatten_tree does, and return
/// each call's Tree work with the atoms and bonds it reports removed.
fn expansions(text: &str) -> Vec<(usize, usize, usize)> {
    let mut tree = Tree::parse(text).unwrap();
    let wrappers: Vec<_> = tree
        .descendants(0)
        .unwrap()
        .into_iter()
        .filter(|&index| wrapper(&tree, index).unwrap())
        .collect();
    wrappers
        .into_iter()
        .map(|outer| {
            let before = tree.remaining_work();
            let expanded = flatten_one(&mut tree, outer).unwrap();
            let after = tree.remaining_work();
            (before - after, expanded.atoms, expanded.bonds)
        })
        .collect()
}

#[test]
fn expansion_work_is_pinned() {
    assert_eq!(expansions(SINGLE), [(175, 2, 1)]);
    assert_eq!(expansions(ASYMMETRIC_TWO_CONNECTIONS), [(247, 3, 2)]);
    assert_eq!(
        expansions(&edit(ASYMMETRIC_TWO_CONNECTIONS, &REVERSED)),
        [(248, 3, 2)]
    );
    // The node-order fallback is built whether or not ConnectionOrder is
    // present, so dropping it spends the same work as the written order.
    assert_eq!(
        expansions(&edit(ASYMMETRIC_TWO_CONNECTIONS, &UNORDERED)),
        [(247, 3, 2)]
    );
    assert_eq!(expansions(HIGHLIGHTED), [(233, 2, 1)]);
    assert_eq!(expansions(LABELED), [(140, 1, 0)]);
    let many = expansions(&many_abbreviations(50));
    assert_eq!(many.len(), 50);
    assert!(
        many.iter()
            .all(|&(_, atoms, bonds)| (atoms, bonds) == (1, 0))
    );
    assert_eq!(many.iter().map(|&(work, ..)| work).sum::<usize>(), 11400);
}

#[test]
fn work_limit_boundary_is_pinned() {
    let groups = 2563;
    let flat = flatten_abbreviations(&many_abbreviations(groups)).unwrap();
    assert_eq!(flat.abbreviations.len(), groups);
    assert_eq!(outcome(&many_abbreviations(groups + 1)), LIMIT);
}

/// reference/cdxml_abbreviations.rs's generator: `count` standalone methyl
/// abbreviations side by side in one fragment.
fn many_abbreviations(count: usize) -> String {
    use std::fmt::Write;
    let mut text = String::from("<CDXML><page><fragment id='1'>");
    for i in 0..count {
        let outer = 100 + 3 * i;
        let inner = outer + 1;
        let atom = outer + 2;
        write!(
            text,
            "<n id='{outer}' p='{i} 0' NodeType='Fragment'><fragment id='{inner}'><n id='{atom}' p='0 0'/></fragment><t><s>Me</s></t></n>"
        )
        .unwrap();
    }
    text.push_str("</fragment></page></CDXML>");
    text
}
