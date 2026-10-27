fn main() {
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let manifest = std::fs::read_to_string(root).expect("read application version");
    let app_version = manifest
        .lines()
        .skip_while(|line| line.trim() != "[package]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "version").then(|| value.trim().trim_matches('"'))
        })
        .expect("quoted application version");
    assert_eq!(
        app_version,
        env!("CARGO_PKG_VERSION"),
        "workspace.package.version must match the ReShiki application version"
    );
}
