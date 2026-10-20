//! Give the browser build the stack the desktop's InChI worker runs on; the
//! wasm default of 1 MiB is too small for the kernel's recursion.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo:rustc-link-arg-cdylib=-zstack-size=33554432");
    }
}
