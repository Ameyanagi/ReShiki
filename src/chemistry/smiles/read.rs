//! Default SMILES import annotations adapted from SmilesParse.cpp and
//! CXSmilesOps.cpp, Copyright (C) 2001-2022 Greg Landrum and contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{Error, Parsed, Prepared, Result, parse_inner, prepare};
use crate::chemistry::{
    cx,
    kekulize::Direction,
    ranking::StereoGroup,
    stereo::{self, Point3, wedging::Conformer},
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Serialize)]
pub struct Imported {
    pub prepared: Prepared,
    pub conformers: Vec<Conformer>,
    pub name: Option<String>,
    /// Reaction CX attachment markers affect wedge selection before drawing.
    #[serde(skip)]
    pub(crate) reaction_attachments: Vec<bool>,
    /// Preserve native string types until the temporary layout pass reads them.
    #[serde(skip)]
    pub(crate) reaction_properties: Vec<Vec<(Vec<u8>, Vec<u8>)>>,
}

fn at<T>(items: &[T], id: usize) -> Result<&T> {
    items.get(id).ok_or(Error::Limit)
}
fn put<T>(items: &mut [T], id: usize, value: T) -> Result<()> {
    *items.get_mut(id).ok_or(Error::Limit)? = value;
    Ok(())
}

struct Annotations {
    context: prepare::Context,
    properties: Vec<HashMap<Vec<u8>, Vec<u8>>>,
    queries: Vec<bool>,
    groups: HashMap<u32, usize>,
    labels_processed: bool,
    atom_stereo: bool,
    work: usize,
}
impl Annotations {
    fn new(parsed: &Parsed) -> Self {
        Self {
            context: prepare::Context::new(parsed),
            properties: vec![HashMap::new(); parsed.graph.atoms.len()],
            queries: vec![false; parsed.graph.atoms.len()],
            groups: HashMap::new(),
            labels_processed: false,
            atom_stereo: false,
            work: 2_000_000,
        }
    }
    fn spend(&mut self, count: usize) -> Result<()> {
        self.work = self.work.checked_sub(count).ok_or(Error::Limit)?;
        Ok(())
    }
    fn labels(&mut self, parsed: &mut Parsed) -> Result<()> {
        if self.labels_processed {
            return Ok(());
        }
        self.labels_processed = true;
        for (id, (atom, props)) in parsed
            .graph
            .atoms
            .iter()
            .zip(&mut self.properties)
            .enumerate()
        {
            if let Some(label) = props.get(b"atomLabel".as_slice()).cloned() {
                put(&mut parsed.dummy_labels, id, None)?;
                props.remove(b"dummyLabel".as_slice());
                if [
                    b"star_e".as_slice(),
                    b"Q_e",
                    b"QH_p",
                    b"AH_p",
                    b"X_p",
                    b"XH_p",
                    b"M_p",
                    b"MH_p",
                ]
                .contains(&label.as_slice())
                {
                    put(&mut self.queries, id, true)?;
                } else if label == b"Pol_p" || label == b"Mod_p" {
                    let label = label.get(..3).ok_or(Error::Limit)?.to_vec();
                    props.insert(b"dummyLabel".to_vec(), label);
                    props.remove(b"atomLabel".as_slice());
                }
            } else if atom.atomic_number == 0
                && atom.isotope == 0
                && props
                    .get(b"dummyLabel".as_slice())
                    .map(Vec::as_slice)
                    .or_else(|| {
                        parsed
                            .dummy_labels
                            .get(id)
                            .and_then(|label| label.as_ref())
                            .map(|label| label.as_bytes())
                    })
                    .is_none_or(|symbol| symbol == b"*")
            {
                put(&mut self.queries, id, true)?;
            }
        }
        Ok(())
    }
    fn apply(&mut self, parsed: &mut Parsed, event: cx::Event) -> Result<()> {
        match event {
            cx::Event::AtomProperty { atom, name, value } => {
                self.properties
                    .get_mut(atom)
                    .ok_or(Error::Limit)?
                    .insert(name, value);
            }
            cx::Event::Coordinates(points) => {
                self.spend(parsed.graph.atoms.len())?;
                let mut positions = vec![Point3::default(); parsed.graph.atoms.len()];
                let mut marked = false;
                for (point, fields) in positions.iter_mut().zip(points) {
                    for (slot, text) in [&mut point.x, &mut point.y, &mut point.z]
                        .into_iter()
                        .zip(&fields)
                    {
                        if !text.is_empty() {
                            *slot = cx::coordinate(
                                std::str::from_utf8(text)
                                    .map_err(|_| Error::Unsupported("non-UTF8 coordinate"))?,
                            )
                            .ok_or(Error::Unsupported("coordinate"))?;
                        }
                    }
                    marked |= fields.get(2).is_some_and(|s| !s.is_empty());
                }
                let is_3d = marked && positions.iter().any(|p| p.z.abs() > 1e-3);
                self.context.conformers.push(Conformer { positions, is_3d });
            }
            cx::Event::BondKind { bond, begin, kind } => {
                put(
                    &mut self.context.unsupported_bonds,
                    bond,
                    matches!(kind, cx::BondKind::Zero),
                )?;
                let bond = parsed.graph.bonds.get_mut(bond).ok_or(Error::Limit)?;
                bond.order = if matches!(kind, cx::BondKind::Coordinate) {
                    5
                } else {
                    0
                };
                if begin.is_some_and(|a| a != bond.a) {
                    std::mem::swap(&mut bond.a, &mut bond.b);
                }
            }
            cx::Event::Radical { atom, electrons } => {
                parsed
                    .graph
                    .atoms
                    .get_mut(atom)
                    .ok_or(Error::Limit)?
                    .radical_electrons = electrons;
            }
            cx::Event::StereoGroup { kind, id, atoms } => {
                let hash = id.wrapping_mul(10).wrapping_add(u32::from(kind));
                if let Some(&index) = self.groups.get(&hash) {
                    let group = parsed.metadata.groups.get_mut(index).ok_or(Error::Limit)?;
                    group.atoms.extend(atoms);
                    group.read_id = id;
                } else {
                    self.groups.insert(hash, parsed.metadata.groups.len());
                    parsed.metadata.groups.push(StereoGroup {
                        kind,
                        read_id: id,
                        atoms,
                        ..StereoGroup::default()
                    });
                }
            }
            cx::Event::QueryAtom(atom) => put(&mut self.queries, atom, true)?,
            cx::Event::SubstanceGroup { atoms, bonds } => {
                let members = atoms.iter().copied().collect::<HashSet<_>>();
                for id in bonds {
                    let bond = at(&parsed.graph.bonds, id)?;
                    if members.contains(&bond.a) != members.contains(&bond.b) {
                        self.context
                            .annotations
                            .protected_atoms
                            .extend([bond.a, bond.b]);
                    }
                }
                self.context.annotations.substance_groups.push(atoms);
            }
            cx::Event::Wedge { bond, begin, kind } => {
                let b = parsed.graph.bonds.get_mut(bond).ok_or(Error::Limit)?;
                if b.a != begin {
                    std::mem::swap(&mut b.a, &mut b.b);
                }
                put(
                    &mut parsed.directions,
                    bond,
                    match kind {
                        1 => Direction::Wedge,
                        3 => Direction::Hash,
                        _ => Direction::Unknown,
                    },
                )?;
                if matches!(b.order, 1 | 4) {
                    if kind == 2 {
                        parsed
                            .metadata
                            .atoms
                            .get_mut(begin)
                            .ok_or(Error::Limit)?
                            .chiral_tag = 0;
                        self.context.needs_bond_stereo = true;
                    } else {
                        self.atom_stereo = true;
                    }
                }
            }
            cx::Event::BondStereo { bond, stereo } => {
                let b = at(&parsed.graph.bonds, bond)?;
                let mut controls = [None::<usize>; 2];
                self.spend(parsed.graph.bonds.len())?;
                for edge in &parsed.graph.bonds {
                    for (slot, (atom, partner)) in controls.iter_mut().zip([(b.a, b.b), (b.b, b.a)])
                    {
                        let other = if edge.a == atom {
                            Some(edge.b)
                        } else if edge.b == atom {
                            Some(edge.a)
                        } else {
                            None
                        };
                        if let Some(other) = other.filter(|&other| other != partner) {
                            *slot = Some(slot.map_or(other, |old| old.min(other)));
                        }
                    }
                }
                if let [Some(a), Some(b)] = controls {
                    let meta = parsed.metadata.bonds.get_mut(bond).ok_or(Error::Limit)?;
                    meta.stereo = stereo;
                    meta.stereo_atoms = vec![a, b];
                    self.context.needs_bond_stereo = true;
                }
            }
            cx::Event::ProcessLabels => self.labels(parsed)?,
        }
        Ok(())
    }
    fn numeric_props(&mut self, parsed: &mut Parsed) -> Result<()> {
        for (id, props) in self.properties.iter().enumerate() {
            let error = self
                .context
                .property_errors
                .get_mut(id)
                .ok_or(Error::Limit)?;
            let meta = parsed.metadata.atoms.get_mut(id).ok_or(Error::Limit)?;
            if let Some(map) = int_property(props, b"molAtomMapNumber", error) {
                meta.map_number = map;
                meta.map_present = true;
            }
            if let Some(value) = props.get(b"_chiralPermutation".as_slice()) {
                let value = std::str::from_utf8(value).ok().and_then(|text| {
                    if let Some(negative) = text.strip_prefix('-') {
                        negative.parse::<u32>().ok().map(u32::wrapping_neg)
                    } else {
                        text.parse::<u32>().ok()
                    }
                });
                if let Some(value) = value {
                    meta.chiral_permutation = Some(value);
                } else {
                    put(&mut self.context.permutation_errors, id, true)?;
                }
            }
            // Default import and drawing ranking never enable non-stereo
            // ranks. Malformed values therefore remain unused native metadata.
            if let Some(value) = int_property(props, b"_CanonicalRankingNumber", &mut None) {
                meta.non_stereo_rank = value;
            }
            let mut unknown_error = None;
            if let Some(value) = int_property(props, b"_UnknownStereo", &mut unknown_error) {
                put(&mut self.context.unknown, id, value != 0)?;
            }
            put(
                &mut self.context.unknown_errors,
                id,
                unknown_error.is_some(),
            )?;
            if let Some(value) = props.get(b"dummyLabel".as_slice()) {
                match String::from_utf8(value.clone()) {
                    Ok(value) => put(&mut parsed.dummy_labels, id, Some(value))?,
                    Err(_) => *error = Some("non-UTF8 atom label"),
                }
            }
        }
        Ok(())
    }
}

fn int_property(
    props: &HashMap<Vec<u8>, Vec<u8>>,
    key: &[u8],
    error: &mut Option<&'static str>,
) -> Option<i32> {
    let text = props.get(key)?;
    let value = std::str::from_utf8(text)
        .ok()
        .and_then(|s| s.parse::<i32>().ok());
    if value.is_none() {
        *error = Some("invalid numeric atom property");
    }
    value
}

/// Reactions apply CX annotations after bare parsing and keep explicit H.
/// They sanitize without the default molecule import's stereo perception.
pub(crate) fn reaction_part(mut parsed: Parsed, events: Vec<cx::Event>) -> Result<Imported> {
    use crate::chemistry::{
        sanitize,
        stereo::perception::{Properties, RingCache, RingKind, State},
    };
    let mut annotations = Annotations::new(&parsed);
    for event in events {
        annotations.apply(&mut parsed, event)?;
    }
    annotations.numeric_props(&mut parsed)?;
    if annotations.queries.iter().any(|&q| q)
        || annotations.context.unsupported_bonds.iter().any(|&b| b)
        || parsed.query_bonds.iter().any(|&b| b)
    {
        return Err(Error::Unsupported("reaction query or bond type"));
    }
    if let Some(error) = annotations.context.property_errors.iter().flatten().next() {
        return Err(Error::Unsupported(error));
    }
    let clean = sanitize::sanitize(&parsed.graph, &parsed.metadata, &parsed.directions)?;
    if clean.graph.atoms.iter().any(|a| a.radical_electrons > 2)
        || clean.metadata.atoms.iter().any(|a| a.chiral_tag > 2)
        || clean.metadata.bonds.iter().any(|b| b.stereo > 5)
        || !clean.metadata.groups.is_empty()
    {
        return Err(Error::Unsupported(
            "reaction stereochemistry or radical count",
        ));
    }
    let mut properties = Properties::unspecified(&clean.graph);
    properties.needs_detection = annotations.context.needs_bond_stereo.then_some(true);
    for (i, (props, atom)) in annotations
        .properties
        .iter()
        .zip(&mut properties.atoms)
        .enumerate()
    {
        atom.unknown = *at(&annotations.context.unknown, i)?;
        atom.invalid_unknown = *at(&annotations.context.unknown_errors, i)?;
        if let Some(code) = props.get(b"_CIPCode".as_slice()) {
            atom.cip_code = Some(
                String::from_utf8(code.clone())
                    .map_err(|_| Error::Unsupported("non-UTF8 CIP code"))?,
            );
        }
        if let Some(value) = props.get(b"_ChiralityPossible".as_slice()) {
            // Preserve the optional legacy flag. The native property dictionary
            // interprets numeric strings as numbers and leaves other text intact.
            atom.possible = Some(
                std::str::from_utf8(value)
                    .ok()
                    .and_then(|s| cx::coordinate(s.trim()))
                    .is_none_or(|n| n != 0.0),
            );
        }
    }
    let reaction_properties = annotations
        .properties
        .iter()
        .map(|props| {
            let mut entries = props
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect::<Vec<_>>();
            entries.sort();
            entries
        })
        .collect();
    Ok(Imported {
        reaction_properties,
        reaction_attachments: annotations
            .properties
            .iter()
            .zip(&clean.graph.atoms)
            .map(|(props, atom)| {
                props.contains_key(b"_fromAttachPoint".as_slice())
                    || atom.atomic_number == 0
                        && props
                            .get(b"atomLabel".as_slice())
                            .is_some_and(|s| matches!(s.as_slice(), b"_AP1" | b"_AP2"))
            })
            .collect(),
        prepared: Prepared {
            state: State {
                graph: clean.graph,
                metadata: clean.metadata,
                directions: clean.directions,
                valences: clean.valences,
                conjugated: clean.conjugated,
                hybridizations: clean.hybridizations,
                rings: RingCache {
                    kind: RingKind::Symmetric,
                    atoms: clean.rings,
                },
                properties,
            },
            dummy_labels: parsed.dummy_labels,
        },
        conformers: annotations.context.conformers,
        name: None,
    })
}

/// Read default SMILES/CXSMILES chemistry, retaining input conformers and name.
/// Drawing layout and the application's editable-chemistry checks follow this.
pub fn read(text: &str) -> Result<Imported> {
    if text.len() > 1024 * 1024 {
        return Err(Error::Limit);
    }
    let split = text
        .bytes()
        .position(|b| matches!(b, b' ' | b'\t'))
        .filter(|&i| i != 0);
    let (body, suffix) = if let Some(i) = split {
        (
            text.get(..i).ok_or(Error::Limit)?,
            cx::trim(text.get(i..).ok_or(Error::Limit)?),
        )
    } else {
        (text, "")
    };
    let mut parsed = parse_inner(body)?;
    let mut annotations = Annotations::new(&parsed);
    let name = if suffix.starts_with('|') {
        let topology = cx::Topology {
            atoms: parsed.graph.atoms.len(),
            bonds: parsed
                .graph
                .bonds
                .iter()
                .zip(&parsed.bond_indices)
                .map(|(b, &index)| cx::ParseBond {
                    a: b.a,
                    b: b.b,
                    index: Some(index),
                })
                .collect(),
        };
        let extension = cx::read(suffix, &topology)?;
        for event in extension.events {
            annotations.apply(&mut parsed, event)?;
        }
        cx::trim(suffix.get(extension.end..).ok_or(Error::Limit)?)
    } else {
        suffix
    };
    if annotations.queries.iter().any(|&q| q) {
        return Err(Error::Unsupported("query atom"));
    }
    annotations.numeric_props(&mut parsed)?;
    let conf = annotations
        .context
        .conformers
        .iter()
        .find(|c| !c.is_3d)
        .or_else(|| annotations.context.conformers.first());
    if let Some(conf) = conf {
        if conf.is_3d {
            parsed.metadata = stereo::from_3d_with_bounds(
                &parsed.graph,
                &parsed.metadata,
                &parsed.directions,
                Some(conf),
                &stereo::SpatialAnnotations {
                    non_explicit: vec![None; parsed.graph.atoms.len()],
                    done: None,
                },
                stereo::SpatialOptions::default(),
                stereo::CoordinateBounds::NativeImport,
            )?
            .metadata;
        } else if annotations.atom_stereo {
            let drawn = stereo::from_directions_with_bounds(
                &parsed.graph,
                &parsed.metadata,
                &parsed.directions,
                Some(&conf.positions),
                true,
                stereo::CoordinateBounds::NativeImport,
            )
            .map_err(Error::Stereo)?;
            parsed.graph = drawn.graph;
            parsed.metadata = drawn.metadata;
        }
    }
    parsed.metadata = stereo::detect_atropisomers_with_bounds(
        &parsed.graph,
        &parsed.metadata,
        &parsed.directions,
        conf,
        stereo::CoordinateBounds::NativeImport,
    )?;
    let (prepared, conformers, kept_atoms) = prepare::finish(parsed, annotations.context)?;
    // Native RemoveHs preserves properties on retained atoms. Keep the raw
    // values for the later depiction stage, which converts rank properties
    // only when the selected fragment/neighbor ordering reads them.
    let reaction_properties = kept_atoms
        .iter()
        .map(|&id| {
            let mut entries = annotations
                .properties
                .get(id)
                .ok_or(Error::Limit)?
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect::<Vec<_>>();
            entries.sort();
            Ok(entries)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Imported {
        prepared,
        conformers,
        name: (!name.is_empty()).then(|| name.to_owned()),
        reaction_attachments: Vec::new(),
        reaction_properties,
    })
}

#[cfg(test)]
mod retained_properties {
    #[test]
    fn hydrogen_removal_retains_raw_properties_by_original_atom() -> anyhow::Result<()> {
        let imported = super::read(
            "[H]CO |atomProp:0._CIPRank.dropped:1._chiralAtomRank.bad:2._CIPRank.4294967295|",
        )?;
        assert_eq!(imported.prepared.state.graph.atoms.len(), 2);
        assert_eq!(
            imported.reaction_properties,
            vec![
                vec![(b"_chiralAtomRank".to_vec(), b"bad".to_vec())],
                vec![(b"_CIPRank".to_vec(), b"4294967295".to_vec())],
            ]
        );
        Ok(())
    }
}
