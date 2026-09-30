//! Dependency-free transport fault injector; never shipped with the app.
use std::{io::{self, Read, Write}, path::Path, time::Duration};
const IMPORTED: &str = r#"{"version":"1.07.5","result":{"Ok":{"Imported":{"status":0,"message":"","log":"Input format: InChI (plain identifier)\nOutput format: Plain text\nAux. info suppressed\nNo timeout\nUp to 1024 atoms per structure","state":{"graph":{"atoms":[{"atomic_number":6,"isotope":0,"charge":0,"explicit_hydrogens":4,"no_implicit":true,"aromatic":false,"radical_electrons":0}],"bonds":[]},"metadata":{"atoms":[{"map_number":0,"map_present":false,"chiral_tag":0,"chiral_permutation":null,"ring_stereo":false,"non_stereo_rank":0}],"bonds":[],"groups":[]},"directions":[],"valences":[{"explicit_valence":4,"implicit_hydrogens":0}],"conjugated":[],"hybridizations":["SP3"],"rings":{"kind":"symmetric","atoms":[]},"properties":{"atoms":[{"cip_code":null,"cip_rank":null,"possible":null,"ring_candidate":null,"ring_members":null,"unknown":false}],"bond_codes":[],"done":true,"needs_detection":null}},"unspecified_bonds":[],"diagnostics":[]}}}}"#;
const GENERATED: &str = r#"{"version":"1.07.5","result":{"Ok":{"Generated":{"status":0,"inchi":"InChI=1S/CH4/h1H4","message":"","log":"","auxiliary":"AuxInfo=1/0/N:1/rA:1C/rB:/rC:;","diagnostics":[]}}}}"#;
fn sleep() -> ! { loop { std::thread::sleep(Duration::from_secs(1)); } }
fn main() {
    let exe = std::env::current_exe().unwrap();
    let mode = exe.file_stem().unwrap().to_string_lossy();
    std::fs::write(exe.parent().unwrap_or(Path::new("." )).join("pid"), std::process::id().to_string()).unwrap();
    if mode == "no-read" { sleep(); }
    let mut request = Vec::new();
    io::stdin().read_to_end(&mut request).unwrap();
    if mode == "hang" { sleep(); }
    if mode == "exit" { std::process::exit(17); }
    if mode == "oversized" { io::stdout().write_all(&vec![b'x';9*1024*1024]).unwrap(); sleep(); }
    if mode == "stderr" { io::stderr().write_all(&vec![b'x';65*1024]).unwrap(); sleep(); }
    if mode.starts_with("resource") {
        let marker = match mode.as_ref() {
            "resource" => "RESHIKI_HEAP_LIMIT 67108864 8 67108864",
            "resource-budget" => "RESHIKI_HEAP_LIMIT 0 0 1",
            "resource-used" => "RESHIKI_HEAP_LIMIT 1024 1025 1",
            "resource-truncated" => "RESHIKI_HEAP_LIMIT 1024 0",
            _ => "RESHIKI_HEAP_LIMIT invalid",
        };
        eprintln!("{marker}"); std::process::exit(75);
    }
    let mut json = if mode.starts_with("read-") { IMPORTED } else { GENERATED }.to_owned();
    match mode.as_ref() {
        "version" => json = json.replace("1.07.5", "wrong"),
        "rejected" => json = r#"{"version":"1.07.5","result":{"Err":"Rejected test request"}}"#.into(),
        "status" | "read-status" => json = json.replace("\"status\":0", "\"status\":99"),
        "nonstandard" => json = json.replace("InChI=1S/", "InChI=1/"),
        "string-length" => json = json.replace("InChI=1S/CH4/h1H4", &"x".repeat(2*1024*1024+1)),
        "read-counts" => json = json.replace("\"directions\":[]", "\"directions\":[\"none\"]"),
        "read-stereo" => json = json.replace("\"chiral_tag\":0", "\"chiral_tag\":99"),
        "read-element" => json = json.replace("\"atomic_number\":6", "\"atomic_number\":255"),
        "read-index" => json = json.replace("\"bonds\":[]", "\"bonds\":[{\"a\":0,\"b\":9,\"order\":1,\"aromatic\":false}]"),
        _ => {},
    }
    let mut frame = b"RSHINCHI\x03\x00\x00\x00".to_vec();
    frame.extend_from_slice(&(json.len() as u32).to_le_bytes());
    frame.extend_from_slice(json.as_bytes());
    match mode.as_ref() {
        "protocol" => frame[8] = 99,
        "truncated" | "read-truncated" => { frame.pop(); },
        "trailing" | "read-trailing" => frame.push(b'x'),
        "utf8" => frame[16] = 255,
        _ => {},
    }
    io::stdout().write_all(&frame).unwrap();
}
