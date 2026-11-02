/// Keep bundled-font registration out of other tests' global font state.
pub(crate) fn with_fonts(test: impl FnOnce()) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        const CHILD: &str = "RESHIKI_STEREO_DESCRIPTOR_FONT_TEST";
        if std::env::var_os(CHILD).is_none() {
            let name = std::thread::current().name().unwrap().to_owned();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", &name, "--nocapture"])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
    }
    for bytes in [
        include_bytes!("../../fonts/LiberationSans-Regular.ttf").as_slice(),
        include_bytes!("../../fonts/LiberationSans-Italic.ttf").as_slice(),
    ] {
        crate::style::add_font(bytes.to_vec()).unwrap();
    }
    test();
}
