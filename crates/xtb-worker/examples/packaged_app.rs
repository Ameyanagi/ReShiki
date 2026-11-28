//! Verify a supplied shipped application without depending on or rebuilding it.
#[cfg(not(target_arch = "wasm32"))]
#[path = "../../../tests/common/xtb_packaged.rs"]
mod packaged;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let source = std::env::var_os("RESHIKI_TEST_PACKAGED_APP")
        .expect("RESHIKI_TEST_PACKAGED_APP must name the application staged from its ZIP");
    packaged::check_relocated_app(&std::path::PathBuf::from(source));
    println!(
        "Verified the ZIP application with GFN2-xTB, g-xTB, GFN-FF, optimization and frequencies."
    );
}

#[cfg(target_arch = "wasm32")]
fn main() {
    panic!("the packaged application check requires a native host");
}
