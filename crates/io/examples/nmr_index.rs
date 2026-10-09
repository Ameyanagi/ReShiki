//! Offline developer-only transform; never invoked by the app.
use reshiki_io::{
    chemistry::nmr::Nucleus,
    nmr::{self, dataset},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let source = std::fs::read_to_string(
        args.get(1)
            .ok_or("Usage: nmr_index SOURCE.sd OUT.tsv VALIDATION.json")?,
    )?;
    let (observations, audit) = dataset::observations(&source)?;
    let training = dataset::build(&observations, true)?;
    let validation = serde_json::json!({"source_sha256":dataset::SOURCE_SHA256,"encoder":reshiki_io::chemistry::nmr::ENCODER,"conditions":nmr::CONDITIONS,"audit":audit,"split":"SHA256(InChI connectivity block) first hex character byte modulo 5 = 0 held out; duplicates/stereoisomers grouped","validation":[dataset::validate(&observations,&training,Nucleus::H1)?,dataset::validate(&observations,&training,Nucleus::C13)?],"interpretation":"Held-out experimental parent-site group medians from the same source release; not an independent acquisition cohort. Spread is not calibrated uncertainty. The released index uses all accepted molecules after held-out evaluation."});
    std::fs::write(
        args.get(3).ok_or("Missing validation path")?,
        format!("{}\n", serde_json::to_string_pretty(&validation)?),
    )?;
    let full = dataset::build(&observations, false)?;
    std::fs::write(
        args.get(2).ok_or("Missing index path")?,
        dataset::serialize(&full),
    )?;
    println!(
        "{} independent groups; {} observed sites; {} indexed environments",
        audit.independent_connectivity_groups,
        audit.observations,
        full.entries.len()
    );
    Ok(())
}
