//! Persistent structure highlight interchange; foreground paint is independent.
use reshiki::{
    chemistry::cdxml::import_cdxml,
    document::{Document, Point},
    exchange::{drawing, from_cdx, to_cdx},
    palette::Color,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const CYAN: Color = Color::Custom([129, 230, 255]);
const YELLOW: Color = Color::Custom([255, 198, 0]);
const RED: Color = Color::Custom([223, 71, 62]);

fn xml(document: &Document) -> Result<String> {
    Ok(drawing::write(document, Default::default())?)
}

fn both(document: &Document) -> Result<Vec<Document>> {
    let xml = xml(document)?;
    let binary_xml = from_cdx(&to_cdx(&xml)?)?;
    [xml, binary_xml]
        .into_iter()
        .map(|xml| Ok(import_cdxml(&xml)?.document))
        .collect()
}

#[test]
fn actual_prime_native_files_preserve_atom_and_bond_highlights() -> Result {
    let text = include_str!("fixtures/structure-highlights/native-prime.cdxml").to_owned();
    let binary = from_cdx(include_bytes!(
        "fixtures/structure-highlights/native-prime.cdx"
    ))?;
    for text in [text, binary] {
        let source = import_cdxml(&text)?.document;
        for drawing in std::iter::once(source.clone()).chain(both(&source)?) {
            assert_eq!(drawing.atoms.len(), 5);
            assert_eq!(drawing.bonds.len(), 3);
            assert_eq!(
                drawing
                    .atoms
                    .iter()
                    .map(|a| a.element.as_str())
                    .collect::<Vec<_>>(),
                ["C", "O", "O", "C", "C"]
            );
            assert_eq!(
                drawing
                    .atoms
                    .iter()
                    .map(|a| a.display.highlight)
                    .collect::<Vec<_>>(),
                [None, Some(CYAN), None, Some(YELLOW), Some(YELLOW)]
            );
            assert_eq!(
                drawing
                    .bonds
                    .iter()
                    .map(|b| b.highlight)
                    .collect::<Vec<_>>(),
                [None, None, Some(YELLOW)]
            );
            assert_eq!(
                drawing.bonds.iter().map(|b| b.order).collect::<Vec<_>>(),
                [1, 2, 1]
            );
        }
    }
    Ok(())
}

#[test]
fn native_clipboard_and_expand_label_preserve_independent_group_paint() -> Result {
    // These exact files were saved after real Prime Copy/New/Paste and
    // Structure > Expand Label. Native loading truncated the original input
    // decimals to cyan green=229 and yellow green=197; retain those facts.
    let cyan = Color::Custom([129, 229, 255]);
    let yellow = Color::Custom([255, 197, 0]);
    for (text, contracted) in [
        (
            include_str!("fixtures/structure-highlights/native-contracted.cdxml"),
            true,
        ),
        (
            include_str!("fixtures/structure-highlights/native-expanded.cdxml"),
            false,
        ),
    ] {
        let source = import_cdxml(text)?.document;
        assert_eq!(source.abbreviations.len(), usize::from(contracted));
        if contracted {
            assert_eq!(source.abbreviations[0].highlight, Some(cyan));
        }
        for back in std::iter::once(source.clone()).chain(both(&source)?) {
            assert_eq!(back.atoms.len(), 3);
            assert_eq!(back.bonds.len(), 2);
            assert_eq!(
                back.atoms
                    .iter()
                    .filter(|a| a.display.highlight == Some(RED))
                    .count(),
                1
            );
            assert_eq!(
                back.atoms
                    .iter()
                    .filter(|a| a.display.highlight == Some(cyan))
                    .count(),
                1
            );
            assert_eq!(
                back.atoms
                    .iter()
                    .filter(|a| a.display.highlight.is_none())
                    .count(),
                1
            );
            assert_eq!(
                back.bonds
                    .iter()
                    .filter(|b| b.highlight == Some(yellow))
                    .count(),
                1
            );
            assert_eq!(
                back.bonds.iter().filter(|b| b.highlight.is_none()).count(),
                1
            );
            assert!(back.bonds.iter().all(|b| b.order == 1));
        }
    }
    Ok(())
}

#[test]
fn every_rgb_channel_survives_native_truncation_and_cdx_rounding() -> Result {
    let mut source = Document::default();
    for channel in 0..=u8::MAX {
        let id = source.add_atom("C", Point::new(f32::from(channel) * 28., 0.));
        source.atom_mut(id).ok_or("atom")?.display.highlight = Some(Color::Custom([channel; 3]));
    }
    let text = xml(&source)?;
    let parsed = roxmltree::Document::parse(&text)?;
    let colors = parsed
        .descendants()
        .find(|n| n.has_tag_name("colortable"))
        .ok_or("color table")?
        .children()
        .filter(|n| n.has_tag_name("color"))
        .collect::<Vec<_>>();
    let atoms = parsed
        .descendants()
        .filter(|n| n.has_tag_name("n"))
        .collect::<Vec<_>>();
    assert_eq!(atoms.len(), 256);
    for (atom, channel) in atoms.into_iter().zip(0..=u8::MAX) {
        let index = atom
            .attribute("highlightColor")
            .ok_or("highlight index")?
            .parse::<usize>()?;
        let color = colors
            .get(index.checked_sub(2).ok_or("special color")?)
            .ok_or("color")?;
        for axis in ["r", "g", "b"] {
            let value = color.attribute(axis).ok_or("component")?.parse::<f64>()?;
            assert_eq!(
                (value * 255.).trunc(),
                f64::from(channel),
                "native channel {channel}"
            );
            assert_eq!(
                (value * 255.).round(),
                f64::from(channel),
                "rounded channel {channel}"
            );
            assert_eq!(
                (value * 65535.).round(),
                f64::from(channel) * 257.,
                "CDX channel {channel}"
            );
            assert!((0.0..=1.0).contains(&value));
            if channel == 0 || channel == 255 {
                assert_eq!(value, f64::from(channel) / 255.);
            } else {
                assert!(value > f64::from(channel) / 255.);
                assert!(value - f64::from(channel) / 255. <= 0.00000001000001);
            }
        }
    }
    Ok(())
}

#[test]
fn atom_bond_and_literal_black_paint_survive_both_editable_formats() -> Result {
    let mut source = Document::default();
    let a = source.add_atom("C", Point::new(0., 0.));
    let b = source.add_atom("C", Point::new(24., 14.));
    let o = source.add_atom("O", Point::new(48., 0.));
    let n = source.add_atom("N", Point::new(100., 0.));
    source.add_bond(a, b, 1, "plain");
    source.add_bond(b, o, 1, "plain");
    source.atom_mut(a).ok_or("carbon")?.display.highlight = Some(YELLOW);
    source.atom_mut(o).ok_or("oxygen")?.display.highlight = Some(CYAN);
    source.atom_mut(n).ok_or("nitrogen")?.display.highlight = Some(Color::Custom([0; 3]));
    source.atom_mut(n).ok_or("nitrogen")?.charge = 1;
    source.bonds[0].highlight = Some(RED);
    source.bonds[0].color = Color::Custom([11, 37, 59]);
    let before = source.clone();
    for back in both(&source)? {
        assert_eq!(back.atoms.len(), 4);
        assert_eq!(back.bonds.len(), 2);
        assert_eq!(
            back.atoms
                .iter()
                .map(|a| a.display.highlight)
                .collect::<Vec<_>>(),
            vec![Some(YELLOW), None, Some(CYAN), Some(Color::Custom([0; 3]))]
        );
        assert_eq!(back.atoms[3].charge, 1);
        assert_eq!(back.atoms[3].element, "N");
        assert_eq!(back.bonds[0].highlight, Some(RED));
        assert_eq!(back.bonds[0].color, Color::Custom([11, 37, 59]));
        assert_eq!(back.bonds[1].highlight, None);
        assert_eq!(
            back.bonds.iter().map(|b| b.order).collect::<Vec<_>>(),
            [1, 1]
        );
    }
    assert_eq!(source, before, "export does not mutate the editable source");
    Ok(())
}

const CONTRACTED: &str = r#"<CDXML BondLength="28">
<colortable><color r="1" g="1" b="1"/><color r="0" g="0" b="0"/>
<color r="0.505882353" g="0.901960784" b="1"/>
<color r="0.874509804" g="0.278431373" b="0.243137255"/>
<color r="1" g="0.776470588" b="0"/></colortable>
<page id="1"><fragment id="2"><n id="3" p="-28 0"/>
<n id="4" p="0 0" NodeType="Fragment" highlightColor="4">
<fragment id="5"><n id="6" p="0 0" Element="8" highlightColor="5"/>
<n id="7" p="28 0"/><b id="8" B="6" E="7" highlightColor="6"/>
<n id="9" p="-28 0" NodeType="ExternalConnectionPoint"/>
<b id="10" B="9" E="6"/></fragment>
<t p="0 0"><s font="3" size="10" face="96">OMe</s></t></n>
<b id="11" B="3" E="4"/></fragment></page></CDXML>"#;

#[test]
fn contracted_label_paint_preserves_existing_internal_colors() -> Result {
    let source = import_cdxml(CONTRACTED)?.document;
    let group = source.abbreviations.first().ok_or("contracted label")?;
    assert_eq!(group.highlight, Some(CYAN));
    assert_eq!(
        source.atom(group.anchor).ok_or("anchor")?.display.highlight,
        Some(RED)
    );
    assert_eq!(
        source
            .atoms
            .iter()
            .filter(|a| a.display.highlight == Some(CYAN))
            .count(),
        1
    );
    assert_eq!(
        source
            .bonds
            .iter()
            .filter(|b| b.highlight == Some(YELLOW))
            .count(),
        1
    );
    assert_eq!(
        source
            .bonds
            .iter()
            .filter(|b| b.highlight.is_none())
            .count(),
        1
    );
    for back in both(&source)? {
        assert_eq!(back.abbreviations.len(), 1);
        assert_eq!(back.abbreviations[0].highlight, Some(CYAN));
        assert_eq!(
            back.atoms
                .iter()
                .filter(|a| a.display.highlight == Some(RED))
                .count(),
            1
        );
        assert_eq!(
            back.atoms
                .iter()
                .filter(|a| a.display.highlight == Some(CYAN))
                .count(),
            1
        );
        assert_eq!(
            back.bonds
                .iter()
                .filter(|b| b.highlight == Some(YELLOW))
                .count(),
            1
        );
        assert_eq!(
            back.bonds.iter().filter(|b| b.highlight.is_none()).count(),
            1
        );
    }
    Ok(())
}

#[test]
fn anchor_only_paint_is_not_promoted_to_a_whole_contracted_group() -> Result {
    let mut source = import_cdxml(CONTRACTED)?.document;
    source.abbreviations[0].highlight = None;
    for atom in &mut source.atoms {
        if atom.element != "O" {
            atom.display.highlight = None;
        }
    }
    for bond in &mut source.bonds {
        bond.highlight = None;
    }
    for back in both(&source)? {
        assert!(back.abbreviations.is_empty());
        assert_eq!(
            back.atoms
                .iter()
                .filter(|a| a.display.highlight.is_some())
                .count(),
            1
        );
        assert!(back.bonds.iter().all(|b| b.highlight.is_none()));
    }
    assert_eq!(source.abbreviations.len(), 1);
    Ok(())
}

#[test]
fn invalid_highlight_palette_indices_fail_instead_of_disappearing() -> Result {
    for tag in ["n", "b"] {
        for index in ["-1", "6", "not-a-number", "65536"] {
            let node = if tag == "n" {
                format!(
                    r#"<n id="3" p="0 0" highlightColor="{index}"/><n id="4" p="28 0"/><b id="5" B="3" E="4"/>"#
                )
            } else {
                format!(
                    r#"<n id="3" p="0 0"/><n id="4" p="28 0"/><b id="5" B="3" E="4" highlightColor="{index}"/>"#
                )
            };
            let xml =
                format!(r#"<CDXML><page id="1"><fragment id="2">{node}</fragment></page></CDXML>"#);
            assert!(import_cdxml(&xml).is_err(), "{tag} {index}");
        }
    }
    let malformed_group = CONTRACTED.replace("highlightColor=\"4\"", "highlightColor=\"999\"");
    assert!(import_cdxml(&malformed_group).is_err());
    Ok(())
}

#[test]
fn binary_highlight_property_is_native_u16_0308() -> Result {
    let source = r#"<CDXML><page id="1"><fragment id="2"><n id="3" p="0 0" highlightColor="29"/><n id="4" p="28 0"/><b id="5" B="3" E="4" highlightColor="15"/></fragment></page></CDXML>"#;
    let bytes = to_cdx(source)?;
    assert_eq!(
        bytes
            .windows(6)
            .filter(|b| *b == [8, 3, 2, 0, 29, 0])
            .count(),
        1
    );
    assert_eq!(
        bytes
            .windows(6)
            .filter(|b| *b == [8, 3, 2, 0, 15, 0])
            .count(),
        1
    );
    let decoded = from_cdx(&bytes)?;
    let document = roxmltree::Document::parse(&decoded)?;
    assert_eq!(
        document
            .descendants()
            .find(|n| n.attribute("id") == Some("3"))
            .and_then(|n| n.attribute("highlightColor")),
        Some("29")
    );
    assert_eq!(
        document
            .descendants()
            .find(|n| n.attribute("id") == Some("5"))
            .and_then(|n| n.attribute("highlightColor")),
        Some("15")
    );
    Ok(())
}
