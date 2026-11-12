use super::*;
use crate::{
    document::{Annotation, Point},
    models3d::{ForceField, Model3d, Placed},
    theme_files::ThemeFile,
};

fn drawing() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(40., 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.annotations.push(Annotation {
        id: doc.next_id(),
        position: Point::new(0., 60.),
        text: "ReShiki 日本語".into(),
        format: Default::default(),
    });
    doc
}

fn envelope(json: &[u8], declared: usize) -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    bytes.push(VERSION);
    bytes.extend_from_slice(&(declared as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(json));
    let mut encoder = ZlibEncoder::new(bytes, Compression::default());
    encoder.write_all(json).unwrap();
    encoder.finish().unwrap()
}

fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[12, 34, 56, 255]).unwrap();
    }
    bytes
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut bytes = (data.len() as u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(data);
    bytes.extend_from_slice(&crc(kind, data).to_be_bytes());
    bytes
}

fn insert_chunk(mut bytes: Vec<u8>, kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let tail = bytes.split_off(33);
    bytes.extend_from_slice(&chunk(kind, data));
    bytes.extend_from_slice(&tail);
    bytes
}

#[test]
fn envelope_round_trip_has_compact_json_length_and_hash() {
    let doc = drawing();
    let bytes = encode(&doc).unwrap();
    assert_eq!(&bytes[..5], b"RSKD\x01");
    let json = serde_json::to_vec(&visible(&doc)).unwrap();
    assert_eq!(
        u32::from_le_bytes(bytes[5..9].try_into().unwrap()) as usize,
        json.len()
    );
    assert_eq!(&bytes[9..HEADER_LEN], Sha256::digest(&json).as_slice());
    assert_eq!(decode(&bytes).unwrap(), doc);
}

#[test]
fn envelope_rejects_truncation_and_bad_signature() {
    let bytes = encode(&drawing()).unwrap();
    for length in 0..bytes.len() {
        assert!(decode(&bytes[..length]).is_err(), "accepted {length} bytes");
    }
    let mut bytes = bytes;
    bytes[0] ^= 1;
    assert!(decode(&bytes).unwrap_err().contains("signature"));
}

#[test]
fn envelope_rejects_corrupt_hash_and_both_length_mismatches() {
    let mut bytes = encode(&drawing()).unwrap();
    bytes[9] ^= 1;
    assert!(decode(&bytes).unwrap_err().contains("checksum"));
    let json = serde_json::to_vec(&drawing()).unwrap();
    for declared in [json.len() - 1, json.len() + 1] {
        assert!(
            decode(&envelope(&json, declared))
                .unwrap_err()
                .contains("length")
        );
    }
}

#[test]
fn future_envelope_and_document_versions_request_an_update() {
    assert!(
        decode(b"RSKD\x02")
            .unwrap_err()
            .contains("newer version of ReShiki")
    );
    assert!(decode(b"RSKD\0").is_err());
    let mut doc = drawing();
    doc.version = crate::document::VERSION + 1;
    assert!(
        decode(&encode(&doc).unwrap())
            .unwrap_err()
            .contains("newer version of ReShiki")
    );
}

#[test]
fn declared_size_cap_is_checked_before_inflation() {
    let mut bytes = b"RSKD\x01".to_vec();
    bytes.extend_from_slice(&((DECODE_LIMIT + 1) as u32).to_le_bytes());
    assert!(decode(&bytes).unwrap_err().contains("64 MiB"));
}

#[test]
fn inflation_stops_at_declared_length_plus_one() {
    let bytes = envelope(&vec![b'x'; 1024 * 1024], 1);
    assert!(decode(&bytes).unwrap_err().contains("length"));
}

#[test]
fn decode_validates_the_document_boundary() {
    let mut doc = drawing();
    doc.atoms[1].id = doc.atoms[0].id;
    assert!(
        decode(&encode(&doc).unwrap())
            .unwrap_err()
            .contains("object ID")
    );
}

#[test]
fn visible_removes_private_metadata_and_preserves_rendering() {
    let mut doc = drawing();
    doc.version = crate::document::VERSION;
    doc.recent_colors = vec![[12, 34, 56]];
    doc.page_layout = Some(Default::default());
    let mut theme = ThemeFile::capture(&doc);
    theme.author = "Private Author".into();
    theme.source = "Private Source".into();
    theme.license = "Private License".into();
    doc.custom_theme = Some(Box::new(theme));
    let mut model = Model3d::new(
        &doc,
        vec![
            Placed {
                id: doc.atoms[0].id,
                xyz: [0., 0., 0.],
            },
            Placed {
                id: doc.atoms[1].id,
                xyz: [1.4, 0., 0.],
            },
        ],
        vec![],
        ForceField::default(),
    );
    model.notes = vec!["Private Model Note".into()];
    doc.models.push(model);
    doc.validate().unwrap();
    let public = visible(&doc);
    let mut expected = doc.clone();
    expected.recent_colors.clear();
    expected.page_layout = None;
    expected.models[0].notes.clear();
    let theme = expected.custom_theme.as_mut().unwrap();
    theme.author.clear();
    theme.source.clear();
    theme.license.clear();
    assert_eq!(public, expected);
    assert_eq!(crate::scene::svg(&doc), crate::scene::svg(&public));
    let decoded = decode(&encode(&doc).unwrap()).unwrap();
    assert_eq!(decoded, public);
    assert!(!doc.models[0].notes.is_empty());
    assert!(doc.page_layout.is_some());
    let json = serde_json::to_string(&decoded).unwrap();
    assert!(!json.contains("Private"));
}

#[test]
fn png_payload_has_standard_itxt_and_valid_crcs_and_decodes_pixels() {
    let doc = drawing();
    let envelope = encode(&doc).unwrap();
    let bytes = embed("png", png(), &envelope).unwrap();
    let mut remaining = &bytes[8..];
    let mut kinds = Vec::new();
    while !remaining.is_empty() {
        let length = u32::from_be_bytes(remaining[..4].try_into().unwrap()) as usize;
        let kind = &remaining[4..8];
        let data = &remaining[8..8 + length];
        let checksum = u32::from_be_bytes(remaining[8 + length..12 + length].try_into().unwrap());
        assert_eq!(checksum, crc(kind, data));
        if kind == b"iTXt" {
            assert!(data.starts_with(b"dev.reshiki.drawing\0\0\0\0\0"));
            assert_eq!(
                STANDARD.decode(&data[KEYWORD.len() + 5..]).unwrap(),
                envelope
            );
        }
        kinds.push(kind.to_vec());
        remaining = &remaining[12 + length..];
    }
    assert_eq!(kinds[0], b"IHDR");
    assert_eq!(kinds[1], b"iTXt");
    let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(&pixels[..info.buffer_size()], &[12, 34, 56, 255]);
    assert_eq!(read(&bytes).unwrap().unwrap(), doc);
}

#[test]
fn png_without_payload_is_absent_and_stops_at_iend() {
    let bytes = png();
    assert!(read(&bytes).is_none());
    assert!(read(b"not a PNG").is_none());
    assert!(read(&bytes[..20]).is_none());
    let bytes = insert_chunk(bytes, b"iTXt", b"other.keyword\0\0\0\0\0invalid");
    assert!(read(&bytes).is_none());
    let mut bytes = png();
    let payload = embed("png", png(), &encode(&drawing()).unwrap()).unwrap();
    bytes.extend_from_slice(&payload[33..]);
    assert!(read(&bytes).is_none());
}

#[test]
fn damaged_present_payload_is_an_error() {
    let bytes = embed("png", png(), &encode(&drawing()).unwrap()).unwrap();
    let mut bad_crc = bytes.clone();
    bad_crc[33 + 8 + KEYWORD.len() + 5] ^= 1;
    assert!(read(&bad_crc).unwrap().is_err());
    let text = b"dev.reshiki.drawing\0\0\0\0\0!";
    assert!(
        read(&insert_chunk(png(), b"iTXt", text))
            .unwrap()
            .unwrap_err()
            .contains("encoding")
    );
    let mut text = b"dev.reshiki.drawing\0\0\0\0\0".to_vec();
    text.extend_from_slice(STANDARD.encode(b"broken envelope").as_bytes());
    assert!(read(&insert_chunk(png(), b"iTXt", &text)).unwrap().is_err());
    let end = 33 + 8 + KEYWORD.len() + 5;
    assert!(
        read(&bytes[..end])
            .unwrap()
            .unwrap_err()
            .contains("Truncated")
    );
    let malformed = b"dev.reshiki.drawing\0\0\0missing-fields";
    assert!(
        read(&insert_chunk(png(), b"iTXt", malformed))
            .unwrap()
            .is_err()
    );
}

#[test]
fn embed_rejects_other_formats_and_invalid_png_headers() {
    assert!(embed("svg", png(), &[]).is_err());
    assert!(embed("png", vec![], &[]).is_err());
    assert!(embed("png", png()[..32].to_vec(), &[]).is_err());
}
