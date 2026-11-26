#![forbid(unsafe_code)]

#[global_allocator]
static ALLOCATOR: reshiki_process_heap::BoundedHeap = reshiki_process_heap::BoundedHeap;

fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let argument = arguments.next();
    if arguments.next().is_none() {
        if argument.as_deref() == Some(std::ffi::OsStr::new("--xtb-worker")) {
            std::process::exit(reshiki_xtb_worker::serve());
        }
        if argument.as_deref() == Some(std::ffi::OsStr::new("--version")) {
            println!(
                "reshiki-xtb-worker {} (axtb {}, LGPL-3.0-or-later, source {}/tree/{})",
                env!("CARGO_PKG_VERSION"),
                reshiki_xtb_worker::AXTB_VERSION,
                reshiki_xtb_worker::AXTB_SOURCE,
                reshiki_xtb_worker::AXTB_REV,
            );
            return;
        }
    }
    eprintln!("This is the xTB worker of the reshiki Python package, started by reshiki.xtb.");
    eprintln!("Usage: reshiki-xtb-worker --xtb-worker | --version");
    std::process::exit(2);
}
