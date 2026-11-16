use super::*;
use crate::{chemistry::cdxml, document_styles};
use std::path::Path;

const SOURCE: &str =
    include_str!("../../../../../tests/fixtures/chemdraw-text-defaults/defaults.cdxml");

fn assert_defaults(xml: &str) {
    let document = cdxml::import_cdxml(xml).unwrap().document;
    let oxygen = document
        .atoms
        .iter()
        .find(|atom| atom.element == "O")
        .unwrap();
    let label = oxygen.text_style.as_ref().unwrap();
    assert_eq!(label.family, "Times New Roman");
    assert_eq!(label.size_pt, 18.);
    assert!(label.bold && label.italic && !label.underline);
    let caption = &document.annotations[0];
    assert_eq!(caption.text, "Inherited caption");
    assert_eq!(caption.format.style.family, "Arial");
    assert_eq!(caption.format.style.size_pt, 12.);
    assert!(!caption.format.style.bold && caption.format.style.italic);
}

#[test]
fn independent_zero_style_drawings_and_stationery_keep_inherited_text_defaults() {
    for (format, bytes) in [
        (
            "cdx",
            &include_bytes!("../../../../../tests/fixtures/chemdraw-text-defaults/defaults.cdx")[..],
        ),
        (
            "cds",
            &include_bytes!("../../../../../tests/fixtures/chemdraw-text-defaults/defaults.cds")[..],
        ),
    ] {
        let decoded = from_cdx(bytes).unwrap();
        assert_defaults(&decoded);
        for encoded in [to_cdx(&decoded).unwrap(), to_cds(&decoded).unwrap()] {
            assert_defaults(&from_cdx(&encoded).unwrap());
        }
        let style =
            document_styles::load_bytes(Path::new(&format!("source.{format}")), bytes.to_vec())
                .unwrap();
        assert_eq!(style.font_family, "Times New Roman");
        assert_eq!(style.font_size_pt, 18.);
    }
    for encoded in [to_cdx(SOURCE).unwrap(), to_cds(SOURCE).unwrap()] {
        assert_defaults(&from_cdx(&encoded).unwrap());
    }
}

#[test]
fn partial_unicode_runs_keep_label_caption_and_explicit_override_precedence() {
    let xml = r#"<CDXML BondLength='14.4' LabelFont='9' LabelSize='18' LabelFace='3'
    CaptionFont='3' CaptionSize='14' CaptionFace='2' CaptionColor='5'>
    <colortable><color r='1' g='1' b='1'/><color r='0' g='0' b='0'/>
    <color r='1' g='0' b='0'/><color r='0' g='0' b='1'/></colortable>
    <fonttable><font id='3' name='Arial' charset='cp1252'/><font id='9' name='Times New Roman'/></fonttable>
    <page id='1' CaptionSize='16'><fragment id='2'>
    <n id='3' Element='8' p='0 0' LabelFace='4'><t LabelFace='2'><s size='20'>O</s></t></n>
    </fragment><t id='4' p='40 60' CaptionFace='3'><s face='0'>触媒 α</s><s color='4'> → ΔG</s><s font='9' size='22' face='4'>終</s></t>
    </page></CDXML>"#;
    let expected = cdxml::import_cdxml(xml).unwrap().document;
    let style = expected.atoms[0].text_style.as_ref().unwrap();
    assert_eq!(style.size_pt, 20.);
    assert!(!style.bold && style.italic && !style.underline);
    for encoded in [to_cdx(xml).unwrap(), to_cds(xml).unwrap()] {
        let decoded = from_cdx(&encoded).unwrap();
        let actual = cdxml::import_cdxml(&decoded).unwrap().document;
        assert_eq!(actual.atoms[0].text_style, expected.atoms[0].text_style);
        assert_eq!(actual.annotations[0].text, expected.annotations[0].text);
        assert_eq!(actual.annotations[0].format, expected.annotations[0].format);
        assert!(decoded.contains("charset=\"utf-8\""));
    }
}

#[test]
fn unicode_without_a_font_table_uses_the_explicit_utf8_text_property() {
    let xml = "<CDXML><page id='1'><t id='2' p='0 0'><s>触媒 α → ΔG</s><s face='1'>終</s></t></page></CDXML>";
    let bytes = to_cdx(xml).unwrap();
    assert!(bytes.windows(2).any(|pair| pair == [0x09, 0x07]));
    let decoded = from_cdx(&bytes).unwrap();
    let document = cdxml::import_cdxml(&decoded).unwrap().document;
    assert_eq!(document.annotations[0].text, "触媒 α → ΔG終");
    assert_eq!(document.annotations[0].format.spans.len(), 1);
    assert!(document.annotations[0].format.spans[0].style.bold);
}
