use reshiki_io::{chemistry::cdxml, engine::LocalEngine, exchange, formats};
use reshiki_model::{document::Document, palette::Color};
use std::collections::BTreeSet;

async fn read(xml: &str, format: &str) -> (Document, Vec<String>) {
    let bytes = if format == "cdx" {
        exchange::to_cdx(xml).unwrap()
    } else {
        xml.as_bytes().to_vec()
    };
    formats::read_with_warnings(bytes, format, &LocalEngine::in_process())
        .await
        .unwrap_or_else(|error| panic!("{format}: {error}"))
}

const HIDDEN_FIGURE: &str = r#"<CDXML BondLength='14.4'>
<colortable><color r='1' g='1' b='1'/><color r='0' g='0' b='0'/>
<color r='1' g='0' b='0'/><color r='0' g='0' b='1'/></colortable>
<page id='1'><fragment id='2'><n id='3' p='10 10'/><n id='4' p='24.4 10' Element='8'/>
<b id='5' B='3' E='4'/></fragment><group id='6' Integral='yes'>
<graphic id='7' GraphicType='Rectangle' BoundingBox='30 30 70 60' color='4' Visible='no'>
<t id='8' p='30 50'><s>HIDDEN child caption</s></t></graphic>
<graphic id='9' GraphicType='Rectangle' BoundingBox='90 30 130 60' color='5' Visible='yes'/>
<t id='10' p='30 80' Visible='no'><s>HIDDEN caption</s></t>
<t id='11' p='90 80' Visible='yes'><s>VISIBLE caption</s></t>
<arrow id='12' Tail3D='30 100 0' Head3D='70 100 0' ArrowheadHead='Full' color='4' Visible='no'/>
<arrow id='13' Tail3D='90 100 0' Head3D='130 100 0' ArrowheadHead='Full' Visible='yes'/>
<curve id='14' CurvePoints='30 130 30 130 40 100 60 160 70 130 80 100 100 100 110 130 110 130'
ArrowheadHead='Full' ArrowheadType='Hollow' color='4' Visible='no'/>
<curve id='15' CurvePoints='30 170 30 170 40 140 60 200 70 170 70 170' color='4' Visible='no'/>
<curve id='16' CurvePoints='90 170 90 170 100 140 120 200 130 170 130 170' color='5' Visible='yes'/>
<group id='17'><curve id='18' CurvePoints='150 30 150 30 150 30 190 60 190 60 190 60'
CurveType='129' FillType='Solid' color='4' Visible='no'/>
<curve id='19' CurvePoints='150 30 150 30 150 30 190 60 190 60 190 60' CurveType='1' color='5'/></group>
</group></page></CDXML>"#;

#[tokio::test]
async fn hidden_presentations_are_omitted_once_and_group_members_stay_valid() {
    for format in ["cdxml", "cdx"] {
        let (document, notices) = read(HIDDEN_FIGURE, format).await;
        assert_eq!(document.atoms.len(), 2, "{format}");
        assert_eq!(document.bonds.len(), 1, "{format}");
        assert_eq!(document.annotations.len(), 1, "{format}");
        assert_eq!(document.annotations[0].text, "VISIBLE caption");
        assert_eq!(document.arrows.len(), 1, "{format}");
        assert_eq!(document.graphics.len(), 3, "{format}");
        assert!(document.graphics.iter().all(|graphic| {
            graphic.style.stroke == Color::Custom([0, 0, 255]) && graphic.style.fill.is_none()
        }));
        assert_eq!(notices.len(), 1, "{format}: {notices:?}");
        assert!(notices[0].contains("Hidden drawing objects were omitted"));
        assert_eq!(document.groups.len(), 1, "{format}");
        let visible_ids: BTreeSet<_> = document
            .graphics
            .iter()
            .map(|g| g.id)
            .chain(document.annotations.iter().map(|a| a.id))
            .chain(document.arrows.iter().map(|a| a.id))
            .collect();
        assert_eq!(
            document.groups[0]
                .members
                .iter()
                .copied()
                .collect::<BTreeSet<_>>(),
            visible_ids
        );
        assert!(document.groups[0].integral);
        document.validate().unwrap();

        let (implicit_visible, implicit_notices) =
            read(&HIDDEN_FIGURE.replace(" Visible='yes'", ""), format).await;
        assert_eq!(implicit_visible, document, "{format}");
        assert_eq!(implicit_notices, notices);
    }
    let imported = cdxml::import_cdxml(HIDDEN_FIGURE).unwrap();
    assert_eq!(imported.notices.len(), 1);
}

#[test]
fn standalone_graphic_reader_does_not_merge_hidden_fill_into_visible_outline() {
    let graphics = cdxml::graphics::parse(HIDDEN_FIGURE, 1., 100, &BTreeSet::new()).unwrap();
    assert_eq!(graphics.graphics.len(), 3);
    assert!(
        graphics
            .graphics
            .iter()
            .all(|graphic| graphic.style.as_ref().unwrap().fill.is_none())
    );
}

#[tokio::test]
async fn document_label_face_is_inherited_without_text_and_explicit_overrides_win() {
    let xml = r#"<CDXML BondLength='14.4' LabelFont='9' LabelSize='18' LabelFace='3'>
    <fonttable><font id='9' name='Times New Roman'/></fonttable><page id='1'><fragment id='2'>
    <n id='3' Element='8' p='0 0'/>
    <n id='4' Element='8' p='40 0' LabelFace='4'/>
    <n id='5' Element='8' p='80 0'><t LabelFace='2'><s>O</s></t></n>
    <n id='6' Element='8' p='120 0'><t LabelFace='2'><s face='0'>O</s></t></n>
    </fragment></page></CDXML>"#;
    for format in ["cdxml", "cdx"] {
        let (document, notices) = read(xml, format).await;
        assert!(notices.is_empty(), "{format}: {notices:?}");
        assert_eq!(document.atoms.len(), 4);
        for (atom, expected_face) in document.atoms.iter().zip([3, 4, 2, 0]) {
            let style = atom.text_style.as_ref().unwrap_or_else(|| {
                panic!("{format}: missing style for atom {}: {document:?}", atom.id)
            });
            assert_eq!(style.family, "Times New Roman", "{format}");
            assert_eq!(style.size_pt, 18., "{format}");
            assert_eq!(
                (style.bold, style.italic, style.underline),
                (
                    expected_face & 1 != 0,
                    expected_face & 2 != 0,
                    expected_face & 4 != 0
                ),
                "{format}"
            );
        }
    }
}

#[tokio::test]
async fn hidden_chemical_atoms_bonds_and_atom_labels_are_never_discarded() {
    for object in [
        "<n id='3' Element='8' p='0 0' Visible='no'/>",
        "<n id='3' p='0 0'/><n id='4' p='14.4 0'/><b id='5' B='3' E='4' Visible='no'/>",
        "<n id='3' Element='8' p='0 0'><t Visible='no'><s>O</s></t></n>",
    ] {
        let xml = format!(
            "<CDXML BondLength='14.4'><page id='1'><fragment id='2'>{object}</fragment></page></CDXML>"
        );
        for format in ["cdxml", "cdx"] {
            let bytes = if format == "cdx" {
                exchange::to_cdx(&xml).unwrap()
            } else {
                xml.as_bytes().to_vec()
            };
            let error = formats::read_with_warnings(bytes, format, &LocalEngine::in_process())
                .await
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("Hidden chemical objects are not supported"),
                "{format}: {error}"
            );
        }
    }
}

#[tokio::test]
async fn existing_hidden_charge_representation_keeps_its_chemistry_and_visibility() {
    let xml = r#"<CDXML BondLength='14.4'><page id='1'><fragment id='2'>
    <n id='3' Element='7' Charge='1' p='0 0'/></fragment>
    <graphic id='4' GraphicType='Symbol' SymbolType='Plus' BoundingBox='4 -8 10 -2' Visible='no'>
    <represent object='3' attribute='Charge'/></graphic></page></CDXML>"#;
    for format in ["cdxml", "cdx"] {
        let (document, notices) = read(xml, format).await;
        assert!(notices.is_empty(), "{format}: {notices:?}");
        assert_eq!(document.atoms[0].charge, 1);
        assert!(document.atoms[0].display.hide_charge);
        assert!(document.graphics.is_empty());
    }
}

#[tokio::test]
async fn hidden_unspecified_dummy_keeps_existing_invisible_anchor_connectivity() {
    let xml = r#"<CDXML BondLength='14.4'><page id='1'><fragment id='2'>
    <n id='3' p='0 0'/><n id='4' Element='0' NodeType='Unspecified' Visible='no' p='14.4 0'>
    <t><s>*</s></t></n><n id='5' p='28.8 0'/><b id='6' B='3' E='4'/><b id='7' B='4' E='5'/>
    </fragment></page></CDXML>"#;
    for format in ["cdxml", "cdx"] {
        let (document, notices) = read(xml, format).await;
        assert!(notices.is_empty(), "{format}: {notices:?}");
        assert_eq!(document.atoms.len(), 3);
        assert_eq!(document.bonds.len(), 2);
        assert_eq!(
            reshiki_model::attachments::editor_markers(&document).count(),
            1
        );
        assert!(reshiki_model::scene::primitives(&document).iter().all(|primitive| {
            !matches!(primitive, reshiki_model::scene::Primitive::Text { text, .. } if text == "*")
        }));
    }
}

#[tokio::test]
async fn drawing_only_variable_import_also_reports_omitted_hidden_presentations_once() {
    let xml = r#"<CDXML BondLength='14.4'><page id='1'><fragment id='2'>
    <n id='3' Element='0' NodeType='Unspecified' p='0 0'><t><s>R1</s></t></n>
    <n id='4' p='14.4 0'/><b id='5' B='3' E='4'/></fragment>
    <graphic id='6' GraphicType='Rectangle' BoundingBox='30 20 70 60' Visible='no'/>
    </page></CDXML>"#;
    for format in ["cdxml", "cdx"] {
        let (document, notices) = read(xml, format).await;
        assert_eq!(document.atoms.len(), 2);
        assert_eq!(document.bonds.len(), 1);
        assert!(document.graphics.is_empty());
        assert_eq!(
            notices
                .iter()
                .filter(|notice| notice.contains("Hidden drawing objects were omitted"))
                .count(),
            1
        );
        assert_eq!(
            notices
                .iter()
                .filter(|notice| notice.contains("variable labels"))
                .count(),
            1
        );
    }
}

const UNUSED_PALETTE: &str =
    include_str!("../../../tests/fixtures/chemdraw-text-defaults/unused-palette.cdxml");

#[test]
fn unused_palette_entries_do_not_create_synthetic_atom_styles() {
    for channel in ["-0.1", "0", "1.1", "1e90"] {
        let xml = UNUSED_PALETTE.replace("r=\"1.1\"", &format!("r=\"{channel}\""));
        let scene = cdxml::assemble_cdxml(&cdxml::prepare_cdxml(&xml).unwrap()).unwrap();
        assert!(
            scene
                .base
                .atoms
                .iter()
                .all(|atom| atom.text_style.is_none())
        );
        let imported = scene.into_document().unwrap();
        assert_eq!(imported.document.atoms.len(), 2);
        assert_eq!(imported.document.bonds.len(), 1);
        assert!(
            imported
                .document
                .atoms
                .iter()
                .all(|atom| atom.text_style.is_none())
        );
        assert!(imported.notices.is_empty());
    }
}

#[test]
fn used_invalid_styles_still_fail_at_their_existing_boundaries() {
    for channel in ["-0.1", "1.1", "1e90"] {
        let xml = UNUSED_PALETTE.replace("r=\"1.1\"", &format!("r=\"{channel}\""));
        let bad_bond = xml.replace("color=\"0\"", "color=\"2\"");
        let bad_text = xml.replace(
            "<n id=\"4\" p=\"14.4 0\"/>",
            "<n id='4' Element='8' NumHydrogens='1' p='14.4 0'><t><s color='2'>OH</s></t></n>",
        );
        for source in [bad_bond, bad_text] {
            let scene = cdxml::assemble_cdxml(&cdxml::prepare_cdxml(&source).unwrap()).unwrap();
            assert!(
                scene.into_document().is_err(),
                "accepted used color {channel}"
            );
        }
    }
    let missing_used_color = UNUSED_PALETTE.replace(
        "<n id=\"4\" p=\"14.4 0\"/>",
        "<n id='4' Element='8' p='14.4 0'><t><s>O</s></t></n>",
    );
    assert!(cdxml::assemble_cdxml(&cdxml::prepare_cdxml(&missing_used_color).unwrap()).is_err());
    for channel in ["nan", "inf", "-inf"] {
        let xml = UNUSED_PALETTE.replace("r=\"1.1\"", &format!("r=\"{channel}\""));
        assert!(
            cdxml::prepare_cdxml(&xml).is_err(),
            "accepted nonfinite palette {channel}"
        );
    }
}

#[test]
fn explicit_atom_overrides_do_not_validate_unused_document_defaults() {
    let xml = UNUSED_PALETTE
        .replace(
            "<CDXML BondLength=\"14.4\">",
            "<CDXML BondLength='14.4' LabelFace='bad' LabelColor='900'>",
        )
        .replace("<n id=\"3\"", "<n LabelFace='0' LabelColor='0' id=\"3\"")
        .replace("<n id=\"4\"", "<n LabelFace='0' LabelColor='0' id=\"4\"");
    let imported = cdxml::import_cdxml(&xml).unwrap();
    assert_eq!(imported.document.atoms.len(), 2);
    assert_eq!(imported.document.bonds.len(), 1);
    assert!(imported.notices.is_empty());
}

#[tokio::test]
async fn declared_document_face_and_color_still_apply_without_atom_text() {
    let xml = "<CDXML BondLength='14.4' LabelFace='3' LabelColor='2'><colortable><color r='0.5' g='0' b='0'/></colortable><page id='1'><fragment id='2'><n id='3' Element='8' p='0 0'/></fragment></page></CDXML>";
    for format in ["cdxml", "cdx"] {
        let (document, notices) = read(xml, format).await;
        let style = document.atoms[0].text_style.as_ref().unwrap();
        assert!(style.bold && style.italic);
        assert_eq!(style.color, Color::Custom([128, 0, 0]));
        assert!(notices.is_empty());
    }
}
