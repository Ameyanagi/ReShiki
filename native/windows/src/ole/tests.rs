use super::*;

#[test]
fn session_file_name_uses_sanitized_host_app() {
    for (app, expected) in [
        (Some("Microsoft PowerPoint"), "Drawing in PowerPoint.rsk"),
        (Some("Microsoft Word"), "Drawing in Word.rsk"),
        (Some("Microsoft Excel"), "Drawing in Excel.rsk"),
        (
            Some("Microsoft  P<>:\"/\\|?*ower\u{0001}\nPoint \t"),
            "Drawing in PowerPoint.rsk",
        ),
        (Some("Microsoft "), "Office drawing.rsk"),
        (Some("<>:\"/\\|?*\n\t"), "Office drawing.rsk"),
        (Some(""), "Office drawing.rsk"),
        (None, "Office drawing.rsk"),
    ] {
        assert_eq!(session_file_name(app, 1), expected, "{app:?}");
    }
}

#[test]
fn later_drawings_from_one_host_are_numbered() {
    for (app, number, expected) in [
        (
            Some("Microsoft PowerPoint"),
            2,
            "Drawing in PowerPoint (2).rsk",
        ),
        (
            Some("Microsoft PowerPoint"),
            3,
            "Drawing in PowerPoint (3).rsk",
        ),
        (None, 2, "Office drawing (2).rsk"),
    ] {
        assert_eq!(session_file_name(app, number), expected, "{app:?}");
    }
}
