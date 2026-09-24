//! Detached original CDXML scene assembly and checked drawing reconstruction.
mod circles;
mod finish;
mod ring_fills;
use super::{
    ImportPoint, NativeArrow, NativeAtomDisplay, NativeMark, NativeStereo, ObjectMapEntry,
    PreparedAtoms, PreparedCdxml,
    bonds::{self, NativeBond},
    graphics::{self, NativeGraphic},
    presentation::{
        self, Attributes, NativeDrawingStyle, NativeFormat, NativeText, NativeTextStyle, TextReader,
    },
    tree::Tree,
};
use crate::{
    chemistry::{self, document::Molecule},
    document::{self, Document, Point},
    typography::Script,
};
pub use finish::import_cdxml;
use roxmltree::Node;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, thiserror::Error)]
pub enum SceneError {
    #[error(transparent)]
    Preparation(#[from] super::PreparationError),
    #[error(transparent)]
    Xml(#[from] super::Error),
    #[error(transparent)]
    Presentation(#[from] presentation::Error),
    #[error(transparent)]
    Numeric(#[from] presentation::NumericError),
    #[error(transparent)]
    Labels(#[from] super::LabelsError<presentation::Error>),
    #[error(transparent)]
    Arrow(#[from] super::ArrowError),
    #[error(transparent)]
    Graphics(#[from] graphics::Error),
    #[error(transparent)]
    Bonds(#[from] bonds::Error),
    #[error(transparent)]
    Drawing(#[from] chemistry::document::Error),
    #[error("{0}")]
    Invalid(&'static str),
    #[error("'{0}'")]
    Missing(&'static str),
    #[error("CDXML scene exceeds its ID, object, or work limit")]
    Limit,
}
type Result<T> = std::result::Result<T, SceneError>;

#[derive(Clone, Debug, Serialize)]
pub struct NativeCaption {
    pub id: u64,
    pub position: ImportPoint,
    pub text: String,
    pub format: NativeFormat,
}
#[derive(Clone, Debug, Serialize)]
pub struct SceneAtom {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_style: Option<NativeTextStyle>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<NativeMark>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<NativeAtomDisplay>,
}
#[derive(Clone, Debug, Serialize)]
pub struct SceneBond {
    #[serde(flatten)]
    pub bond: NativeBond,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicator: Option<NativeStereo>,
}
#[derive(Clone, Debug, Serialize)]
pub struct NativeScene {
    pub drawing_style: NativeDrawingStyle,
    pub atoms: Vec<SceneAtom>,
    pub bonds: Vec<SceneBond>,
    pub annotations: Vec<NativeCaption>,
    pub arrows: Vec<NativeArrow>,
    pub atom_labels: crate::atom_labels::Settings,
    pub graphics: Vec<NativeGraphic>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ring_fills: Vec<crate::ring_fills::RingFill>,
    pub groups: Vec<crate::grouping::Group>,
    pub abbreviations: Vec<crate::abbreviations::Abbreviation>,
}
#[derive(Clone, Debug, Serialize)]
pub struct CdxmlScene {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<crate::attachments::Attachment>,
    pub molecule: Molecule,
    pub conformer_3d: Option<bool>,
    pub base: NativeScene,
    /// Object identities remain ordinals in PreparedCdxml::expanded_xml, even
    /// after owned circles are excluded from subsequent readers.
    pub objects: Vec<ObjectMapEntry>,
    pub removed_sources: Vec<usize>,
}
#[derive(Clone, Debug)]
pub struct ImportedCdxml {
    pub molecule: Molecule,
    pub document: Document,
}

fn point(text: &str, scale: f64) -> Result<ImportPoint> {
    let mut xy = [0.0; 2];
    let mut count = 0usize;
    for text in text.split_whitespace() {
        let value = super::numeric::float(text)?;
        if !value.is_finite() {
            return Err(SceneError::Invalid("Invalid CDXML coordinates"));
        }
        if let Some(slot) = xy.get_mut(count) {
            *slot = value;
        }
        count += 1;
    }
    if count < 2 {
        return Err(SceneError::Invalid("Invalid CDXML coordinates"));
    }
    Ok(ImportPoint {
        x: xy[0] * scale,
        y: xy[1] * scale,
    })
}
fn attributes(node: Node<'_, '_>) -> Attributes {
    node.attributes()
        .map(|a| (a.name().into(), a.value().into()))
        .collect()
}
fn plain(mut style: NativeTextStyle) -> NativeTextStyle {
    style.script = Script::Normal;
    style.formula = false;
    style
}
fn default_style() -> NativeTextStyle {
    NativeTextStyle {
        family: "Arial".into(),
        size_pt: 10.0,
        bold: false,
        italic: false,
        underline: false,
        color: presentation::NativeColor([0.0; 3]),
        script: Script::Normal,
        formula: false,
    }
}
fn atom_slot(
    atoms: &mut Vec<SceneAtom>,
    slots: &mut HashMap<u64, usize>,
    id: u64,
) -> Result<usize> {
    if let Some(&slot) = slots.get(&id) {
        return Ok(slot);
    }
    if atoms.len() >= 100_000 {
        return Err(SceneError::Limit);
    }
    let slot = atoms.len();
    atoms.push(SceneAtom {
        id,
        text_style: None,
        marks: Vec::new(),
        display: None,
    });
    slots.insert(id, slot);
    Ok(slot)
}
struct Objects {
    entries: Vec<ObjectMapEntry>,
    slots: HashMap<usize, usize>,
    members: usize,
}
impl Objects {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            slots: HashMap::new(),
            members: 0,
        }
    }
    fn put(&mut self, source: usize, atoms: Vec<u64>) -> Result<()> {
        let previous = self
            .slots
            .get(&source)
            .and_then(|&i| self.entries.get(i))
            .map_or(0, |p| p.atoms.len());
        self.members = self
            .members
            .checked_sub(previous)
            .and_then(|n| n.checked_add(atoms.len()))
            .filter(|n| *n <= 1_000_000)
            .ok_or(SceneError::Limit)?;
        if let Some(&slot) = self.slots.get(&source) {
            self.entries.get_mut(slot).ok_or(SceneError::Limit)?.atoms = atoms;
        } else {
            if self.entries.len() >= 100_000 {
                return Err(SceneError::Limit);
            }
            self.slots.insert(source, self.entries.len());
            self.entries.push(ObjectMapEntry { source, atoms });
        }
        Ok(())
    }
    fn get(&self, source: usize) -> Option<&[u64]> {
        self.slots
            .get(&source)
            .and_then(|&i| self.entries.get(i))
            .map(|e| e.atoms.as_slice())
    }
}

/// Assemble native helper values without narrowing numbers or changing the
/// prepared snapshot. All source identities refer to its expanded XML.
pub fn assemble_cdxml(prepared: &PreparedCdxml) -> Result<CdxmlScene> {
    let members = prepared
        .fragment_bindings
        .iter()
        .try_fold(0usize, |total, entry| {
            total
                .checked_add(entry.atoms.len())
                .filter(|n| *n <= 1_000_000)
        });
    if prepared.molecule.ids.len() > 100_000
        || prepared.bonds.len() > 300_000
        || prepared.fragment_bindings.len() > 100_000
        || members.is_none()
    {
        return Err(SceneError::Limit);
    }
    let source = presentation::parse(&prepared.expanded_xml)?;
    let nodes = source
        .descendants()
        .filter(Node::is_element)
        .collect::<Vec<_>>();
    let ordinals: HashMap<_, _> = nodes.iter().enumerate().map(|(i, n)| (n.id(), i)).collect();
    let root = source.root_element();
    let reader = TextReader::new(root)?;
    let mut next_id = u64::try_from(prepared.molecule.ids.len())
        .map_err(|_| SceneError::Limit)?
        .checked_add(1)
        .ok_or(SceneError::Limit)?;
    if prepared
        .molecule
        .ids
        .iter()
        .enumerate()
        .any(|(i, id)| u64::try_from(i + 1).ok() != Some(*id))
    {
        return Err(SceneError::Invalid("Prepared CDXML atom IDs changed"));
    }
    let mut objects = Objects::new();
    for binding in &prepared.fragment_bindings {
        if binding.source >= nodes.len() {
            return Err(SceneError::Limit);
        }
        objects.put(binding.source, binding.atoms.clone())?;
    }
    let mut base = NativeScene {
        drawing_style: prepared.drawing_style.clone(),
        atoms: Vec::new(),
        bonds: prepared
            .bonds
            .iter()
            .cloned()
            .map(|bond| SceneBond {
                bond,
                indicator: None,
            })
            .collect(),
        annotations: Vec::new(),
        arrows: Vec::new(),
        atom_labels: Default::default(),
        graphics: Vec::new(),
        ring_fills: Vec::new(),
        groups: Vec::new(),
        abbreviations: Vec::new(),
    };
    let arrows = super::ArrowReader::new(&prepared.expanded_xml)?;
    for page in nodes.iter().filter(|n| n.has_tag_name("page")) {
        let defaults = attributes(*page);
        for node in page.descendants().filter(Node::is_element) {
            let ordinal = *ordinals.get(&node.id()).ok_or(SceneError::Limit)?;
            if node.has_tag_name("t")
                && node
                    .parent_element()
                    .is_none_or(|p| !matches!(p.tag_name().name(), "n" | "objecttag"))
            {
                let text = reader.read(node, Some(&defaults), false)?;
                let position = point(
                    node.attribute("BoundingBox")
                        .or_else(|| node.attribute("p"))
                        .unwrap_or("0 0"),
                    prepared.source_scale,
                )?;
                base.annotations.push(NativeCaption {
                    id: next_id,
                    position,
                    text: text.text,
                    format: text.format,
                });
            } else if node.has_tag_name("arrow")
                || node.has_tag_name("curve")
                    && ["ArrowheadHead", "ArrowheadTail"].iter().any(|key| {
                        !matches!(
                            node.attribute(*key).unwrap_or("None"),
                            "None" | "Unspecified"
                        )
                    })
            {
                base.arrows.push(arrows.read(
                    ordinal,
                    prepared.source_scale,
                    &prepared.palette.colors,
                    next_id,
                )?);
            } else {
                continue;
            }
            objects.put(ordinal, vec![next_id])?;
            next_id = next_id.checked_add(1).ok_or(SceneError::Limit)?;
        }
    }
    let association = PreparedAtoms::new(
        &prepared.molecule.state.graph.atoms,
        &prepared.molecule.positions,
    )?;
    let mut tree = Tree::parse_import(&prepared.expanded_xml)?;
    let order = tree.descendants(0)?;
    let source_by_index: HashMap<_, _> = order
        .iter()
        .copied()
        .enumerate()
        .map(|(ordinal, index)| (index, ordinal))
        .collect();
    let synthetic = presentation::parse("<t><s/></t>")?;
    let mut slots = HashMap::new();
    for (ordinal, node) in nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.has_tag_name("n"))
    {
        let label = node.children().find(|n| n.has_tag_name("t"));
        if label.is_none()
            && !["LabelFont", "LabelSize", "LabelFace", "LabelColor"]
                .iter()
                .any(|key| node.attribute(*key).is_some())
        {
            continue;
        }
        let text = reader.read(
            label.unwrap_or(synthetic.root_element()),
            Some(&attributes(*node)),
            true,
        )?;
        let style = plain(text.format.style);
        if text
            .format
            .spans
            .into_iter()
            .any(|span| plain(span.style) != style)
        {
            return Err(SceneError::Invalid(
                "Mixed fonts within a CDXML atom label are not supported yet",
            ));
        }
        if style == default_style() {
            continue;
        }
        let p = point(
            node.attribute("p").ok_or(SceneError::Missing("p"))?,
            prepared.source_scale,
        )?;
        let id = association.identify_with_tolerance(
            &tree,
            *order.get(ordinal).ok_or(SceneError::Limit)?,
            p,
            0.01,
            "Could not safely associate CDXML text style with its atom",
        )?;
        // Native appends styled patches; later marks/labels alter the FIRST
        // patch with this ID, while reconstruction takes the LAST patch.
        if base.atoms.len() >= 100_000 {
            return Err(SceneError::Limit);
        }
        slots.entry(id).or_insert(base.atoms.len());
        base.atoms.push(SceneAtom {
            id,
            text_style: Some(style),
            marks: Vec::new(),
            display: None,
        });
    }
    let marks = super::read_marks(&prepared.expanded_xml, &association, prepared.source_scale)?;
    for patch in marks.atoms {
        let slot = atom_slot(&mut base.atoms, &mut slots, patch.id)?;
        base.atoms
            .get_mut(slot)
            .ok_or(SceneError::Limit)?
            .marks
            .extend(patch.marks);
    }
    for patch in marks.objects {
        objects.put(patch.source, patch.atoms)?;
    }
    let endpoints = base
        .bonds
        .iter()
        .map(|b| [b.bond.a, b.bond.b])
        .collect::<Vec<_>>();
    let labels = super::read_labels(
        &prepared.expanded_xml,
        &association,
        &endpoints,
        prepared.source_scale,
        |ordinal, attrs| {
            reader.read(
                *nodes.get(ordinal).ok_or(presentation::Error::Limit)?,
                Some(attrs),
                false,
            )
        },
    )?;
    base.atom_labels = labels.atom_labels;
    for patch in labels.atoms {
        let slot = atom_slot(&mut base.atoms, &mut slots, patch.id)?;
        base.atoms.get_mut(slot).ok_or(SceneError::Limit)?.display = Some(patch.display);
    }
    for patch in labels.bonds {
        base.bonds
            .get_mut(patch.index)
            .ok_or(SceneError::Limit)?
            .indicator = Some(patch.indicator);
    }
    for patch in labels.objects {
        objects.put(patch.source, patch.atoms)?;
    }
    let removed = circles::remove(&mut tree, prepared)?;
    let remaining = tree.descendants(0)?;
    let old_to_new: HashMap<_, _> = remaining
        .iter()
        .enumerate()
        .map(|(new, index)| {
            source_by_index
                .get(index)
                .copied()
                .map(|old| (old, new))
                .ok_or(SceneError::Limit)
        })
        .collect::<Result<_>>()?;
    let new_to_old = remaining
        .iter()
        .map(|index| source_by_index.get(index).copied().ok_or(SceneError::Limit))
        .collect::<Result<Vec<_>>>()?;
    let filtered = tree.serialize()?;
    let filtered_source = presentation::parse(&filtered)?;
    for (source, fill) in ring_fills::read(&nodes, &order, &tree, prepared, &association)? {
        objects.put(source, fill.atoms.clone())?;
        base.ring_fills.push(fill);
    }
    let claimed: BTreeSet<_> = objects
        .entries
        .iter()
        .filter_map(|p| old_to_new.get(&p.source).copied())
        .collect();
    let graphics = graphics::read(
        filtered_source.root_element(),
        prepared.source_scale,
        next_id,
        &claimed,
    )?;
    for (source, id) in graphics.bindings {
        objects.put(*new_to_old.get(source).ok_or(SceneError::Limit)?, vec![id])?;
    }
    base.graphics = graphics.graphics;
    // Extend only original fragment bindings, in native dict insertion order.
    let fragments = objects
        .entries
        .iter()
        .filter_map(|p| {
            nodes
                .get(p.source)
                .filter(|n| n.has_tag_name("fragment"))
                .map(|_| p.source)
        })
        .collect::<Vec<_>>();
    for ordinal in fragments {
        let index = *order.get(ordinal).ok_or(SceneError::Limit)?;
        let mut ids = objects.get(ordinal).ok_or(SceneError::Limit)?.to_vec();
        for child in &tree.node(index)?.children {
            if let Some(mapped) = source_by_index
                .get(child)
                .and_then(|source| objects.get(*source))
            {
                tree.spend(mapped.len())?;
                ids.extend_from_slice(mapped);
            }
        }
        objects.put(ordinal, ids)?;
    }
    let translated = objects
        .entries
        .iter()
        .filter_map(|p| {
            old_to_new.get(&p.source).map(|&source| ObjectMapEntry {
                source,
                atoms: p.atoms.clone(),
            })
        })
        .collect::<Vec<_>>();
    let group_id = next_id
        .checked_add(u64::try_from(base.graphics.len()).map_err(|_| SceneError::Limit)?)
        .ok_or(SceneError::Limit)?;
    base.groups = super::read_groups(&filtered, &translated, group_id)?;
    base.abbreviations = super::read_abbreviations(
        &prepared.abbreviations,
        &filtered,
        &prepared.fragments,
        &prepared.molecule.positions,
        prepared.source_scale,
    )?;
    if prepared.molecule.ids.is_empty()
        && base.annotations.is_empty()
        && base.arrows.is_empty()
        && base.graphics.is_empty()
    {
        return Err(SceneError::Invalid("No supported drawing objects found"));
    }
    Ok(CdxmlScene {
        attachments: prepared.attachments.clone(),
        molecule: prepared.molecule.clone(),
        conformer_3d: prepared.conformer_3d,
        base,
        objects: objects.entries,
        removed_sources: removed
            .into_iter()
            .map(|index| {
                source_by_index
                    .get(&index)
                    .copied()
                    .ok_or(SceneError::Limit)
            })
            .collect::<Result<_>>()?,
    })
}
