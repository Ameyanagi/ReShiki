//! Give the naming worker the desktop naming process's 8 MiB stack, and cap
//! its memory at 128 MiB: the desktop's 96 MiB heap budget, that stack and
//! the OPSIN tables, with room for the allocator's own overhead.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo:rustc-link-arg-cdylib=-zstack-size=8388608");
        println!("cargo:rustc-link-arg-cdylib=--max-memory=134217728");
    }
}
