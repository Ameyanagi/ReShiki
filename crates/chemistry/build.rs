//! Stores the layout template catalog compressed; `depict/templates/catalog.rs` inflates it on first use.
fn main() {
    let source = "src/depict/templates/builtin.json";
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={source}");
    let data = std::fs::read(source).expect("read the embedded data");
    let packed =
        std::path::Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("builtin.json.deflate");
    std::fs::write(packed, miniz_oxide::deflate::compress_to_vec(&data, 10))
        .expect("write the compressed data");
}
