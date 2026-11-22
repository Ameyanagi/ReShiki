//! Plain Wasm embedding regression; no wasm-bindgen glue or async runtime.
//! Build with `cargo build -p reshiki-io --example wasm_import_smoke
//! --target wasm32-unknown-unknown --profile dev`, then run
//! `node scripts/check_wasm_import.mjs PATH/wasm_import_smoke.wasm`.
use reshiki_io::{engine::LocalEngine, formats};
use std::{
    future::Future,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

static PANIC: Mutex<Option<String>> = Mutex::new(None);

fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut context = Context::from_waker(Waker::noop());
    match std::pin::pin!(future).as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("In-process Wasm import unexpectedly needs an async host"),
    }
}

fn import(text: &str, format: &str, atoms: usize, bonds: usize) -> u32 {
    std::panic::set_hook(Box::new(|info| {
        *PANIC.lock().unwrap() = Some(info.to_string());
    }));
    let document = ready(formats::read_with_engine(
        text.as_bytes().to_vec(),
        format,
        &LocalEngine::in_process(),
    ))
    .expect("import completes");
    document.validate().expect("valid imported drawing");
    assert_eq!(document.atoms.len(), atoms);
    assert_eq!(document.bonds.len(), bonds);
    // Successful parsing alone is insufficient: the complete layout must
    // contain finite, distinct atom positions as a native import does.
    for (i, atom) in document.atoms.iter().enumerate() {
        assert!(atom.position.is_finite());
        assert!(
            document
                .atoms
                .iter()
                .skip(i + 1)
                .all(|other| atom.position.distance(other.position) > 0.01)
        );
    }
    atoms as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn parse_benzene() -> u32 {
    reshiki_io::chemistry::smiles::parse_input_for_import("c1ccccc1")
        .expect("benzene parses")
        .graph
        .atoms
        .len() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn import_benzene() -> u32 {
    import("c1ccccc1", "smiles", 6, 6)
}

#[unsafe(no_mangle)]
pub extern "C" fn import_fused_rings() -> u32 {
    import("c1ccc2ccccc2c1", "smiles", 10, 11)
}

#[unsafe(no_mangle)]
pub extern "C" fn import_mol() -> u32 {
    import(
        "benzene\n  ReShiki\n\n  6  6  0  0  0  0            999 V2000\n    1.5000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0\n    0.7500    1.2990    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0\n   -0.7500    1.2990    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0\n   -1.5000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0\n   -0.7500   -1.2990    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0\n    0.7500   -1.2990    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0\n  1  2  2  0  0  0  0\n  2  3  1  0  0  0  0\n  3  4  2  0  0  0  0\n  4  5  1  0  0  0  0\n  5  6  2  0  0  0  0\n  6  1  1  0  0  0  0\nM  END\n",
        "mol",
        6,
        6,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn import_cdxml() -> u32 {
    import(
        "<CDXML><page><fragment id='1'><n id='2' p='0 0'/><n id='3' p='30 0' Element='8'/><b id='4' B='2' E='3' Order='1'/></fragment></page></CDXML>",
        "cdxml",
        2,
        1,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn panic_len() -> u32 {
    PANIC
        .lock()
        .unwrap()
        .as_ref()
        .map_or(0, |message| message.len() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn panic_byte(index: u32) -> u32 {
    PANIC
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|message| message.as_bytes().get(index as usize))
        .copied()
        .map_or(0, u32::from)
}
