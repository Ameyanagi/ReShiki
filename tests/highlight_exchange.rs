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
fn contracted_and_expanded_atom_ink_follow_their_own_highlight_backgrounds() -> Result {
    use reshiki::{
        canvas_theme::{self, CanvasTheme},
        color_contrast,
        palette::Palette,
    };
    fn text_rgb(node: roxmltree::Node<'_, '_>) -> Result<[u8; 3]> {
        let run = node
            .children()
            .find(|n| n.has_tag_name("t"))
            .and_then(|n| n.children().find(|n| n.has_tag_name("s")))
            .ok_or("label run")?;
        let index = run
            .attribute("color")
            .ok_or("text color")?
            .parse::<usize>()?;
        if index < 2 {
            return Ok(if index == 0 { [0; 3] } else { [255; 3] });
        }
        let color = node
            .document()
            .descendants()
            .find(|n| n.has_tag_name("colortable"))
            .and_then(|n| {
                n.children()
                    .filter(|n| n.has_tag_name("color"))
                    .nth(index - 2)
            })
            .ok_or("palette color")?;
        let mut rgb = [0; 3];
        for (value, axis) in rgb.iter_mut().zip(["r", "g", "b"]) {
            *value = (color.attribute(axis).ok_or("channel")?.parse::<f64>()? * 255.).round() as u8;
        }
        Ok(rgb)
    }
    for canvas in [CanvasTheme::Light, CanvasTheme::Dark] {
        for manual in [None, Some(Color::Custom([71, 40, 210])), Some(Color::Ink)] {
            let mut source = import_cdxml(CONTRACTED)?.document;
            source.canvas_theme = canvas;
            let anchor = source.abbreviations[0].anchor;
            source.abbreviations[0].highlight = Some(Color::Custom([0; 3]));
            let atom = source.atom_mut(anchor).ok_or("anchor")?;
            atom.display.highlight = Some(Color::Custom([255; 3]));
            atom.display.color_override = manual.is_some();
            atom.display.hydrogen_color = None;
            atom.text_style = manual.map(|color| reshiki::typography::TextStyle {
                color,
                ..Default::default()
            });
            let expected_outer = canvas.color(canvas_theme::atom_color(
                &source,
                source.atom(anchor).ok_or("anchor")?,
            ));
            let mut expanded = source.clone();
            expanded.abbreviations.clear();
            let expected_inner = canvas.color(canvas_theme::atom_color(
                &expanded,
                expanded.atom(anchor).ok_or("expanded anchor")?,
            ));
            if let Some(color) = manual {
                assert_eq!(expected_outer, Palette::of(&source).rgb(color));
                assert_eq!(expected_inner, expected_outer);
            } else {
                assert_ne!(expected_inner, expected_outer);
                assert!(
                    color_contrast::contrast(expected_outer, [0; 3]) >= color_contrast::TEXT_MIN
                );
                assert!(
                    color_contrast::contrast(expected_inner, [255; 3]) >= color_contrast::TEXT_MIN
                );
            }
            let before = source.clone();
            let written = xml(&source)?;
            for written in [written.clone(), from_cdx(&to_cdx(&written)?)?] {
                let tree = roxmltree::Document::parse(&written)?;
                let outer = tree
                    .descendants()
                    .find(|n| n.attribute("NodeType") == Some("Fragment"))
                    .ok_or("outer label")?;
                let inner = outer
                    .descendants()
                    .find(|n| n.has_tag_name("n") && n.attribute("Element") == Some("8"))
                    .ok_or("inner oxygen")?;
                assert_eq!(
                    text_rgb(outer)?,
                    expected_outer,
                    "visible label, {canvas:?}, {manual:?}"
                );
                assert_eq!(
                    text_rgb(inner)?,
                    expected_inner,
                    "native expanded atom, {canvas:?}, {manual:?}"
                );
                let mut back = import_cdxml(&written)?.document;
                let anchor = back.abbreviations.first().ok_or("reimported label")?.anchor;
                assert_eq!(
                    back.canvas_theme.color(canvas_theme::atom_color(
                        &back,
                        back.atom(anchor).ok_or("reimported anchor")?,
                    )),
                    expected_outer,
                    "reimported visible label, {canvas:?}, {manual:?}"
                );
                assert_eq!(back.expand_abbreviations(&[anchor]), 1);
                assert_eq!(
                    back.canvas_theme.color(canvas_theme::atom_color(
                        &back,
                        back.atom(anchor).ok_or("reimported expanded anchor")?,
                    )),
                    expected_inner,
                    "reimported expanded atom, {canvas:?}, {manual:?}"
                );
            }
            assert_eq!(source, before);
        }
    }
    Ok(())
}

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
