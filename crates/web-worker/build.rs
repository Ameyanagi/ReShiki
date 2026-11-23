//! Export jobs use the editor's 32 MiB stack and a bounded 1.5 GiB heap,
//! including full-size raster buffers. Naming retains its separate 96 MiB
//! heap budget. The memory maximum includes the stack, static data and heap.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo:rustc-link-arg-cdylib=-zstack-size=33554432");
        println!("cargo:rustc-link-arg-cdylib=--max-memory=2147483648");
    }
}
