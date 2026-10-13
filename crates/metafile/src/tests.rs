use super::*;
fn file() -> Vec<u8> {
    let mut bytes = vec![0; 108];
    for (offset, value) in [
        (0, 1),
        (4, 88),
        (24, 100),
        (28, 200),
        (32, 2640),
        (36, 1470),
        (40, 0x464d4520),
        (44, 0x10000),
        (48, 108),
        (52, 2),
        (56, 1),
        (88, 14),
        (92, 20),
        (104, 20),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&u32::to_le_bytes(value));
    }
    bytes
}
#[test]
fn physical_extent_uses_frame_not_reference_resolution() {
    assert_eq!(
        validate(&file()).unwrap(),
        Dimensions {
            width_pt: 72.,
            height_pt: 36.
        }
    );
    assert_eq!(preview_size(validate(&file()).unwrap()), (1200, 600));
    let (w, h) = preview_size(Dimensions {
        width_pt: 2880.,
        height_pt: 1440.,
    });
    assert!(w <= MAX_SIDE && h <= MAX_SIDE && u64::from(w) * u64::from(h) <= MAX_PIXELS);
}
#[test]
fn real_windows_vector_and_raster_fixture_has_its_authored_physical_size() {
    for bytes in [
        &include_bytes!("../../../tests/fixtures/emf-import/controlled-spectrum.emf")[..],
        &include_bytes!("../../../tests/fixtures/emf-import/controlled-spectrum-shifted.emf")[..],
    ] {
        let dimensions = validate(bytes).unwrap();
        assert!((dimensions.width_pt * 25.4 / 72. - 100.).abs() < 0.001);
        assert!((dimensions.height_pt * 25.4 / 72. - 60.).abs() < 0.001);
        assert_eq!(preview_size(dimensions), (4724, 2834));
    }
}
#[test]
fn malformed_files_fail_before_native_playback() {
    let valid = file();
    for end in 0..valid.len() {
        assert!(validate(&valid[..end]).is_err());
    }
    for (offset, value) in [
        (4, 84),
        (4, 89),
        (40, 0),
        (48, 104),
        (52, 3),
        (52, 100001),
        (32, 100),
        (60, 10),
        (68, 1),
        (88, 0),
        (88, 1),
        (92, 16),
        (104, 16),
    ] {
        let mut bytes = valid.clone();
        bytes[offset..offset + 4].copy_from_slice(&u32::to_le_bytes(value));
        assert!(
            validate(&bytes).is_err(),
            "accepted offset{offset} value{value}"
        );
    }
}
#[test]
fn empty_palette_offsets_are_ignored_but_entries_cannot_overlap_size_last() {
    let mut bytes = file();
    bytes[100..104].copy_from_slice(&16u32.to_le_bytes());
    assert!(validate(&bytes).is_ok());
    bytes[96..100].copy_from_slice(&1u32.to_le_bytes());
    assert!(validate(&bytes).is_err());
}
#[test]
fn original_description_and_extended_header_arrays_use_actual_field_offsets() {
    let extended = |size: usize| {
        let mut bytes = file();
        bytes.splice(88..88, vec![0; size - 88]);
        bytes[4..8].copy_from_slice(&(size as u32).to_le_bytes());
        let length = bytes.len() as u32;
        bytes[48..52].copy_from_slice(&length.to_le_bytes());
        bytes
    };
    // An original 88-byte header plus a description is not an extended
    // header, even when the whole record is longer than 108 bytes.
    let mut described = extended(112);
    described[60..64].copy_from_slice(&12u32.to_le_bytes());
    described[64..68].copy_from_slice(&88u32.to_le_bytes());
    described[88..112].copy_from_slice(&[
        65, 0, 66, 0, 67, 0, 68, 0, 69, 0, 70, 0, 71, 0, 72, 0, 73, 0, 74, 0, 0, 0, 0, 0,
    ]);
    assert!(validate(&described).is_ok());
    for size in [100, 108] {
        let mut bytes = extended(size);
        // An unused pixel-format offset must not be dereferenced.
        bytes[92..96].copy_from_slice(&16u32.to_le_bytes());
        assert!(validate(&bytes).is_ok());
        bytes[88..92].copy_from_slice(&40u32.to_le_bytes());
        assert!(validate(&bytes).is_err());
        bytes[88..92].copy_from_slice(&0u32.to_le_bytes());
        bytes[96..100].copy_from_slice(&1u32.to_le_bytes());
        assert!(validate(&bytes).is_err());
    }
}
#[test]
fn comment_payloads_and_plus_records_are_bounded() {
    let mut bytes = file();
    bytes.truncate(88);
    bytes.extend(
        [70u32, 28, 16, 0x2b464d45, 0x4002, 12, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes),
    );
    bytes.extend([14u32, 20, 0, 0, 20].into_iter().flat_map(u32::to_le_bytes));
    let len = bytes.len() as u32;
    bytes[48..52].copy_from_slice(&len.to_le_bytes());
    bytes[52..56].copy_from_slice(&3u32.to_le_bytes());
    assert!(validate(&bytes).is_ok());
    bytes[108..112].copy_from_slice(&0u32.to_le_bytes());
    assert!(validate(&bytes).is_err());
}
