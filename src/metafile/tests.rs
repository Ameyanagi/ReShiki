use super::*;

#[test]
fn picture_extensions_start_with_rasters_and_add_emf_only_on_windows() {
    assert!(PICTURE_EXTENSIONS.starts_with(crate::pictures::RASTER_EXTENSIONS));
    assert_eq!(PICTURE_EXTENSIONS.contains(&"emf"), cfg!(windows));
}
