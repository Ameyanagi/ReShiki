//! Reproducible measured-only nmrshiftdb2 SDF transformation. No legacy
//! predictor code is used. The source and derived data have a separate license.
use super::*;
use crate::chemistry::molfile;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

pub const SOURCE_SHA256: &str = "0e86688360e23c88ccf0eb82a1251315fa57ec6f5b376dc8886f3311793a0afe";
pub const SOURCE_URL: &str =
    "https://downloads.sourceforge.net/project/nmrshiftdb2/data/nmrshiftdb2withsignals.sd";
const CALCULATION_FIELDS: [&str; 8] = [
    "Program",
    "NMRProgram",
    "NMRMethod",
    "NMRBasisSet",
    "GeomMethod",
    "GeomBasisSet",
    "NMRStandard",
    "NMRLocalis",
];

#[derive(Debug, Clone, Serialize)]
pub struct Observation {
    pub molecule_id: String,
    /// Connectivity grouping prevents duplicate structures/stereoisomers leaking.
    pub connectivity: String,
    pub nucleus: Nucleus,
    pub parent: usize,
    pub ppm: f64,
    pub codes: Vec<(u8, String)>,
}
#[derive(Debug, Default, Serialize)]
pub struct Audit {
    pub records: usize,
    pub selected_spectra: usize,
    pub rejected_calculated: usize,
    pub rejected_conditions: usize,
    pub rejected_chemistry: usize,
    pub rejected_assignments: usize,
    pub independent_connectivity_groups: usize,
    pub observations: usize,
}

fn fields(record: &str) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    let mut key: Option<String> = None;
    let mut value = String::new();
    for line in record.lines() {
        if line.starts_with('>') {
            if let Some(k) = key.take() {
                result.insert(k, value.trim().to_string());
                value.clear();
            }
            key = line
                .find('<')
                .zip(line.rfind('>'))
                .and_then(|(a, b)| line.get(a + 1..b))
                .map(str::to_string);
        } else if key.is_some() {
            value.push_str(line);
            value.push('\n');
        }
    }
    if let Some(k) = key {
        result.insert(k, value.trim().to_string());
    }
    result
}
/// Exporter metadata uses space-separated `spectrumIndex:value` segments.
/// Parse only numbered boundaries so conditions cannot bleed into another row.
fn numbered(text: &str, index: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut positions = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b':' {
            let mut start = i;
            while start > 0 && bytes.get(start - 1).is_some_and(u8::is_ascii_digit) {
                start -= 1;
            }
            if start < i
                && (start == 0 || bytes.get(start - 1).is_some_and(u8::is_ascii_whitespace))
            {
                positions.push((start, i));
            }
        }
    }
    for (n, &(start, colon)) in positions.iter().enumerate() {
        if text.get(start..colon) == Some(index) {
            let end = positions.get(n + 1).map(|x| x.0).unwrap_or(text.len());
            return text.get(colon + 1..end).map(|s| s.trim().to_string());
        }
    }
    None
}

pub fn observations(source: &str) -> Result<(Vec<Observation>, Audit), String> {
    if core::identity_digest(source.as_bytes()) != SOURCE_SHA256 {
        return Err("NMR source does not match the pinned 2026-03-15 SHA-256; refusing a silent dataset change".into());
    }
    let mut audit = Audit::default();
    let mut output = Vec::new();
    for record in source.split("$$$$") {
        if record.trim().is_empty() {
            continue;
        }
        audit.records += 1;
        let fields = fields(record);
        let mut spectra = Vec::new();
        for (key, shifts) in &fields {
            let words: Vec<_> = key.split_whitespace().collect();
            let ["Spectrum", nucleus, index] = words.as_slice() else {
                continue;
            };
            let nucleus = match *nucleus {
                "1H" => Nucleus::H1,
                "13C" => Nucleus::C13,
                _ => continue,
            };
            if CALCULATION_FIELDS
                .iter()
                .any(|f| fields.get(*f).and_then(|s| numbered(s, index)).is_some())
            {
                audit.rejected_calculated += 1;
                continue;
            }
            let solvent = fields.get("Solvent").and_then(|s| numbered(s, index));
            let temperature = fields
                .get("Temperature [K]")
                .and_then(|s| numbered(s, index))
                .and_then(|s| s.parse::<f64>().ok());
            if solvent.as_deref() != Some("Chloroform-D1 (CDCl3)")
                || !temperature.is_some_and(|t| (273.0..=323.0).contains(&t))
            {
                audit.rejected_conditions += 1;
                continue;
            }
            spectra.push((nucleus, shifts));
        }
        if spectra.is_empty() {
            continue;
        }
        let Some(connectivity) = fields
            .get("INChI key")
            .and_then(|s| s.split('-').next())
            .filter(|s| s.len() == 14)
            .map(str::to_string)
        else {
            audit.rejected_chemistry += 1;
            continue;
        };
        let molecule_id = fields
            .get("nmrshiftdb2 ID")
            .cloned()
            .ok_or("Missing source molecule ID")?;
        let lines: Vec<_> = record.lines().collect();
        let Some(counts) = lines
            .iter()
            .position(|s| s.contains("V2000") || s.contains("V3000"))
        else {
            audit.rejected_chemistry += 1;
            continue;
        };
        let Some(start) = counts.checked_sub(3) else {
            audit.rejected_chemistry += 1;
            continue;
        };
        let Some(end) = lines.iter().position(|s| *s == "M  END") else {
            audit.rejected_chemistry += 1;
            continue;
        };
        let Some(mol_lines) = lines.get(start..=end) else {
            audit.rejected_chemistry += 1;
            continue;
        };
        let imported = match molfile::read(&mol_lines.join("\n")) {
            Ok(x) => x,
            Err(_) => {
                audit.rejected_chemistry += 1;
                continue;
            }
        };
        let environment = match Environment::new(&imported.molecule.state.graph) {
            Ok(x) => x,
            Err(_) => {
                audit.rejected_chemistry += 1;
                continue;
            }
        };
        for (nucleus, shifts) in spectra {
            let sites = environment.sites(nucleus)?;
            let mut values: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
            let mut invalid = false;
            for assignment in shifts.split('|').filter(|s| !s.trim().is_empty()) {
                let columns: Vec<_> = assignment.split(';').collect();
                let [ppm, _, atom] = columns.as_slice() else {
                    invalid = true;
                    break;
                };
                let Some((ppm, atom)) = ppm.parse::<f64>().ok().zip(atom.parse::<usize>().ok())
                else {
                    invalid = true;
                    break;
                };
                let range = if nucleus == Nucleus::H1 {
                    -5.0..=25.0
                } else {
                    -100.0..=300.0
                };
                if !range.contains(&ppm) {
                    invalid = true;
                    break;
                }
                let Some(site) = sites.iter().find(|s| {
                    s.parent == atom
                        || nucleus == Nucleus::H1 && s.explicit_hydrogens.contains(&atom)
                }) else {
                    invalid = true;
                    break;
                };
                if !site.exchangeable {
                    values.entry(site.parent).or_default().push(ppm);
                }
            }
            if invalid {
                audit.rejected_assignments += 1;
                continue;
            }
            audit.selected_spectra += 1;
            for site in sites.iter().filter(|s| !s.exchangeable) {
                let Some(values) = values.get(&site.parent) else {
                    continue;
                };
                let ppm = Statistics::from_values(values.clone())?.median;
                let deadline = Instant::now() + Duration::from_secs(3);
                let codes = (core::MIN_RADIUS..=core::MAX_RADIUS)
                    .map(|r| Ok((r, environment.code(site, nucleus, r, deadline)?)))
                    .collect::<Result<_, String>>()?;
                output.push(Observation {
                    molecule_id: molecule_id.clone(),
                    connectivity: connectivity.clone(),
                    nucleus,
                    parent: site.parent,
                    ppm,
                    codes,
                });
            }
        }
    }
    audit.independent_connectivity_groups = output
        .iter()
        .map(|o| &o.connectivity)
        .collect::<BTreeSet<_>>()
        .len();
    audit.observations = output.len();
    Ok((output, audit))
}

pub fn held_out(connectivity: &str) -> bool {
    core::identity_digest(connectivity.as_bytes())
        .bytes()
        .next()
        .is_some_and(|b| b % 5 == 0)
}
/// One value per connectivity/environment, including symmetrical sites, repeated
/// spectra and stereoisomer records. This is the independent support count.
pub fn build(observations: &[Observation], training_only: bool) -> Result<Index, String> {
    let mut grouped: BTreeMap<(Nucleus, u8, String, String), Vec<f64>> = BTreeMap::new();
    for o in observations
        .iter()
        .filter(|o| !training_only || !held_out(&o.connectivity))
    {
        for (radius, code) in &o.codes {
            grouped
                .entry((o.nucleus, *radius, code.clone(), o.connectivity.clone()))
                .or_default()
                .push(o.ppm);
        }
    }
    let mut entries: BTreeMap<(Nucleus, u8, String), Vec<f64>> = BTreeMap::new();
    for ((nucleus, radius, code, _), values) in grouped {
        entries
            .entry((nucleus, radius, code))
            .or_default()
            .push(Statistics::from_values(values)?.median);
    }
    let mut index = Index::default();
    for (key, values) in entries {
        let stats = Statistics::from_values(values)?;
        if stats.support >= core::MIN_SUPPORT {
            index.entries.insert(key, stats);
        }
    }
    Ok(index)
}
pub fn serialize(index: &Index) -> String {
    let mut out = format!(
        "# encoder={}\n# source-sha256={SOURCE_SHA256}\n# data-version={DATA_VERSION}\n# conditions={CONDITIONS}\n# license=nmrshiftdb2 Database License; see licenses/nmrshiftdb2/DATABASE-LICENSE.txt\n# {ATTRIBUTION}\n# nucleus\tradius\tcode-sha256\tindependent-molecules\tmedian-ppm\tsample-sd-ppm\tmin-ppm\tmax-ppm\n",
        core::ENCODER
    );
    for ((nucleus, radius, code), s) in &index.entries {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            nucleus.code(),
            radius,
            code,
            s.support,
            s.median,
            s.standard_deviation,
            s.minimum,
            s.maximum
        ));
    }
    out
}

#[derive(Debug, Serialize)]
pub struct Validation {
    pub nucleus: Nucleus,
    pub molecules: usize,
    pub target_groups: usize,
    pub predicted_groups: usize,
    pub coverage: f64,
    pub mae: Option<f64>,
    pub median_absolute_error: Option<f64>,
    pub rmse: Option<f64>,
    pub radius_counts: BTreeMap<u8, usize>,
}
pub fn validate(
    observations: &[Observation],
    index: &Index,
    nucleus: Nucleus,
) -> Result<Validation, String> {
    let mut grouped: BTreeMap<(String, u8, String), Vec<&Observation>> = BTreeMap::new();
    for o in observations
        .iter()
        .filter(|o| o.nucleus == nucleus && held_out(&o.connectivity))
    {
        let Some((radius, code)) = o.codes.last() else {
            continue;
        };
        grouped
            .entry((o.connectivity.clone(), *radius, code.clone()))
            .or_default()
            .push(o);
    }
    let mut errors = Vec::new();
    let mut radius_counts = BTreeMap::new();
    let mut molecules = BTreeSet::new();
    for ((connectivity, _, _), samples) in &grouped {
        molecules.insert(connectivity);
        let target = Statistics::from_values(samples.iter().map(|o| o.ppm).collect())?.median;
        let sample = samples.first().ok_or("Missing validation target")?;
        for (radius, code) in sample.codes.iter().rev() {
            if let Some(s) = index
                .entries
                .get(&(nucleus, *radius, code.clone()))
                .filter(|s| s.support >= core::MIN_SUPPORT)
            {
                errors.push((s.median - target).abs());
                *radius_counts.entry(*radius).or_default() += 1;
                break;
            }
        }
    }
    let predicted_groups = errors.len();
    let target_groups = grouped.len();
    let mae = if errors.is_empty() {
        None
    } else {
        Some(errors.iter().sum::<f64>() / errors.len() as f64)
    };
    let rmse = if errors.is_empty() {
        None
    } else {
        Some((errors.iter().map(|e| e * e).sum::<f64>() / errors.len() as f64).sqrt())
    };
    let median_absolute_error = if errors.is_empty() {
        None
    } else {
        Some(Statistics::from_values(errors)?.median)
    };
    Ok(Validation {
        nucleus,
        molecules: molecules.len(),
        target_groups,
        predicted_groups,
        coverage: if target_groups == 0 {
            0.
        } else {
            predicted_groups as f64 / target_groups as f64
        },
        mae,
        median_absolute_error,
        rmse,
        radius_counts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn indexed_metadata_cannot_bleed_between_spectra() {
        assert_eq!(
            numbered(
                "0:Chloroform-D1 (CDCl3) 10:Unreported 1:Benzene-D6 (C6D6)",
                "1"
            )
            .as_deref(),
            Some("Benzene-D6 (C6D6)")
        );
        assert_eq!(
            numbered("1: 2:Quantum ESPRESSO 3:", "1"),
            Some(String::new())
        );
        assert!(numbered("2:ACD/Labs 2020.1.0", "0").is_none());
    }
    #[test]
    fn duplicate_spectra_and_equivalent_atoms_do_not_inflate_support() {
        let codes = vec![(2, "a".into())];
        let o = |id: &str, ppm| Observation {
            molecule_id: id.into(),
            connectivity: id.into(),
            nucleus: Nucleus::H1,
            parent: 0,
            ppm,
            codes: codes.clone(),
        };
        let index = build(&[o("a", 1.), o("a", 1.), o("a", 3.), o("b", 5.)], false).unwrap();
        let s = index.entries.values().next().unwrap();
        assert_eq!(s.support, 2);
        assert_eq!(s.median, 3.);
    }
    #[test]
    fn retained_source_fixture_separates_calculation_metadata_from_observations() {
        let fixture = include_str!("../../../../tests/fixtures/nmr/mixed-measured-calculated.sd");
        let metadata = fields(fixture);
        assert!(numbered(metadata.get("Program").unwrap(), "1").is_some());
        assert!(numbered(metadata.get("Program").unwrap(), "2").is_some());
        assert!(numbered(metadata.get("Program").unwrap(), "4").is_none());
        assert_eq!(
            numbered(metadata.get("Solvent").unwrap(), "4").as_deref(),
            Some("Chloroform-D1 (CDCl3)")
        );
        assert_eq!(
            numbered(metadata.get("Temperature [K]").unwrap(), "4").as_deref(),
            Some("298")
        );
        assert!(metadata.contains_key("Spectrum 1H 4"));
    }
}
