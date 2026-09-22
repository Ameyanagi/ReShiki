//! Stream independently captured test fixtures without a reference interpreter.
use anyhow::Context;
use flate2::read::GzDecoder;
use std::{fs::File, io::BufReader, path::Path};

pub fn open(name: &str) -> anyhow::Result<BufReader<GzDecoder<File>>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let source = File::open(&path)
        .with_context(|| format!("Open independent fixture {}", path.display()))?;
    Ok(BufReader::new(GzDecoder::new(source)))
}
