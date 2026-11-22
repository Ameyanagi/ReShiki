//! A report of placed equipment, with actual cut lengths and endpoint seats.
//! View and illustration scale never change an item's identity or dimensions.
use super::eval::{self, Value, Values};
use super::format::{Definition, HoldKind, TubeLength, TubeMaterial, TubeSpec};
use super::solve::{self, JointRef};
use crate::document::Document;
use serde::Serialize;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartRow {
    pub id: u64,
    /// Actual equipment name and graphic ID, stable across filtering/reordering.
    pub label: String,
    pub definition_id: String,
    pub name: String,
    pub variant: BTreeMap<String, String>,
    pub tube: Option<TubeRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TubeRow {
    pub material: TubeMaterial,
    pub od_mm: f64,
    pub id_mm: Option<f64>,
    pub cut_length_mm: f64,
    pub ends: [TubeEnd; 2],
    pub through: Vec<TubePass>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TubeEnd {
    pub joint: String,
    /// `None` is a free end, not an inferred connection at a line crossing.
    pub connection: Option<PartConnection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartConnection {
    pub id: u64,
    pub label: String,
    pub port: String,
    pub port_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TubePass {
    pub span: String,
    pub fitting: PartConnection,
}

fn name(d: &Definition) -> &str {
    d.names.get("en").map_or(d.id.as_str(), String::as_str)
}

fn label(d: &Definition, id: u64) -> String {
    format!("{} #{id}", name(d))
}

/// One row per selected placed equipment ID, in drawing order. An empty
/// selection reports the whole document; other object kinds are ignored.
/// Connected objects outside the selection are named but not added as rows.
pub fn parts_list(doc: &Document, selection: &[u64]) -> Result<Vec<PartRow>, String> {
    doc.graphics
        .iter()
        .filter(|g| g.piece.is_some() && (selection.is_empty() || selection.contains(&g.id)))
        .map(|g| {
            let d = g
                .piece
                .as_ref()
                .and_then(|p| p.linked.as_ref())
                .ok_or_else(|| format!("Equipment #{} has no linked definition.", g.id))?;
            Ok(PartRow {
                id: g.id,
                label: label(d, g.id),
                definition_id: d.id.clone(),
                name: name(d).to_owned(),
                variant: d.variant.clone(),
                tube: tube_row(doc, g.id)?,
            })
        })
        .collect()
}

/// The physical cut length in local millimetres, including inserted ends.
/// It never uses graphic bounds, world coordinates or the drawing scale.
pub fn cut_length_mm(d: &Definition, values: &Values) -> Result<Option<f64>, String> {
    spec(d).map(|s| length(d, values, &s.length)).transpose()
}

fn length(d: &Definition, values: &Values, length: &TubeLength) -> Result<f64, String> {
    let values = eval::resolve(d, values)?;
    let mut total = length.fixed_mm;
    for id in &length.params {
        let Some(Value::Number(value)) = values.get(id) else {
            return Err(format!("Tube length parameter \"{id}\" is not numeric."));
        };
        total += value;
    }
    if !total.is_finite() || total <= 0. {
        return Err("The tube cut length must be positive and finite.".into());
    }
    Ok(total)
}

fn connection(doc: &Document, at: &JointRef) -> Result<PartConnection, String> {
    let d = doc
        .graphics
        .iter()
        .find(|g| g.id == at.piece)
        .and_then(|g| g.piece.as_ref()?.linked.as_ref())
        .ok_or_else(|| format!("Connected equipment #{} is missing.", at.piece))?;
    Ok(PartConnection {
        id: at.piece,
        label: label(d, at.piece),
        port: at.joint.clone(),
        port_name: match at.joint.as_str() {
            "a" => "End A".into(),
            "b" => "End B".into(),
            id => id.replace(['-', '_'], " "),
        },
    })
}

/// Calculate one placed tube's dimensions, endpoint connections and mechanical
/// passages. Ordinary equipment and unrecognized older tubes return `None`.
pub fn tube_row(doc: &Document, id: u64) -> Result<Option<TubeRow>, String> {
    let Some(g) = doc.graphics.iter().find(|g| g.id == id) else {
        return Ok(None);
    };
    let Some(piece) = &g.piece else {
        return Ok(None);
    };
    let d = piece
        .linked
        .as_ref()
        .ok_or("The tube definition is missing.")?;
    let Some(spec) = spec(d) else { return Ok(None) };
    let endpoint = |joint: &String| -> Result<TubeEnd, String> {
        let at = JointRef {
            piece: id,
            joint: joint.clone(),
        };
        let partner = doc.seats.iter().find_map(|s| {
            if s.a == at {
                Some(&s.b)
            } else if s.b == at {
                Some(&s.a)
            } else {
                None
            }
        });
        Ok(TubeEnd {
            joint: joint.clone(),
            connection: partner.map(|p| connection(doc, p)).transpose()?,
        })
    };
    let ports = solve::ports(g);
    let mut through = Vec::new();
    let mut seen = BTreeSet::new();
    for s in &doc.seats {
        let (mine, other) = if s.a.piece == id {
            (&s.a, &s.b)
        } else if s.b.piece == id {
            (&s.b, &s.a)
        } else {
            continue;
        };
        if !ports
            .iter()
            .any(|p| p.at == *mine && p.hold() == Some(HoldKind::TubeSpan))
        {
            continue;
        }
        let bore = doc
            .graphics
            .iter()
            .find(|g| g.id == other.piece)
            .is_some_and(|g| {
                solve::ports(g)
                    .iter()
                    .any(|p| p.at == *other && p.hold() == Some(HoldKind::ThroughBore))
            });
        if bore && seen.insert(other.piece) {
            through.push(TubePass {
                span: mine.joint.clone(),
                fitting: connection(doc, other)?,
            });
        }
    }
    Ok(Some(TubeRow {
        material: spec.material,
        od_mm: spec.od_mm,
        id_mm: spec.id_mm,
        cut_length_mm: length(d, &piece.params, &spec.length)?,
        ends: [endpoint(&spec.ends[0])?, endpoint(&spec.ends[1])?],
        through,
    }))
}

fn spec(d: &Definition) -> Option<Cow<'_, TubeSpec>> {
    if let Some(spec) = &d.tube_spec {
        return Some(Cow::Borrowed(spec));
    }
    legacy_spec(d).map(Cow::Owned)
}

/// Only the three exact authored revision-1 geometries from #1371 get a
/// dimensional fallback. Names, keywords and variant labels are not parsed.
/// Keeping these snapshots also preserves drawings from before English-only
/// catalogue metadata. Unknown or modified old geometry remains generic.
fn legacy_spec(d: &Definition) -> Option<TubeSpec> {
    if d.revision != 1
        || !matches!(
            d.id.as_str(),
            "reshiki/compression-tube-60-1-4"
                | "reshiki/compression-tube-120-1-4"
                | "reshiki/compression-tube-bend-90-1-4"
        )
    {
        return None;
    }
    static LEGACY: OnceLock<Vec<Definition>> = OnceLock::new();
    let originals = LEGACY.get_or_init(|| {
        [
            include_str!("parts_list_legacy/compression-tube-60-1-4.reshiki-piece"),
            include_str!("parts_list_legacy/compression-tube-120-1-4.reshiki-piece"),
            include_str!("parts_list_legacy/compression-tube-bend-90-1-4.reshiki-piece"),
        ]
        .into_iter()
        .map(|text| super::format::parse(text).expect("archived tube definition"))
        .collect()
    });
    let original = originals.iter().find(|o| o.id == d.id)?;
    if d.shapes != original.shapes
        || d.joints != original.joints
        || d.holds != original.holds
        || d.params != original.params
        || d.parts != original.parts
        || d.tube != original.tube
    {
        return None;
    }
    let fixed_mm = match d.id.as_str() {
        "reshiki/compression-tube-60-1-4" => 60.,
        "reshiki/compression-tube-120-1-4" => 120.,
        _ => 112. + 10. * std::f64::consts::PI,
    };
    Some(TubeSpec {
        material: TubeMaterial::StainlessSteel,
        od_mm: 6.35,
        id_mm: Some(4.57),
        ends: ["a".into(), "b".into()],
        length: TubeLength {
            fixed_mm,
            params: vec![],
        },
    })
}

#[cfg(test)]
#[path = "parts_list_tests.rs"]
mod tests;
