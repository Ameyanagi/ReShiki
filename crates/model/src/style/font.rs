/// Resolve the requested weight before reading advances, bounds or outlines.
/// A variable font's built-in default can be Thin even for a normal CSS request.
/// Static faces retain their existing metrics because they have no weight axis.
pub(super) fn face(bytes: &[u8], index: u32, weight: u16) -> Option<ttf_parser::Face<'_>> {
    let mut face = ttf_parser::Face::parse(bytes, index).ok()?;
    face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), f32::from(weight));
    Some(face)
}

#[cfg(test)]
#[path = "font/tests.rs"]
mod tests;
