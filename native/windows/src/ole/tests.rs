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
        assert_eq!(session_file_name(app), expected, "{app:?}");
    }
}
