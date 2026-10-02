//! Headless protocol-test helper; production enters through ReShiki's main.
fn main() {
    #[cfg(target_os = "linux")]
    if let Err(error) = reshiki_linux::clipboard_worker() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
