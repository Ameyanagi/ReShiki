//! Give the browser build the stack the desktop's InChI worker runs on; the
//! wasm default of 1 MiB is too small for the kernel's recursion.
fn main() {
    // Internal crates keep their own versions; the public API follows the app.
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    let manifest = std::fs::read_to_string(
        std::path::Path::new(&std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("../../Cargo.toml"),
    )
    .expect("read the ReShiki release manifest");
    let version = manifest
        .lines()
        .skip_while(|line| line.trim() != "[package]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            if key.trim() != "version" {
                return None;
            }
            value
                .trim()
                .strip_prefix('"')?
                .split_once('"')
                .map(|(version, _)| version)
        })
        .expect("root package.version is a quoted release version");
    println!("cargo:rustc-env=RESHIKI_VERSION={version}");
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo:rustc-link-arg-cdylib=-zstack-size=33554432");
    }
}
