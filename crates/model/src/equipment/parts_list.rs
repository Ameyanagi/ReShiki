//! A report of placed equipment, with actual cut lengths and endpoint seats.
//! View and illustration scale never change an item's identity or dimensions.
use super::eval::{self, Value, Values};
use super::format::{Definition, HoldKind, Param, TubeLength, TubeMaterial, TubeSpec};
use super::solve::{self, JointRef};
use crate::{document::Document, graphics::Graphic};
use serde::Serialize;
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
    /// Flexible assemblies report connections without turning a drawn route
    /// into a fabricated physical cut length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hose: Option<HoseRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HoseRow {
    pub interfaces: [String; 2],
    pub ends: [TubeEnd; 2],
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
                hose: hose_row(doc, g)?,
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

fn endpoint(doc: &Document, id: u64, joint: &str) -> Result<TubeEnd, String> {
    let at = JointRef {
        piece: id,
        joint: joint.to_owned(),
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
        joint: joint.to_owned(),
        connection: partner.map(|p| connection(doc, p)).transpose()?,
    })
}

fn hose_row(doc: &Document, g: &Graphic) -> Result<Option<HoseRow>, String> {
    let id = g.id;
    let Some(d) = g.piece.as_ref().and_then(|p| p.linked.as_ref()) else {
        return Ok(None);
    };
    let Some(hose) = &d.tube else { return Ok(None) };
    let ports = solve::ports(g);
    let interface = |joint: &str| -> String {
        ports
            .iter()
            .find(|p| p.at.joint == joint)
            .map_or_else(String::new, |p| {
                let size = match &p.kind {
                    solve::Kind::Plug(super::format::PlugFamily::Hose, size) => {
                        format!("Ø{size} mm hose")
                    }
                    _ => p.size(),
                };
                format!(
                    "{} {}",
                    size,
                    if p.gender == super::format::Gender::M {
                        "male"
                    } else {
                        "female"
                    }
                )
            })
    };
    Ok(Some(HoseRow {
        interfaces: [interface(&hose.ends[0]), interface(&hose.ends[1])],
        ends: [
            endpoint(doc, id, &hose.ends[0])?,
            endpoint(doc, id, &hose.ends[1])?,
        ],
    }))
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
        ends: [
            endpoint(doc, id, &spec.ends[0])?,
            endpoint(doc, id, &spec.ends[1])?,
        ],
        through,
    }))
}

fn spec(d: &Definition) -> Option<&TubeSpec> {
    d.tube_spec.as_ref().or_else(|| legacy_spec(d))
}

#[derive(serde::Deserialize)]
struct ArchivedTube {
    definition: Definition,
    spec: TubeSpec,
}

fn archived_tubes() -> &'static [ArchivedTube] {
    static LEGACY: OnceLock<Vec<ArchivedTube>> = OnceLock::new();
    LEGACY.get_or_init(|| {
        serde_json::from_str(include_str!("parts_list_legacy/tubes.json")).unwrap_or_default()
    })
}

/// Pinned stock tubes keep their original definitions. Recognize archived
/// geometry and ports without inferring physical dimensions from labels.
fn legacy_spec(d: &Definition) -> Option<&'static TubeSpec> {
    if !(d.id.starts_with("reshiki/compression-tube-") || d.id == "reshiki/quartz-work-tube-600-32")
    {
        return None;
    }
    let archived = archived_tubes().iter().find(|tube| {
        let old = &tube.definition;
        old.id == d.id
            && old.revision == d.revision
            && old.shapes == d.shapes
            && old.joints == d.joints
            && old.holds == d.holds
            && old.parts == d.parts
            && old.tube == d.tube
            && same_view(&old.params, &d.params)
    })?;
    Some(&archived.spec)
}

fn same_view(old: &[Param], current: &[Param]) -> bool {
    // A stock tube only has View. Its localized labels may have changed
    // without a geometry revision.
    let (
        [
            Param::States {
                id: a,
                states: sa,
                default: da,
                ..
            },
        ],
        [
            Param::States {
                id: b,
                states: sb,
                default: db,
                ..
            },
        ],
    ) = (old, current)
    else {
        return false;
    };
    a == b && da == db && sa.iter().map(|s| &s.id).eq(sb.iter().map(|s| &s.id))
}

#[cfg(test)]
#[path = "parts_list_tests.rs"]
mod tests;
