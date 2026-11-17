//! The NMR prediction that Python, the command line and MCP return.
use super::{DATA_ACCESS, Nucleus, Report};
use serde::{Serialize, Serializer};

/// The solvent of the reference data, as scripts name it.
pub const SOLVENT: &str = "CDCl3";

/// One molecule's predicted shifts with what **Export…** writes beside them.
/// IDs are decimal strings on the wire; values are rounded to 9 decimal
/// places, as analyze rounds its properties.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Shifts {
    /// `1H` or `13C`.
    pub nucleus: &'static str,
    pub solvent: &'static str,
    /// One row per site, in drawing order.
    pub rows: Vec<Shift>,
    pub method: String,
    pub data_version: String,
    pub conditions: String,
    pub limitations: String,
    /// The nmrshiftdb2 attribution its licence requires with published values.
    pub credit: String,
    pub data_access: String,
}

/// One site: a carbon for ¹³C, or the H attached to one heavy atom for ¹H.
/// The statistics are `None` when the site has no prediction.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Shift {
    /// The drawing's atom ID; for ¹H, the heavy atom the H are attached to.
    #[serde(serialize_with = "decimal")]
    pub atom: u64,
    pub nucleus: &'static str,
    /// The median of the matched reference group.
    pub ppm: Option<f64>,
    /// The sphere radius of the match.
    pub shell: Option<u8>,
    /// The number of reference molecules.
    pub references: Option<u32>,
    pub sd: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub broad: bool,
    /// The attached H count, for ¹H only.
    pub hydrogen_count: Option<u8>,
    /// The IDs of attached H drawn as atoms; implicit H have none.
    #[serde(serialize_with = "decimals")]
    pub hydrogens: Vec<u64>,
    /// The note **Export…** writes in its Limitations column.
    pub note: Option<String>,
}

fn decimal<S: Serializer>(id: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(id)
}

fn decimals<S: Serializer>(ids: &[u64], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(ids.iter().map(u64::to_string))
}

impl From<Report> for Shifts {
    fn from(report: Report) -> Self {
        let round = |value: f64| (value * 1e9).round() / 1e9 + 0.0;
        let nucleus = report.nucleus;
        let rows = report
            .rows
            .iter()
            .map(|row| {
                let statistics = row.statistics.as_ref();
                let value = |field: fn(&super::Statistics) -> f64| {
                    statistics.map(|statistics| round(field(statistics)))
                };
                Shift {
                    atom: row.atom_id,
                    nucleus: nucleus.code(),
                    ppm: value(|s| s.median),
                    shell: row.radius,
                    references: statistics.map(|s| s.support),
                    sd: value(|s| s.standard_deviation),
                    min: value(|s| s.minimum),
                    max: value(|s| s.maximum),
                    broad: row.broad,
                    hydrogen_count: (nucleus == Nucleus::H1).then_some(row.hydrogen_count),
                    hydrogens: row.hydrogen_ids.clone(),
                    note: row.note(nucleus).map(str::to_owned),
                }
            })
            .collect();
        Self {
            nucleus: nucleus.code(),
            solvent: SOLVENT,
            rows,
            method: report.method,
            data_version: report.data_version,
            conditions: report.conditions,
            limitations: report.limitations,
            credit: report.attribution,
            data_access: DATA_ACCESS.into(),
        }
    }
}
