//! Complete drawing graph adapter and separately licensed offline NMR index.
use crate::{chemistry::document, document::Document, editing, reactions};
use reshiki_model::chemistry::nmr::{
    self as core, Environment, Index, Nucleus, Prediction, Statistics,
};
use serde::Serialize;
use std::sync::OnceLock;

pub mod dataset;
pub use core::Nucleus as ObservedNucleus;
pub const DATA_VERSION: &str = "nmrshiftdb2 2026-03-15 · measured CDCl3 subset v1";
pub const CONDITIONS: &str =
    "CDCl3; recorded temperature 273–323 K, pooled without temperature/concentration correction";
pub const ATTRIBUTION: &str = "Contains information from nmrshiftdb2 (https://nmrshiftdb.nmr.uni-koeln.de), available under the nmrshiftdb2 Database License (https://svn.code.sf.net/p/nmrshiftdb2/code/trunk/snapshots/nmrshiftdb2datalicense.txt).";
pub const DATA_ACCESS: &str = "The derivative index and exact transformation method are available at https://github.com/Ameyanagi/ReShiki/tree/main/data/nmr and https://github.com/Ameyanagi/ReShiki/blob/main/crates/io/src/nmr/dataset.rs.";
pub const LIMITATIONS: &str = "Connectivity-only 2D environments do not distinguish E/Z or relative configurations, stereoisomers, or resolve diastereotopic H. Multiple attached H are an unresolved group median, not an assigned peak. Exchangeable H, charged/radical structures, metals and non-default isotope labels are unsupported. Reference spread is descriptive and is not a calibrated confidence interval.";

#[derive(Debug, Clone)]
pub struct Request {
    pub source: Document,
    pub atom_ids: Vec<u64>,
    pub fingerprint: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub atom_id: u64,
    pub hydrogen_ids: Vec<u64>,
    pub hydrogen_count: u8,
    pub radius: Option<u8>,
    pub statistics: Option<Statistics>,
    pub limitation: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub nucleus: Nucleus,
    pub atom_ids: Vec<u64>,
    pub fingerprint: String,
    pub rows: Vec<Row>,
    pub method: String,
    pub data_version: String,
    pub conditions: String,
    pub limitations: String,
    pub attribution: String,
}

pub fn prepare(document: &Document, selection: &[u64]) -> Result<Request, String> {
    document.validate()?;
    let selection = if selection.is_empty() {
        document.atoms.iter().map(|a| a.id).collect::<Vec<_>>()
    } else {
        document.expand_abbreviation_selection(selection)
    };
    let components = reactions::molecules(document, &selection);
    let [atom_ids] = components.as_slice() else {
        return Err("Select atoms in one complete molecular structure for NMR prediction".into());
    };
    if atom_ids.len() > core::MAX_ATOMS {
        return Err("NMR prediction supports at most 128 atoms".into());
    }
    let source = editing::analysis_document(document, atom_ids);
    if source
        .atoms
        .iter()
        .any(|a| a.attachment.is_some() || !a.centroid.is_empty())
    {
        return Err(
            "NMR does not support multi-centre or unresolved abbreviation attachments".into(),
        );
    }
    let fingerprint = fingerprint(document, atom_ids)?;
    Ok(Request {
        source,
        atom_ids: atom_ids.clone(),
        fingerprint,
    })
}

/// A presentation-independent identity for linked results. Position, projection
/// paint, labels, atom-map numbers and collapsed abbreviation visibility are
/// excluded. Stored atom/bond chemistry and stable IDs are retained.
pub fn fingerprint(document: &Document, ids: &[u64]) -> Result<String, String> {
    let wanted: std::collections::BTreeSet<_> = ids.iter().copied().collect();
    let mut atoms: Vec<_> = document
        .atoms
        .iter()
        .filter(|a| wanted.contains(&a.id))
        .collect();
    atoms.sort_by_key(|a| a.id);
    if atoms.len() != wanted.len() {
        return Err("NMR-linked atom was removed".into());
    }
    let atoms: Vec<_> = atoms
        .into_iter()
        .map(|a| {
            (
                &a.id,
                &a.element,
                &a.isotope,
                &a.charge,
                &a.radical_electrons,
                &a.explicit_h,
                &a.no_implicit,
                &a.aromatic,
                &a.stereo,
                &a.attachment,
                &a.centroid,
            )
        })
        .collect();
    let mut bonds: Vec<_> = document
        .bonds
        .iter()
        .filter(|b| wanted.contains(&b.a) || wanted.contains(&b.b))
        .map(|b| {
            let display = if b.projection || b.order == 4 {
                "plain"
            } else if matches!(b.display.as_str(), "wedge" | "hollow_wedge" | "bold") {
                "wedge"
            } else if matches!(b.display.as_str(), "hash" | "hashed") {
                "hash"
            } else if b.display == "wavy" {
                "wavy"
            } else {
                "plain"
            };
            (
                b.a,
                b.b,
                b.order,
                display,
                &b.stereo,
                &b.stereo_atoms,
                b.stereo_authoritative,
            )
        })
        .collect();
    bonds.sort_by_key(|b| (b.0, b.1, b.2));
    let bytes = serde_json::to_vec(&(atoms, bonds))
        .map_err(|e| format!("Could not identify NMR chemistry: {e}"))?;
    Ok(core::identity_digest(&bytes))
}

static BUNDLED: OnceLock<Result<Index, String>> = OnceLock::new();
pub fn bundled_index() -> Result<&'static Index, String> {
    BUNDLED
        .get_or_init(|| parse_index(include_str!("../../../data/nmr/index.tsv")))
        .as_ref()
        .map_err(Clone::clone)
}
pub fn parse_index(text: &str) -> Result<Index, String> {
    if text.len() > 32 * 1024 * 1024 {
        return Err("NMR index exceeds 32 MiB".into());
    }
    let mut index = Index::default();
    for (line_number, line) in text.lines().enumerate() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        let [nucleus, radius, code, support, median, sd, minimum, maximum] = fields.as_slice()
        else {
            return Err(format!("Invalid NMR index row {}", line_number + 1));
        };
        let nucleus = match *nucleus {
            "1H" => Nucleus::H1,
            "13C" => Nucleus::C13,
            _ => return Err("Unsupported NMR index nucleus".into()),
        };
        let radius: u8 = radius.parse().map_err(|_| "Invalid NMR index radius")?;
        if !(core::MIN_RADIUS..=core::MAX_RADIUS).contains(&radius)
            || code.len() != 64
            || !code.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Incompatible NMR index encoding".into());
        }
        let stats = Statistics {
            support: support.parse().map_err(|_| "Invalid NMR support")?,
            median: median.parse().map_err(|_| "Invalid NMR median")?,
            standard_deviation: sd.parse().map_err(|_| "Invalid NMR spread")?,
            minimum: minimum.parse().map_err(|_| "Invalid NMR minimum")?,
            maximum: maximum.parse().map_err(|_| "Invalid NMR maximum")?,
        };
        if !stats.validate()
            || index
                .entries
                .insert((nucleus, radius, code.to_string()), stats)
                .is_some()
        {
            return Err("Invalid or duplicate NMR index entry".into());
        }
    }
    if !text
        .lines()
        .any(|line| line == format!("# encoder={}", core::ENCODER))
        || index.entries.is_empty()
    {
        return Err("NMR index requires a compatible versioned encoder".into());
    }
    Ok(index)
}

pub fn predict(request: &Request, nucleus: Nucleus) -> Result<Report, String> {
    let deadline = Environment::deadline();
    let molecule = document::prepare(&request.source).map_err(|e| e.to_string())?;
    let environment = Environment::new(&molecule.state.graph)?;
    let predictions = bundled_index()?.predict(&environment, nucleus, deadline)?;
    let rows = predictions
        .into_iter()
        .map(
            |Prediction {
                 site,
                 radius,
                 statistics,
                 limitation,
             }| {
                Ok(Row {
                    atom_id: *molecule
                        .ids
                        .get(site.parent)
                        .ok_or("Missing NMR atom identity")?,
                    hydrogen_ids: site
                        .explicit_hydrogens
                        .iter()
                        .map(|&a| molecule.ids.get(a).copied().ok_or("Missing NMR H identity"))
                        .collect::<Result<_, _>>()?,
                    hydrogen_count: site.count,
                    radius,
                    statistics,
                    limitation,
                })
            },
        )
        .collect::<Result<Vec<_>, String>>()?;
    if rows.is_empty() {
        return Err(format!(
            "This molecule has no supported {} sites",
            nucleus.label()
        ));
    }
    Ok(Report {
        nucleus,
        atom_ids: request.atom_ids.clone(),
        fingerprint: request.fingerprint.clone(),
        rows,
        method: core::METHOD.into(),
        data_version: DATA_VERSION.into(),
        conditions: CONDITIONS.into(),
        limitations: LIMITATIONS.into(),
        attribution: ATTRIBUTION.into(),
    })
}

pub fn to_tsv(report: &Report) -> String {
    let mut output = format!(
        "# PREDICTED {} chemical shifts (ppm)\n# {}\n# {}\n# {}\n# {}\n# {}\n# {}\nAtom ID\tH count\tPredicted ppm\tSphere radius\tReference molecules\tObserved SD ppm\tObserved min ppm\tObserved max ppm\tLimitations\n",
        report.nucleus.label(),
        report.method,
        report.data_version,
        report.conditions,
        report.limitations,
        report.attribution,
        DATA_ACCESS
    );
    for row in &report.rows {
        let [median, sd, min, max, support] = row
            .statistics
            .as_ref()
            .map(|s| {
                [
                    format!("{:.3}", s.median),
                    format!("{:.3}", s.standard_deviation),
                    format!("{:.3}", s.minimum),
                    format!("{:.3}", s.maximum),
                    s.support.to_string(),
                ]
            })
            .unwrap_or_default();
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            row.atom_id,
            if report.nucleus == Nucleus::H1 {
                row.hydrogen_count.to_string()
            } else {
                String::new()
            },
            median,
            row.radius.map(|r| r.to_string()).unwrap_or_default(),
            support,
            sd,
            min,
            max,
            row.limitation.as_deref().unwrap_or(
                if report.nucleus == Nucleus::H1 && row.hydrogen_count > 1 {
                    "Unresolved attached-H group; not a stereospecific peak assignment"
                } else {
                    ""
                }
            )
        ));
    }
    output
}

#[cfg(test)]
mod tests;
