//! Stores the NMR shift index compressed; `nmr.rs` inflates it on first use.
fn main() {
    let source = "../../data/nmr/index.tsv";
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={source}");
    let data = std::fs::read(source).expect("read the embedded data");
    let packed =
        std::path::Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("index.tsv.deflate");
    std::fs::write(packed, miniz_oxide::deflate::compress_to_vec(&data, 10))
        .expect("write the compressed data");
}
