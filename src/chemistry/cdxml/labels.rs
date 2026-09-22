//! Detached label/indicator patches from engine/labels_exchange.py::read_labels.
use super::{
    Error,
    association::{self, ImportPoint, ObjectMapEntry, PreparedAtoms, invalid, numbers},
    presentation::{Attributes, NativeText, NativeTextStyle},
    tree::Tree,
};
use crate::{
    atom_labels::{Carbons, HydrogenPosition, Settings},
    typography::Script,
};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum LabelsError<E> {
    #[error(transparent)]
    Xml(#[from] Error),
    #[error("{0}")]
    Text(E),
}
#[derive(Clone, Debug, Serialize)]
pub struct NativeNumber {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<ImportPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<NativeTextStyle>,
}
#[derive(Clone, Debug, Serialize)]
pub struct NativeStereo {
    pub show: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<ImportPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<NativeTextStyle>,
}
impl NativeStereo {
    fn new(show: bool) -> Self {
        Self {
            show,
            offset: None,
            style: None,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct NativeAtomDisplay {
    pub carbons: Carbons,
    pub hydrogens: bool,
    pub stereo: NativeStereo,
    pub hydrogen_position: HydrogenPosition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<NativeNumber>,
}
#[derive(Clone, Debug, Serialize)]
pub struct AtomLabel {
    pub id: u64,
    pub display: NativeAtomDisplay,
}
#[derive(Clone, Debug, Serialize)]
pub struct BondIndicator {
    /// Index in the caller's original base bond list, preserving first-match
    /// behavior for duplicate endpoint pairs and either bond orientation.
    pub index: usize,
    pub indicator: NativeStereo,
}
#[derive(Clone, Debug, Serialize)]
pub struct Labels {
    pub atom_labels: Settings,
    /// First atom encounter order. Replace display on the first base atom with
    /// the ID, creating it at the end if absent. Keep all unrelated fields.
    pub atoms: Vec<AtomLabel>,
    /// First source bond encounter order; replace the indicator at this index.
    pub bonds: Vec<BondIndicator>,
    pub objects: Vec<ObjectMapEntry>,
}

enum Owner {
    Atom(usize),
    Bond(usize),
}
impl Owner {
    fn stereo<'a>(&self, output: &'a mut Labels) -> super::Result<&'a mut NativeStereo> {
        match *self {
            Self::Atom(i) => output.atoms.get_mut(i).map(|a| &mut a.display.stereo),
            Self::Bond(i) => output.bonds.get_mut(i).map(|b| &mut b.indicator),
        }
        .ok_or_else(|| invalid("Missing label output owner"))
    }
}
fn yes(attrs: &Attributes, name: &str) -> super::Result<bool> {
    match attrs.get(name).map(String::as_str) {
        None | Some("no") => Ok(false),
        Some("yes") => Ok(true),
        _ => Err(invalid(&format!("Invalid CDXML label setting: {name}"))),
    }
}
fn carbons(attrs: &Attributes) -> super::Result<Carbons> {
    Ok(
        match (
            yes(attrs, "ShowTerminalCarbonLabels")?,
            yes(attrs, "ShowNonTerminalCarbonLabels")?,
        ) {
            (false, false) => Carbons::Skeletal,
            (true, false) => Carbons::Terminal,
            (false, true) => Carbons::Internal,
            (true, true) => Carbons::All,
        },
    )
}
fn inherited(tree: &Tree, element: usize) -> super::Result<Attributes> {
    let mut chain = Vec::new();
    let mut current = Some(element);
    while let Some(index) = current {
        chain.push(index);
        current = tree.node(index)?.parent;
    }
    let mut attrs = Attributes::new();
    for index in chain.into_iter().rev() {
        for (key, value) in &tree.node(index)?.attributes {
            tree.spend(key.len().saturating_add(value.len()))?;
            attrs.insert(key.clone(), value.clone());
        }
    }
    Ok(attrs)
}
fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}

/// Read appearance using prepared atoms and positions AFTER conformer scaling.
/// `bond_endpoints` is the existing drawing's bond list in its original order.
/// The text callback is lazy and called in the original all-atoms-then-all-bonds
/// order, even for hidden indicators. It receives the text element's document
/// ordinal and inherited OWNER attributes; use TextReader::read(node,Some(attrs),false).
/// Source XML, chemistry, and an existing drawing are never modified. Imported
/// stereo text controls appearance only and is never stored as chemical truth.
pub fn read_labels<E, F>(
    xml: &str,
    prepared: &PreparedAtoms<'_>,
    bond_endpoints: &[[u64; 2]],
    source_scale: f64,
    mut read_text: F,
) -> std::result::Result<Labels, LabelsError<E>>
where
    F: FnMut(usize, &Attributes) -> std::result::Result<NativeText, E>,
{
    association::scale(source_scale)?;
    if bond_endpoints.len() > 200_000 {
        return Err(Error::Limit.into());
    }
    let tree = Tree::parse(xml)?;
    let order = tree.descendants(0)?;
    let ordinals: HashMap<_, _> = order
        .iter()
        .copied()
        .enumerate()
        .map(|(ordinal, node)| (node, ordinal))
        .collect();
    let root_attrs: Attributes = tree.node(0)?.attributes.iter().cloned().collect();
    let mut output = Labels {
        atom_labels: Settings {
            carbons: carbons(&root_attrs)?,
            hydrogens: !yes(&root_attrs, "HideImplicitHydrogens")?,
            stereo: yes(&root_attrs, "ShowAtomStereo")?,
        },
        atoms: Vec::new(),
        bonds: Vec::new(),
        objects: Vec::new(),
    };
    let mut nodes = HashMap::new();
    let mut positions = HashMap::new();
    let mut entries = HashMap::new();
    let mut source_atoms = Vec::new();
    let mut source_bonds = Vec::new();
    for &index in &order {
        let node = tree.node(index)?;
        if node.tag == "b" {
            source_bonds.push(index);
        }
        if node.tag != "n" {
            continue;
        }
        source_atoms.push(index);
        let values = numbers(
            node.attr("p").unwrap_or(""),
            "Invalid atom label coordinates",
        )?;
        let [x, y] = values.as_slice() else {
            return Err(invalid("Invalid atom label coordinates").into());
        };
        let point = ImportPoint {
            x: x * source_scale,
            y: y * source_scale,
        };
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(invalid("Invalid atom label coordinates").into());
        }
        let id = prepared.identify(
            &tree,
            index,
            point,
            "Could not safely associate atom labels with their atom",
        )?;
        nodes.insert(node.attr("id"), id);
        positions.insert(id, point);
        let attrs = inherited(&tree, index)?;
        let mut display = NativeAtomDisplay {
            carbons: carbons(&attrs)?,
            hydrogens: !yes(&attrs, "HideImplicitHydrogens")?,
            stereo: NativeStereo::new(yes(&attrs, "ShowAtomStereo")?),
            hydrogen_position: HydrogenPosition::Auto,
            number: None,
        };
        display.hydrogen_position = match attrs
            .get("LabelDisplay")
            .map(String::as_str)
            .unwrap_or("Auto")
        {
            "Auto" | "Best" => HydrogenPosition::Auto,
            "Left" => HydrogenPosition::Left,
            "Right" => HydrogenPosition::Right,
            "Above" => HydrogenPosition::Above,
            "Below" => HydrogenPosition::Below,
            _ => return Err(invalid("This atom-label alignment is not supported yet").into()),
        };
        if yes(&attrs, "ShowAtomNumber")?
            && let Some(value) = node.attr("AtomNumber").filter(|v| !v.is_empty())
        {
            display.number = Some(NativeNumber {
                text: value.into(),
                offset: None,
                style: None,
            });
        }
        let entry = *entries.entry(id).or_insert_with(|| output.atoms.len());
        if entry == output.atoms.len() {
            output.atoms.push(AtomLabel { id, display });
        } else {
            output
                .atoms
                .get_mut(entry)
                .ok_or_else(|| invalid("Missing label output atom"))?
                .display = display;
        }
        output.objects.push(ObjectMapEntry {
            source: *ordinals
                .get(&index)
                .ok_or_else(|| invalid("Missing source object ordinal"))?,
            atoms: vec![id],
        });
    }
    let mut bonds = HashMap::new();
    for (index, &[a, b]) in bond_endpoints.iter().enumerate() {
        bonds.entry(pair(a, b)).or_insert(index);
    }
    let mut bond_entries = HashMap::new();
    for index in source_atoms.into_iter().chain(source_bonds) {
        let element = tree.node(index)?;
        let attrs = inherited(&tree, index)?;
        let (owner, anchor, show_number) = if element.tag == "n" {
            let id = *nodes
                .get(&element.attr("id"))
                .ok_or_else(|| invalid("Missing label source atom"))?;
            let entry = *entries
                .get(&id)
                .ok_or_else(|| invalid("Missing label output atom"))?;
            (
                Owner::Atom(entry),
                *positions
                    .get(&id)
                    .ok_or_else(|| invalid("Missing label atom position"))?,
                yes(&attrs, "ShowAtomNumber")?,
            )
        } else {
            let missing = || invalid("A stereochemistry indicator refers to a missing bond");
            let a = *nodes.get(&element.attr("B")).ok_or_else(missing)?;
            let b = *nodes.get(&element.attr("E")).ok_or_else(missing)?;
            let bond = *bonds.get(&pair(a, b)).ok_or_else(missing)?;
            let indicator = NativeStereo::new(yes(&attrs, "ShowBondStereo")?);
            let entry = *bond_entries
                .entry(bond)
                .or_insert_with(|| output.bonds.len());
            if entry == output.bonds.len() {
                output.bonds.push(BondIndicator {
                    index: bond,
                    indicator,
                });
            } else {
                output.bonds.get_mut(entry).ok_or_else(missing)?.indicator = indicator;
            }
            let a = positions.get(&a).ok_or_else(missing)?;
            let b = positions.get(&b).ok_or_else(missing)?;
            (
                Owner::Bond(entry),
                ImportPoint {
                    x: (a.x + b.x) / 2.,
                    y: (a.y + b.y) / 2.,
                },
                false,
            )
        };
        let (mut number_seen, mut stereo_seen) = (false, false);
        for tag in tree.children(index, "objecttag")? {
            let name = tree.node(tag)?.attr("Name");
            let seen = match name {
                Some("number") if element.tag == "n" => &mut number_seen,
                Some("stereo") => &mut stereo_seen,
                _ => return Err(invalid("Unsupported or duplicate atom/bond object tag").into()),
            };
            if std::mem::replace(seen, true) {
                return Err(invalid("Unsupported or duplicate atom/bond object tag").into());
            }
            let texts = tree.children(tag, "t")?;
            let [text] = texts.as_slice() else {
                return Err(invalid("An atom indicator must contain one text object").into());
            };
            let source = *ordinals
                .get(text)
                .ok_or_else(|| invalid("Missing source text ordinal"))?;
            let value = read_text(source, &attrs).map_err(LabelsError::Text)?;
            if !value.format.spans.is_empty() || value.format.style.script != Script::Normal {
                return Err(
                    invalid("Mixed formatting in atom indicators is not supported yet").into(),
                );
            }
            if value.text.is_empty()
                || value.text.chars().count() > 32
                || value.text.chars().any(|c| u32::from(c) < 32)
            {
                return Err(invalid("Invalid atom indicator text").into());
            }
            if (name == Some("number") && !show_number)
                || (name == Some("stereo") && !owner.stereo(&mut output)?.show)
            {
                continue;
            }
            tree.spend(value.format.style.family.len())?;
            let bounds = numbers(
                tree.node(*text)?.attr("BoundingBox").unwrap_or(""),
                "Invalid atom indicator bounds",
            )?;
            let offset = if bounds.is_empty() {
                None
            } else {
                let [x1, y1, x2, y2] = bounds.as_slice() else {
                    return Err(invalid("Invalid atom indicator bounds").into());
                };
                let (x1, y1, x2, y2) = (
                    x1 * source_scale,
                    y1 * source_scale,
                    x2 * source_scale,
                    y2 * source_scale,
                );
                if ![x1, y1, x2, y2].iter().all(|v| v.is_finite()) {
                    return Err(invalid("Invalid atom indicator bounds").into());
                }
                Some(ImportPoint {
                    x: (x1 + x2) / 2. - anchor.x,
                    y: (y1 + y2) / 2. - anchor.y,
                })
            };
            if name == Some("number") {
                let Owner::Atom(entry) = owner else {
                    return Err(invalid("Missing number owner").into());
                };
                output
                    .atoms
                    .get_mut(entry)
                    .ok_or_else(|| invalid("Missing label output atom"))?
                    .display
                    .number = Some(NativeNumber {
                    text: value.text,
                    offset,
                    style: Some(value.format.style),
                });
            } else {
                let stereo = owner.stereo(&mut output)?;
                stereo.style = Some(value.format.style);
                if offset.is_some() {
                    stereo.offset = offset;
                }
            }
        }
    }
    for &index in &order {
        let element = tree.node(index)?;
        if element.tag == "objecttag"
            && !matches!(
                element
                    .parent
                    .map(|i| tree.node(i))
                    .transpose()?
                    .map(|n| n.tag.as_str()),
                Some("n" | "b")
            )
        {
            return Err(invalid("Unattached object tags are not supported yet").into());
        }
    }
    Ok(output)
}
