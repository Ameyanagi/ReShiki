//! Replace a selected endpoint using fixed native template geometry. Chemistry
//! preparation and drawing reconstruction remain Rust calculations; the final
//! whole-document check and full CIP labels are the caller's responsibility.
use super::{Preset, check_size, presets, validate};
#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
use crate::chemistry::windows_trigonometry as trigonometry;
use crate::{
    abbreviations::Abbreviation,
    chemistry::{
        RDKIT_VERSION,
        document::{self, BondLabel, Labels, Molecule},
        graph::Graph,
        kekulize::Direction,
        ranking::Metadata,
        sanitize, smiles,
        stereo::{Point3, perception},
    },
    document::Document,
};

use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Choose a defined abbreviation")]
    Label,
    #[error("Select one terminal atom or one abbreviation to replace")]
    Selection,
    #[error("Select an endpoint with a single bond, or an isolated atom")]
    Attachment,
    #[error("The attachment bond has invalid geometry")]
    Geometry,
    #[error("Object ID limit exceeded")]
    Identity,
    #[error("Invalid abbreviation definition: {0}")]
    Definition(String),
    #[error(transparent)]
    Abbreviation(#[from] super::Error),
    #[error(transparent)]
    Smiles(#[from] smiles::Error),
    #[error(transparent)]
    Sanitization(#[from] sanitize::Error),
    #[error(transparent)]
    Drawing(#[from] document::Error),
}
type Result<T> = std::result::Result<T, Error>;
fn at<T>(items: &[T], index: usize) -> Result<&T> {
    items
        .get(index)
        .ok_or_else(|| Error::Definition("Missing template item".into()))
}

/// Two-coordinate specialization of CPython's compensated vector_norm. Inputs
/// are differences of drawing f32 coordinates, so finite nonzero magnitudes are
/// normal f64 values and the power-of-two scale cannot overflow or underflow.
/// See CPython Modules/mathmodule.c (PSF license); this preserves math.hypot's
/// rounding at attachment lengths that put generated positions at f32 midpoints.
fn native_hypot(x: f64, y: f64) -> f64 {
    let maximum = x.abs().max(y.abs());
    if !maximum.is_finite() || maximum == 0.0 || x.is_nan() || y.is_nan() {
        return maximum;
    }
    let exponent = ((maximum.to_bits() >> 52) & 0x7ff) as i32;
    let scale = 2.0_f64.powi(1022 - exponent);
    let mut sum = 1.0;
    let mut products = 0.0;
    let mut additions = 0.0;
    for value in [x.abs(), y.abs()] {
        let value = value * scale;
        let square = value * value;
        let error = value.mul_add(value, -square);
        let combined = sum + square;
        additions += square - (combined - sum);
        products += error;
        sum = combined;
    }
    let mut norm = (sum - 1.0 + (products + additions)).sqrt();
    let square = -norm * norm;
    let error = (-norm).mul_add(norm, -square);
    let combined = sum + square;
    additions += square - (combined - sum);
    products += error;
    sum = combined;
    norm += (sum - 1.0 + (products + additions)) / (2.0 * norm);
    norm / scale
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Geometry {
    label: String,
    smiles: String,
    positions: Vec<Point3>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Geometries {
    rdkit_version: String,
    templates: Vec<Geometry>,
}
fn geometry_json() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => include_str!("geometry-macos-aarch64.json"),
        ("linux", "x86_64") => include_str!("geometry-linux-x86_64.json"),
        ("linux", "aarch64") => include_str!("geometry-linux-aarch64.json"),
        ("windows", "x86_64") => include_str!("geometry-windows-x86_64.json"),
        // The x64 reference worker produces different low bits under ARM emulation.
        ("windows", "aarch64") => include_str!("geometry-windows-aarch64.json"),
        _ => "",
    }
}

fn geometries() -> Result<&'static [Geometry]> {
    static DATA: OnceLock<std::result::Result<Geometries, String>> = OnceLock::new();
    DATA.get_or_init(|| {
        if geometry_json().is_empty() {
            return Err("Preset geometry is unavailable for this target ABI".into());
        }
        let data: Geometries =
            serde_json::from_str(geometry_json()).map_err(|error| error.to_string())?;
        let presets = presets().map_err(|error| error.to_string())?;
        if data.rdkit_version != RDKIT_VERSION || data.templates.len() != presets.len() {
            return Err("Template geometry version or count mismatch".into());
        }
        for (template, preset) in data.templates.iter().zip(presets) {
            if template.label != preset.label
                || template.smiles != preset.smiles
                || template.positions.len() != preset.atoms.len()
                || template
                    .positions
                    .iter()
                    .any(|p| !p.x.is_finite() || !p.y.is_finite() || p.z != 0.0)
            {
                return Err("Invalid template geometry".into());
            }
        }
        Ok(data)
    })
    .as_ref()
    .map(|data| data.templates.as_slice())
    .map_err(|error| Error::Definition(error.clone()))
}

/// Restore the fixed fragment's chemistry after deleting its outside dummy.
/// No template contains stereo tags, so its full CIP result is provably empty;
/// this condition is checked before supplying empty labels to reconstruction.
fn fragment(preset: &Preset) -> Result<perception::State> {
    let prepared = smiles::prepare(&preset.smiles)?;
    if at(&prepared.state.graph.atoms, 0)?.atomic_number != 0
        || prepared
            .state
            .metadata
            .atoms
            .iter()
            .any(|a| a.chiral_tag != 0 || a.map_present)
        || prepared
            .state
            .metadata
            .bonds
            .iter()
            .any(|b| b.stereo != 0 || !b.stereo_atoms.is_empty())
        || prepared
            .state
            .properties
            .atoms
            .iter()
            .any(|a| a.cip_code.is_some())
        || prepared
            .state
            .properties
            .bond_codes
            .iter()
            .any(Option::is_some)
        || !prepared.state.metadata.groups.is_empty()
    {
        return Err(Error::Definition(
            "Template contains unexpected stereo or map annotations".into(),
        ));
    }
    let graph = prepared.state.graph;
    let mut atoms = graph.atoms.into_iter().skip(1).collect::<Vec<_>>();
    let root = atoms
        .first_mut()
        .ok_or_else(|| Error::Definition("Missing attachment atom".into()))?;
    root.no_implicit = false;
    root.radical_electrons = 0;
    let bonds = graph
        .bonds
        .into_iter()
        .filter(|b| b.a != 0 && b.b != 0)
        .map(|mut b| {
            b.a -= 1;
            b.b -= 1;
            b
        })
        .collect();
    let graph = Graph { atoms, bonds };
    let sanitized = sanitize::sanitize(
        &graph,
        &Metadata::unspecified(&graph),
        &vec![Direction::None; graph.bonds.len()],
    )?;
    let properties = perception::Properties::unspecified(&sanitized.graph);
    Ok(perception::State {
        graph: sanitized.graph,
        metadata: sanitized.metadata,
        directions: sanitized.directions,
        valences: sanitized.valences,
        conjugated: sanitized.conjugated,
        hybridizations: sanitized.hybridizations,
        rings: perception::RingCache {
            kind: perception::RingKind::Symmetric,
            atoms: sanitized.rings,
        },
        properties,
    })
}

/// Mirror the original replacement edit. It retains the existing attachment ID,
/// appends new atoms/bonds, extends groups/reaction membership and replaces only
/// intersecting abbreviations. Input and final whole-document chemistry should
/// be prepared by the caller, just as they are around the original Python edit.
pub fn replace(document: &Document, selection: &[u64], label: &str) -> Result<Document> {
    let preset = presets()?
        .iter()
        .find(|p| p.label == label)
        .ok_or(Error::Label)?;
    check_size(document)?;
    if selection.len() > 100_000 {
        return Err(Error::Selection);
    }
    let remove: HashSet<_> = selection.iter().copied().collect();
    let old = document
        .abbreviations
        .iter()
        .find(|g| g.members.iter().copied().collect::<HashSet<_>>() == remove);
    let target = old
        .map(|g| g.anchor)
        .or_else(|| {
            if selection.len() == 1 {
                selection.first().copied()
            } else {
                None
            }
        })
        .ok_or(Error::Selection)?;
    let atoms: HashMap<_, _> = document.atoms.iter().map(|a| (a.id, a)).collect();
    let anchor = atoms.get(&target).ok_or(Error::Selection)?;
    let boundary = document
        .bonds
        .iter()
        .filter(|b| remove.contains(&b.a) != remove.contains(&b.b))
        .collect::<Vec<_>>();
    if boundary.len() > 1
        || boundary
            .iter()
            .any(|b| b.order != 1 || b.a != target && b.b != target)
    {
        return Err(Error::Attachment);
    }
    let geometry = geometries()?
        .iter()
        .find(|g| g.label == label)
        .ok_or_else(|| Error::Definition("Missing geometry".into()))?;
    let root = at(&geometry.positions, 1)?;
    let dummy = at(&geometry.positions, 0)?;
    let original_angle = (-(dummy.y - root.y)).atan2(dummy.x - root.x);
    // Requests pass through serde_json::Value before encoding, which promotes
    // the exact f32 value to f64 before the worker receives its coordinates.
    let origin = Point3 {
        x: f64::from(anchor.position.x),
        y: f64::from(anchor.position.y),
        z: 0.0,
    };
    let outside = boundary
        .first()
        .map(|b| {
            atoms
                .get(&if b.a == target { b.b } else { b.a })
                .map(|a| Point3 {
                    x: f64::from(a.position.x),
                    y: f64::from(a.position.y),
                    z: 0.0,
                })
                .ok_or(Error::Attachment)
        })
        .transpose()?;
    let desired_angle = outside
        .map(|p| (p.y - origin.y).atan2(p.x - origin.x))
        .unwrap_or(std::f64::consts::PI);
    let angle = desired_angle - original_angle;
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    let (s, c) = trigonometry::sin_cos(angle).ok_or(Error::Geometry)?;
    #[cfg(not(all(target_os = "windows", target_arch = "aarch64")))]
    let (c, s) = (angle.cos(), angle.sin());
    let scale = outside
        .map(|p| native_hypot(p.x - origin.x, p.y - origin.y) / 1.5)
        .unwrap_or(28.0);
    if !scale.is_finite() || scale < 0.1 {
        return Err(Error::Geometry);
    }
    // The reference includes group IDs but excludes reaction references.
    let maximum = document
        .atoms
        .iter()
        .map(|a| a.id)
        .chain(document.annotations.iter().map(|a| a.id))
        .chain(document.arrows.iter().map(|a| a.id))
        .chain(document.graphics.iter().map(|a| a.id))
        .chain(document.groups.iter().map(|a| a.id))
        .max()
        .unwrap_or(0);
    let mut next_id = u128::from(maximum) + 1;
    let mut ids = Vec::with_capacity(geometry.positions.len() - 1);
    let mut positions = Vec::with_capacity(geometry.positions.len() - 1);
    for (index, point) in geometry.positions.iter().enumerate().skip(1) {
        let identifier = if index == 1 {
            u128::from(target)
        } else {
            let id = next_id;
            next_id += 1;
            id
        };
        if identifier >= u128::from(u64::MAX) {
            return Err(Error::Identity);
        }
        ids.push(u64::try_from(identifier).map_err(|_| Error::Identity)?);
        let x = (point.x - root.x) * scale;
        let y = -(point.y - root.y) * scale;
        positions.push(Point3 {
            x: (origin.x + x * c - y * s) / 28.0,
            y: -(origin.y + x * s + y * c) / 28.0,
            z: 0.0,
        });
    }
    let molecule = Molecule {
        rdkit_version: RDKIT_VERSION,
        ids,
        positions,
        state: fragment(preset)?,
    };
    let drawing = document::for_import(&molecule, false, &vec![None; molecule.ids.len()])?;
    let state = &drawing.molecule().state;
    if state.metadata.atoms.iter().any(|a| a.chiral_tag != 0)
        || state
            .metadata
            .bonds
            .iter()
            .any(|b| b.stereo != 0 || !b.stereo_atoms.is_empty())
    {
        return Err(Error::Definition(
            "Reconstructed template contains unexpected stereo".into(),
        ));
    }
    let labels = Labels {
        rdkit_version: RDKIT_VERSION.into(),
        atoms: vec![None; molecule.ids.len()],
        bonds: state
            .graph
            .bonds
            .iter()
            .map(|_| BondLabel {
                code: None,
                stereo: 0,
                stereo_atoms: Vec::new(),
            })
            .collect(),
    };
    // Responses also pass through Value: JSON first recovers each f64, then
    // Document rounds it to f32 exactly as the shared drawing builder does.
    let mut part = drawing.finish(labels)?;
    for atom in &mut part.atoms {
        if atom.id == target {
            atom.text_style = anchor.text_style.clone();
        }
    }
    let members = part.atoms.iter().map(|a| a.id).collect::<Vec<_>>();
    let mut result = document.clone();
    result.atoms.retain(|a| !remove.contains(&a.id));
    result.atoms.extend(part.atoms);
    result
        .bonds
        .retain(|b| !(remove.contains(&b.a) && remove.contains(&b.b)));
    result.bonds.extend(part.bonds);
    for group in &mut result.groups {
        if group.members.contains(&target) {
            group.members.retain(|id| !remove.contains(id));
            group.members.extend(&members);
        }
    }
    result
        .abbreviations
        .retain(|g| !g.members.iter().any(|id| remove.contains(id)));
    for reaction in &mut result.reactions {
        for role in [
            &mut reaction.reactants,
            &mut reaction.products,
            &mut reaction.agents,
        ] {
            for participant in role {
                if participant.atoms.contains(&target) {
                    participant.atoms.retain(|id| !remove.contains(id));
                    participant.atoms.extend(&members);
                }
            }
        }
    }
    result.abbreviations.push(Abbreviation {
        label: label.into(),
        reverse_label: preset.reverse_label.clone(),
        anchor: target,
        members,
    });
    result.version = 15;
    validate(&result)?;
    Ok(result)
}
