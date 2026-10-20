/// Resolve the requested weight before reading advances, bounds or outlines.
/// A variable font's built-in default can be Thin even for a normal CSS request.
/// Static faces retain their existing metrics because they have no weight axis.
pub(super) fn face(bytes: &[u8], index: u32, weight: u16) -> Option<ttf_parser::Face<'_>> {
    let mut face = ttf_parser::Face::parse(bytes, index).ok()?;
    face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), f32::from(weight));
    Some(face)
}

/// The browser has no system fonts. Liberation Sans has Arial's metrics, so
/// its faces also answer to Arial, the default label family.
/// Source and license: licenses/liberation-fonts.
#[cfg(target_arch = "wasm32")]
pub(super) fn load_bundled(db: &mut resvg::usvg::fontdb::Database) {
    use resvg::usvg::fontdb::{Language, Source};
    const FACES: [&[u8]; 4] = [
        include_bytes!("../../fonts/LiberationSans-Regular.ttf"),
        include_bytes!("../../fonts/LiberationSans-Bold.ttf"),
        include_bytes!("../../fonts/LiberationSans-Italic.ttf"),
        include_bytes!("../../fonts/LiberationSans-BoldItalic.ttf"),
    ];
    for bytes in FACES {
        for id in db.load_font_source(Source::Binary(std::sync::Arc::new(bytes))) {
            if let Some(mut face) = db.face(id).cloned() {
                db.remove_face(id);
                face.families
                    .insert(0, ("Arial".into(), Language::English_UnitedStates));
                db.push_face_info(face);
            }
        }
    }
}

#[cfg(test)]
#[path = "font/tests.rs"]
mod tests;
