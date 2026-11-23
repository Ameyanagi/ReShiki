use super::*;
use crate::document::{Annotation, Point};

fn drawing() -> Document {
    Document {
        annotations: vec![Annotation {
            id: 1,
            position: Point::default(),
            text: "O₃ label".into(),
            format: Default::default(),
            angle: 0.,
        }],
        ..Document::default()
    }
}

#[tokio::test]
async fn publication_protocol_preserves_existing_bytes_and_details() {
    for format in ["png", "svg", "svg-office", "pdf"] {
        let doc = drawing();
        let expected = export::publication(
            &LocalEngine::default(),
            doc.clone(),
            format,
            false,
            true,
            export::Background::Color([250, 248, 244]),
            144,
            1_000_000,
        )
        .await
        .unwrap();
        let request = Request::Publication {
            document: doc,
            format: format.into(),
            pages: false,
            editable: true,
            background: Background::Color([250, 248, 244]),
            dpi: 144,
            max_pixels: 1_000_000,
        };
        let bytes = respond(&serde_json::to_vec(&request).unwrap())
            .await
            .unwrap();
        let actual = decode(bytes).unwrap();
        assert_eq!(actual.bytes, expected.bytes, "{format}");
        assert_eq!(actual.details, expected.details, "{format}");
    }
}

#[tokio::test]
async fn save_protocol_preserves_existing_formats_and_operation_errors() {
    for format in ["svg", "png", "pdf", "cdxml", "cdx"] {
        let doc = drawing();
        let expected = formats::write(doc.clone(), format).await.unwrap();
        let request = Request::Save {
            document: doc,
            format: format.into(),
        };
        let actual = decode(
            respond(&serde_json::to_vec(&request).unwrap())
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(actual.bytes, expected, "{format}");
    }
    let doc = drawing();
    let expected = formats::write(doc.clone(), "not-a-format")
        .await
        .unwrap_err();
    let request = Request::Save {
        document: doc,
        format: "not-a-format".into(),
    };
    let reply = respond(&serde_json::to_vec(&request).unwrap())
        .await
        .unwrap();
    assert_eq!(decode(reply).err().unwrap(), expected);
}

#[test]
fn font_envelope_keeps_job_bytes_and_rejects_truncated_headers() {
    let request = br#"{"job":"clipboard"}"#;
    let envelope = request_fonts(request).unwrap();
    let (fonts, body) = split(&envelope, MAX_REQUEST_BYTES).unwrap();
    let fonts: Vec<String> = serde_json::from_slice(fonts).unwrap();
    assert!(fonts.len() <= crate::style::font_generation());
    assert_eq!(body, request);
    assert!(install_fonts(&0_u32.to_le_bytes()).is_err());
    assert!(install_fonts(&[0, 0, 0]).is_err());
    assert!(install_fonts(&[255; 4]).is_err());
    assert!(decode(vec![0, 0, 0]).is_err());
    assert!(decode(vec![255; 4]).is_err());
}
