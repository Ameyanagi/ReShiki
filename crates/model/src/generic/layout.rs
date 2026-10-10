//! Coordinate-only layout for newly materialized link/repeat components.
use super::{Work, drawing_size};
use crate::{
    chemistry::{depict, document::Molecule},
    document::{Document, Point},
};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

#[cfg(test)]
thread_local! {
    static STAGES: std::cell::Cell<[usize; 5]> = const { std::cell::Cell::new([0; 5]) };
}
#[cfg(test)]
pub(super) fn entered(stage: usize) {
    STAGES.with(|s| {
        let mut counts = s.get();
        counts[stage] += 1;
        s.set(counts);
    });
}

/// Owned presentation buffers are copied by every materializer, not merely
/// measured after enumeration. Inspect lengths without serializing or cloning.
pub(super) struct Presentation {
    copy: usize,
    atom: usize,
    bond: usize,
    named_members: usize,
    names: usize,
    attached_ends: usize,
}
/// Counts for the current tile and the accumulated prepared drawing.
pub(super) struct AssemblyCounts {
    pub tile_atoms: usize,
    pub tile_size: usize,
    pub tiles: usize,
    pub total_atoms: usize,
    pub total_size: usize,
}
impl Presentation {
    pub(super) fn inspect(doc: &Document, work: &mut Work) -> Result<Self, String> {
        fn records<T>(n: usize) -> usize {
            n.saturating_mul(std::mem::size_of::<T>())
        }
        fn style(s: &crate::typography::TextStyle) -> usize {
            s.family.len()
        }
        fn optional_style(s: Option<&crate::typography::TextStyle>) -> usize {
            s.map_or(0, style)
        }
        fn map_cost(
            map: &std::collections::BTreeMap<String, crate::theme_files::ColorValue>,
            work: &mut Work,
        ) -> Result<usize, String> {
            // Include the partially occupied root and spare node slots, as
            // well as owned keys; counting only live pairs underbounds cloning.
            let mut size = if map.is_empty() { 0usize } else { 1024 };
            for key in map.keys() {
                work.charge(1)?;
                size = size.saturating_add(key.len()).saturating_add(
                    3 * std::mem::size_of::<(String, crate::theme_files::ColorValue)>()
                        + 3 * std::mem::size_of::<usize>(),
                );
            }
            Ok(size)
        }
        let mut copy = std::mem::size_of::<Document>()
            .saturating_add(doc.drawing_style.name.len())
            .saturating_add(doc.drawing_style.font_family.len());
        let mut atom = std::mem::size_of::<crate::document::Atom>() + 128;
        let mut bond = std::mem::size_of::<crate::document::Bond>() + 128;
        let mut retained_pictures = HashSet::new();
        let mut named_members = 0usize;
        let mut attached_ends = 0usize;
        for a in &doc.atoms {
            work.charge(1)?;
            let mut size = std::mem::size_of::<crate::document::Atom>()
                .saturating_add(a.element.len())
                .saturating_add(a.cip_label.as_ref().map_or(0, String::len))
                .saturating_add(records::<u64>(a.centroid.len()))
                .saturating_add(records::<crate::scientific::AtomMark>(a.marks.len()))
                .saturating_add(optional_style(a.text_style.as_ref()))
                .saturating_add(style(&a.display.stereo.style))
                .saturating_add(style(&a.display.mapping.style))
                .saturating_add(a.display.variable.as_ref().map_or(0, String::len));
            if let Some(s) = &a.stereo {
                size = size
                    .saturating_add(s.winding.len())
                    .saturating_add(records::<u64>(s.neighbors.len()));
            }
            if let Some(n) = &a.display.number {
                size = size
                    .saturating_add(n.text.len())
                    .saturating_add(style(&n.style));
            }
            copy = copy.saturating_add(size);
            atom = atom.max(size);
        }
        for b in &doc.bonds {
            work.charge(1)?;
            let size = std::mem::size_of::<crate::document::Bond>()
                .saturating_add(b.display.len())
                .saturating_add(b.stereo.as_ref().map_or(0, String::len))
                .saturating_add(b.cip_label.as_ref().map_or(0, String::len))
                .saturating_add(b.secondary_display.as_ref().map_or(0, String::len))
                .saturating_add(records::<u64>(b.stereo_atoms.len()))
                .saturating_add(style(&b.indicator.style));
            copy = copy.saturating_add(size);
            bond = bond.max(size);
        }
        for a in &doc.annotations {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<crate::document::Annotation>())
                .saturating_add(a.text.len())
                .saturating_add(style(&a.format.style))
                .saturating_add(records::<crate::typography::TextSpan>(a.format.spans.len()));
            for span in &a.format.spans {
                work.charge(1)?;
                copy = copy.saturating_add(style(&span.style));
            }
        }
        for a in &doc.arrows {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<crate::document::Arrow>())
                .saturating_add(a.kind.len());
            // Cubic controls and attachment targets/offsets/directions are inline.
            attached_ends = attached_ends
                .saturating_add(usize::from(a.start_anchor.is_some()))
                .saturating_add(usize::from(a.end_anchor.is_some()));
        }
        for name in &doc.molecule_names {
            work.charge(1)?;
            named_members = named_members.saturating_add(name.atoms.len());
            copy = copy
                .saturating_add(std::mem::size_of::<crate::molecule_names::MoleculeName>())
                .saturating_add(records::<u64>(name.atoms.len()))
                .saturating_add(name.smiles.len())
                .saturating_add(name.text.len());
        }
        for g in &doc.graphics {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<crate::graphics::Graphic>())
                .saturating_add(records::<crate::graphics::PathCommand>(g.path.len()));
            // PNG preview and retained EMF share immutable Arc storage. An exact
            // shared byte-buffer pair is charged once for this source inspection;
            // subsequent candidate copies contain only the Arc record. Pointer
            // identities are borrowed storage keys, never dereferenced or persisted.
            if let Some(p) = &g.picture {
                let png = p.png();
                let emf = p.emf().map_or((0, 0), |b| (b.as_ptr() as usize, b.len()));
                if retained_pictures.insert((png.as_ptr() as usize, png.len(), emf.0, emf.1)) {
                    work.charge(p.stored_bytes())?;
                }
            }
        }
        for g in &doc.groups {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<crate::grouping::Group>())
                .saturating_add(records::<u64>(g.members.len()));
        }
        for g in &doc.abbreviations {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<crate::abbreviations::Abbreviation>())
                .saturating_add(g.label.len())
                .saturating_add(g.reverse_label.len())
                .saturating_add(optional_style(g.label_style.as_ref()))
                .saturating_add(records::<u64>(g.members.len()));
        }
        for f in &doc.ring_fills {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<crate::ring_fills::RingFill>())
                .saturating_add(records::<u64>(f.atoms.len()));
        }
        for d in &doc.depth_appearance {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<crate::depth_appearance::Scope>())
                .saturating_add(records::<u64>(d.atoms.len()))
                .saturating_add(
                    d.weights
                        .len()
                        .saturating_add(d.overrides.len())
                        .saturating_add(d.rear_weights.len())
                        .saturating_add(d.rear_overrides.len())
                        .saturating_mul(64),
                );
            for map in [&d.weights, &d.overrides, &d.rear_weights, &d.rear_overrides] {
                if !map.is_empty() {
                    copy = copy.saturating_add(256);
                }
            }
        }
        for r in &doc.reactions {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<crate::reactions::Reaction>())
                .saturating_add(records::<u64>(r.annotations.len()));
            for p in r.reactants.iter().chain(&r.products).chain(&r.agents) {
                work.charge(1)?;
                copy = copy
                    .saturating_add(std::mem::size_of::<crate::reactions::Participant>())
                    .saturating_add(records::<u64>(p.atoms.len()));
            }
        }
        copy = copy
            .saturating_add(records::<crate::color_contrast::Rgb>(
                doc.recent_colors.len(),
            ))
            .saturating_add(
                doc.page_layout
                    .as_ref()
                    .map_or(0, |_| std::mem::size_of::<crate::pages::Layout>()),
            );
        for s in &doc.generic.alternatives {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<super::AlternativeSite>())
                .saturating_add(records::<super::Alternative>(s.choices.len()));
            for c in &s.choices {
                work.charge(1)?;
                copy = copy.saturating_add(match c {
                    super::Alternative::Element { symbol, .. } => symbol.len(),
                    super::Alternative::Group { label }
                    | super::Alternative::Unresolved { label } => label.len(),
                });
            }
        }
        copy = copy.saturating_add(records::<super::LinkNode>(doc.generic.links.len()));
        for r in &doc.generic.repeats {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<super::RepeatUnit>())
                .saturating_add(r.usage.len())
                .saturating_add(r.label.len())
                .saturating_add(r.flip.len())
                .saturating_add(records::<u64>(
                    r.bracket_objects
                        .len()
                        .saturating_add(r.members.len())
                        .saturating_add(r.crossing_brackets.len()),
                ))
                .saturating_add(records::<super::Crossing>(r.crossings.len()));
            if let super::RepeatCount::Indefinite { label } = &r.count {
                copy = copy.saturating_add(label.len());
            }
        }
        if let Some(t) = &doc.custom_theme {
            copy = copy
                .saturating_add(std::mem::size_of::<crate::theme_files::ThemeFile>())
                .saturating_add(t.id.len())
                .saturating_add(t.name.len())
                .saturating_add(t.author.len())
                .saturating_add(t.source.len())
                .saturating_add(t.license.len());
            for p in [&t.light, &t.dark] {
                for map in [&p.elements, &p.tile_seeds, &p.ring_fills] {
                    copy = copy.saturating_add(map_cost(map, work)?);
                }
            }
            if let Some(reference) = t.generator.as_ref().and_then(|g| g.reference.as_ref()) {
                copy = copy
                    .saturating_add(std::mem::size_of::<crate::theme_generator::Reference>())
                    .saturating_add(reference.id.len())
                    .saturating_add(reference.name.len())
                    .saturating_add(reference.author.len())
                    .saturating_add(reference.source.len())
                    .saturating_add(reference.license.len());
                for map in [
                    &reference.light,
                    &reference.dark,
                    &reference.light_tile_seeds,
                    &reference.dark_tile_seeds,
                ] {
                    copy = copy.saturating_add(map_cost(map, work)?);
                }
            }
        }
        // Even source validation may clone a generator-reference palette.
        let result = Self {
            copy,
            atom,
            bond,
            named_members,
            names: doc.molecule_names.len(),
            attached_ends,
        };
        work.charge(copy.saturating_add(result.reference_work(
            doc,
            doc.atoms.len(),
            drawing_size(doc),
        )))?;
        Ok(result)
    }
    pub(super) fn reserve(
        &self,
        source: &Document,
        upper_atoms: usize,
        upper_size: usize,
        copies: usize,
        work: &mut Work,
    ) -> Result<(), String> {
        let grown = self
            .copy
            .saturating_add(
                upper_atoms
                    .saturating_sub(source.atoms.len())
                    .saturating_mul(self.atom),
            )
            .saturating_add(
                upper_size
                    .saturating_sub(super::drawing_size(source))
                    .saturating_mul(self.bond),
            );
        // Names/anchors use linear endpoint searches in native validation. Pay
        // their maximum grown-candidate cost before any Cartesian materializer.
        work.charge(
            grown
                .saturating_add(self.reference_work(source, upper_atoms, upper_size))
                .saturating_mul(copies),
        )
    }
    pub(super) fn charge_input(
        &self,
        source: &Document,
        selected: &[u64],
        work: &mut Work,
    ) -> Result<(), String> {
        let size = drawing_size(source);
        let refs = source
            .generic
            .repeats
            .iter()
            .map(|r| {
                r.members
                    .len()
                    .saturating_add(r.crossings.len().saturating_mul(2))
                    .saturating_add(r.bracket_objects.len())
            })
            .chain(source.atoms.iter().map(|a| a.centroid.len()))
            .chain(source.groups.iter().map(|g| g.members.len()))
            .chain(source.abbreviations.iter().map(|g| g.members.len()))
            .fold(0usize, usize::saturating_add);
        // Pay owned snapshot buffers before the first clone. Selected inputs
        // additionally pay bounded closure/validation/dependency scans. The
        // strict selection never calls delete, identity, anchor reconciliation
        // or automatic rear-weight materialization.
        let selection = if selected.is_empty() {
            0
        } else {
            size.saturating_add(refs)
                .saturating_add(selected.len())
                .saturating_mul(
                    size.saturating_add(refs)
                        .saturating_add(selected.len())
                        .saturating_add(32),
                )
                .saturating_mul(32)
        };
        work.charge(self.copy.saturating_mul(2).saturating_add(selection))
    }
    pub(super) fn charge_measurement(
        &self,
        source: &Document,
        tile: &Document,
        work: &mut Work,
    ) -> Result<(), String> {
        let bytes = self
            .copy
            .saturating_add(
                tile.atoms
                    .len()
                    .saturating_sub(source.atoms.len())
                    .saturating_mul(self.atom),
            )
            .saturating_add(
                drawing_size(tile)
                    .saturating_sub(drawing_size(source))
                    .saturating_mul(self.bond),
            );
        // Actual detached measurement/normalization uses one preview snapshot;
        // charge two full owned records conservatively before any clone.
        work.charge(bytes.saturating_mul(2).saturating_add(self.reference_work(
            source,
            tile.atoms.len(),
            drawing_size(tile),
        )))?;
        charge_ink_work(tile, work)
    }
    pub(super) fn charge_assembly(
        &self,
        source: &Document,
        counts: AssemblyCounts,
        work: &mut Work,
    ) -> Result<(), String> {
        let AssemblyCounts {
            tile_atoms,
            tile_size,
            tiles,
            total_atoms,
            total_size,
        } = counts;
        let tile = self
            .copy
            .saturating_add(
                tile_atoms
                    .saturating_sub(source.atoms.len())
                    .saturating_mul(self.atom),
            )
            .saturating_add(
                tile_size
                    .saturating_sub(drawing_size(source))
                    .saturating_mul(self.bond),
            );
        let owned = tile.saturating_mul(tiles);
        let refs = self
            .named_members
            .saturating_mul(tiles)
            .saturating_mul(total_atoms.saturating_add(1))
            .saturating_add(
                self.names.saturating_mul(tiles).saturating_mul(
                    source
                        .annotations
                        .len()
                        .saturating_mul(tiles)
                        .saturating_add(1),
                ),
            )
            .saturating_add(
                self.attached_ends.saturating_mul(tiles).saturating_mul(
                    total_atoms
                        .saturating_mul(2)
                        .saturating_add(total_size)
                        .saturating_add(32),
                ),
            );
        // Tile remap, inverse snapshot, inherited-atom/bond comparisons, then
        // accumulated native validation. The cumulative result is charged at
        // every append; no fictitious constant-time or per-product reset.
        work.charge(
            tile.saturating_mul(6)
                .saturating_add(owned.saturating_mul(2))
                .saturating_add(refs.saturating_mul(32))
                .saturating_add(self.names.saturating_mul(tile_size).saturating_mul(32))
                .saturating_add(total_atoms.saturating_mul(total_size).saturating_mul(8))
                .saturating_add(total_size.saturating_mul(64)),
        )
    }
    pub(super) fn charge_publication(
        &self,
        source: &Document,
        tiles: usize,
        atoms: usize,
        size: usize,
        work: &mut Work,
    ) -> Result<(), String> {
        let bits = usize::BITS as usize - atoms.max(1).leading_zeros() as usize;
        // Empty-tab observer/history bookkeeping is a linear scan plus sorting
        // changed atom IDs. Palette/default snapshots copy owned global data.
        // There are no name identities, annotation layout or curve bounds here.
        work.charge(
            self.copy
                .saturating_mul(tiles)
                .saturating_mul(4)
                .saturating_add(atoms.saturating_mul(bits).saturating_mul(32))
                .saturating_add(size.saturating_mul(128))
                .saturating_add(self.reference_work(source, atoms, size)),
        )
    }
    fn reference_work(&self, source: &Document, atoms: usize, size: usize) -> usize {
        self.named_members
            .saturating_mul(atoms.saturating_add(1))
            .saturating_add(
                self.names
                    .saturating_mul(source.annotations.len().saturating_add(1)),
            )
            .saturating_add(
                self.attached_ends.saturating_mul(
                    atoms
                        .saturating_mul(2)
                        .saturating_add(size)
                        .saturating_add(32),
                ),
            )
            .saturating_mul(16)
    }
}

/// Native captions and attached endpoints cannot silently survive chemical or
/// moved-component changes. This structural dependency check does not verify
/// arbitrary saved name/SMILES claims or invoke name/identifier generation.
pub(super) fn protect_references(doc: &Document, work: &mut Work) -> Result<(), String> {
    let members = doc
        .molecule_names
        .iter()
        .map(|n| n.atoms.len())
        .chain(doc.depth_appearance.iter().map(|s| s.atoms.len()))
        .chain(doc.atoms.iter().map(|a| a.centroid.len()))
        .chain(doc.generic.repeats.iter().map(|r| {
            r.members
                .len()
                .saturating_add(r.crossings.len().saturating_mul(2))
        }))
        .fold(0usize, usize::saturating_add);
    work.charge(
        drawing_size(doc)
            .saturating_add(members)
            .saturating_add(doc.arrows.len().saturating_mul(2))
            .saturating_mul(32),
    )?;
    let index: HashMap<_, _> = doc
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect();
    let mut adjacent = vec![Vec::new(); doc.atoms.len()];
    for b in &doc.bonds {
        let a = *index
            .get(&b.a)
            .ok_or("Missing source dependency endpoint")?;
        let z = *index
            .get(&b.b)
            .ok_or("Missing source dependency endpoint")?;
        at_mut(&mut adjacent, a)?.push(z);
        at_mut(&mut adjacent, z)?.push(a);
    }
    let mut seeds: HashSet<_> = doc
        .generic
        .alternatives
        .iter()
        .map(|s| s.atom)
        .chain(doc.generic.links.iter().map(|s| s.atom))
        .chain(doc.generic.repeats.iter().flat_map(|r| {
            r.members
                .iter()
                .copied()
                .chain(r.crossings.iter().flat_map(|c| [c.inside, c.outside]))
        }))
        .collect();
    for a in doc
        .atoms
        .iter()
        .filter(|a| a.attachment == Some(crate::attachments::Kind::Variable))
    {
        seeds.insert(a.id);
        seeds.extend(a.centroid.iter().copied());
    }
    let mut labels = vec![usize::MAX; doc.atoms.len()];
    let mut sizes = Vec::new();
    let mut changed = vec![false; doc.atoms.len()];
    for start in 0..doc.atoms.len() {
        if *at(&labels, start)? != usize::MAX {
            continue;
        }
        let label = sizes.len();
        let mut pending = vec![start];
        let mut component = Vec::new();
        *at_mut(&mut labels, start)? = label;
        while let Some(i) = pending.pop() {
            component.push(i);
            for &j in at(&adjacent, i)? {
                if *at(&labels, j)? == usize::MAX {
                    *at_mut(&mut labels, j)? = label;
                    pending.push(j);
                }
            }
        }
        let touched = component
            .iter()
            .any(|&i| doc.atoms.get(i).is_some_and(|a| seeds.contains(&a.id)));
        for &i in &component {
            *at_mut(&mut changed, i)? = touched;
        }
        sizes.push(component.len());
    }
    for name in &doc.molecule_names {
        let first = *index
            .get(name.atoms.first().ok_or("Empty named component")?)
            .ok_or("Missing named atom")?;
        let label = *at(&labels, first)?;
        if name.atoms.len() != *at(&sizes, label)?
            || name
                .atoms
                .iter()
                .any(|id| index.get(id).is_none_or(|&i| labels.get(i) != Some(&label)))
        {
            return Err("Generated drawings require complete inherited named components; the source drawing is retained".into());
        }
        if doc
            .annotations
            .iter()
            .find(|a| a.id == name.annotation)
            .is_none_or(|a| a.text != name.text)
        {
            return Err("Inherited generated-caption text does not match its association; the source drawing is retained".into());
        }
        if *at(&changed, first)? {
            return Err(reference_error("generated name captions"));
        }
    }
    let changed_ids: HashSet<_> = doc
        .atoms
        .iter()
        .zip(&changed)
        .filter(|(_, flag)| **flag)
        .map(|(a, _)| a.id)
        .collect();
    if doc
        .arrows
        .iter()
        .flat_map(|a| a.start_anchor.iter().chain(a.end_anchor.iter()))
        .any(|anchor| anchor_touches(anchor, &changed_ids))
    {
        return Err(reference_error("attached arrows"));
    }
    // No clone-membership/frozen-weight policy exists in current materializers.
    // Retain rather than silently create unpainted replicas or widen a scope.
    if doc
        .depth_appearance
        .iter()
        .any(|s| s.atoms.iter().any(|id| changed_ids.contains(id)))
    {
        return Err(reference_error("depth appearance scopes"));
    }
    Ok(())
}
fn anchor_touches(anchor: &crate::arrow_anchors::Anchor, ids: &HashSet<u64>) -> bool {
    use crate::arrow_anchors::Target;
    match anchor.target {
        Target::Atom { atom } | Target::LonePair { atom, .. } => ids.contains(&atom),
        Target::Bond { a, b, .. } => ids.contains(&a) || ids.contains(&b),
    }
}
fn reference_error(kind: &str) -> String {
    format!(
        "Generic drawing expansion involving {kind} on a changed component is unsupported; the source drawing is retained"
    )
}

pub(super) struct Plan {
    adjacent: Vec<Vec<usize>>,
    components: Vec<Vec<usize>>,
    affected: Vec<bool>,
    indices: HashMap<u64, usize>,
}

fn charge_ink_work(doc: &Document, work: &mut Work) -> Result<(), String> {
    let n = doc.atoms.len();
    let size = drawing_size(doc);
    // Bound saved + refreshed ink and both native-state anchor passes. This
    // remains a debit of the caller's aggregate counter, never a fresh budget.
    let mut reserve = |cost: usize| work.charge(cost.saturating_mul(4));
    // Reserve indexing, detached snapshots, restoration/equality checks and
    // all atom-label lookups/pair checks before preparation or copying. The
    // existing label helper scans the document and neighbors; it is not O(1).
    reserve(
        size.saturating_mul(32).saturating_add(
            n.saturating_mul(size.saturating_add(n).saturating_add(128))
                .saturating_mul(32),
        ),
    )?;
    // Paint bounds reuse measured label boxes. Highlight/abbreviation
    // lookups and the final bounded envelope pass share this allowance;
    // no renderer, curve flattening or unmetered join construction runs.
    reserve(
        n.saturating_mul(doc.abbreviations.len().saturating_add(1))
            .saturating_mul(8),
    )?;
    for a in &doc.atoms {
        let bytes = a.element.len()
            + a.cip_label.as_ref().map_or(0, String::len)
            + a.display.variable.as_ref().map_or(0, String::len)
            + a.display.number.as_ref().map_or(0, |v| v.text.len())
            + a.marks.len().saturating_mul(512);
        let number_chars = a.display.number.as_ref().map_or(0, |v| v.text.len());
        let fonts = a
            .text_style
            .as_ref()
            .map_or(doc.drawing_style.font_family.len(), |s| s.family.len())
            .saturating_mul(64)
            .saturating_add(a.display.mapping.style.family.len().saturating_mul(10))
            .saturating_add(
                a.display.stereo.style.family.len().saturating_mul(
                    a.cip_label
                        .as_ref()
                        .map_or(0, |v| v.len().saturating_add(2))
                        .saturating_add(FUTURE_CIP_LABELS.len().saturating_mul(3)),
                ),
            )
            .saturating_add(
                a.display
                    .number
                    .as_ref()
                    .map_or(0, |v| v.style.family.len().saturating_mul(number_chars)),
            );
        reserve(
            bytes
                .saturating_add(128)
                .saturating_mul(32)
                .saturating_add(fonts.saturating_mul(8)),
        )?;
    }
    for b in &doc.bonds {
        // Fresh labels can restore displayed descriptors on an unrelated fixed
        // bond after installation. Pay their finite glyph/font envelope before
        // measurement; assigning CIP or preparing a second Molecule is absent.
        let labels = b
            .cip_label
            .as_ref()
            .map_or(0, |v| v.len().saturating_add(2))
            .saturating_add(FUTURE_CIP_LABELS.len().saturating_mul(3));
        reserve(
            labels.saturating_mul(32).saturating_add(
                labels
                    .saturating_mul(b.indicator.style.family.len())
                    .saturating_mul(8),
            ),
        )?;
    }
    for a in &doc.annotations {
        // Rich-text layout searches spans and can wrap at every glyph.
        reserve(
            a.text
                .len()
                .saturating_add(1)
                .saturating_mul(
                    a.text
                        .len()
                        .saturating_add(a.format.spans.len())
                        .saturating_add(1),
                )
                .saturating_mul(32),
        )?;
        let family = a
            .format
            .spans
            .iter()
            .map(|s| s.style.family.len())
            .max()
            .unwrap_or(0)
            .max(a.format.style.family.len());
        reserve(a.text.len().saturating_mul(family).saturating_mul(8))?;
    }
    for g in &doc.graphics {
        reserve(g.path.len().saturating_add(128).saturating_mul(8))?;
    }
    for g in &doc.abbreviations {
        reserve(
            g.label
                .len()
                .saturating_add(g.members.len())
                .saturating_mul(n.saturating_add(1))
                .saturating_mul(32),
        )?;
    }
    // Current arrow emission has <=2 shafts, fixed 16-step parallel curves
    // and fixed 32-step insets; no adaptive flattening. Target/label lookups
    // for the two native-state passes are included in the n*size reservation.
    // Pay the bounded command generation/control-hull pass before prepare.
    reserve(doc.arrows.len().saturating_mul(8192))?;
    Ok(())
}

impl Plan {
    pub(super) fn new(
        doc: &Document,
        anchors: &HashSet<u64>,
        work: &mut Work,
    ) -> Result<Self, String> {
        let n = doc.atoms.len();
        charge_ink_work(doc, work)?;
        let indices: HashMap<_, _> = doc
            .atoms
            .iter()
            .enumerate()
            .map(|(i, a)| (a.id, i))
            .collect();
        let mut adjacent = vec![Vec::new(); n];
        for b in &doc.bonds {
            let a = *indices.get(&b.a).ok_or("Missing generated bond endpoint")?;
            let z = *indices.get(&b.b).ok_or("Missing generated bond endpoint")?;
            at_mut(&mut adjacent, a)?.push(z);
            at_mut(&mut adjacent, z)?.push(a);
        }
        let mut done = vec![false; n];
        let mut affected = vec![false; n];
        let mut components = Vec::new();
        for start in 0..n {
            if *at(&done, start)? {
                continue;
            }
            let mut pending = vec![start];
            let mut component = Vec::new();
            *at_mut(&mut done, start)? = true;
            while let Some(i) = pending.pop() {
                component.push(i);
                for &j in at(&adjacent, i)? {
                    if !*at(&done, j)? {
                        *at_mut(&mut done, j)? = true;
                        pending.push(j);
                    }
                }
            }
            if component
                .iter()
                .any(|&i| doc.atoms.get(i).is_some_and(|a| anchors.contains(&a.id)))
            {
                for &i in &component {
                    *at_mut(&mut affected, i)? = true;
                }
                components.push(component);
            }
        }
        // An entire moved component is guarded: a remote stereocenter is not
        // safe merely because it is several bonds from the repeat definition.
        let affected_ids: HashSet<_> = doc
            .atoms
            .iter()
            .zip(&affected)
            .filter(|(_, flag)| **flag)
            .map(|(a, _)| a.id)
            .collect();
        if doc
            .atoms
            .iter()
            .any(|a| affected_ids.contains(&a.id) && a.stereo.is_some())
            || doc.bonds.iter().any(|b| {
                affected_ids.contains(&b.a)
                    && (b.stereo.is_some()
                        || b.stereo_authoritative
                        || !b.stereo_atoms.is_empty()
                        || !b.projection
                            && matches!(
                                b.display.as_str(),
                                "wedge" | "hollow_wedge" | "bold" | "hash" | "hashed" | "wavy"
                            ))
            })
        {
            return Err(stereo_error());
        }
        if doc
            .molecule_names
            .iter()
            .any(|n| n.atoms.iter().any(|id| affected_ids.contains(id)))
        {
            return Err(reference_error("generated name captions"));
        }
        if doc
            .arrows
            .iter()
            .flat_map(|a| a.start_anchor.iter().chain(a.end_anchor.iter()))
            .any(|anchor| anchor_touches(anchor, &affected_ids))
        {
            return Err(reference_error("attached arrows"));
        }
        if doc
            .depth_appearance
            .iter()
            .any(|s| s.atoms.iter().any(|id| affected_ids.contains(id)))
        {
            return Err(reference_error("depth appearance scopes"));
        }
        Ok(Self {
            adjacent,
            components,
            affected,
            indices,
        })
    }

    pub(super) fn finish(
        self,
        doc: &mut Document,
        molecule: &Molecule,
        work: &mut Work,
    ) -> Result<(Point, Point), String> {
        let n = doc.atoms.len();
        let state = &molecule.state;
        if molecule.ids.len() != n
            || molecule.positions.len() != n
            || state.graph.atoms.len() != n
            || state.valences.len() != n
            || state.metadata.atoms.len() != n
            || state.graph.bonds.len() != doc.bonds.len()
            || state.metadata.bonds.len() != doc.bonds.len()
            || molecule
                .ids
                .iter()
                .zip(&doc.atoms)
                .any(|(&id, a)| id != a.id)
        {
            return Err("Generated layout received mismatched chemical state".into());
        }
        if state
            .metadata
            .atoms
            .iter()
            .zip(&self.affected)
            .any(|(m, flag)| *flag && m.chiral_tag != 0)
        {
            return Err(stereo_error());
        }
        for (b, m) in state.graph.bonds.iter().zip(&state.metadata.bonds) {
            if *at(&self.affected, b.a)?
                && (m.stereo != 0 || m.unknown_stereo || !m.stereo_atoms.is_empty())
            {
                return Err(stereo_error());
            }
        }
        let before = doc.clone();
        if !self.components.is_empty() {
            work.charge(n.saturating_add(doc.bonds.len()).saturating_mul(8))?;
            let fixed: depict::geometry::Coordinates = doc
                .atoms
                .iter()
                .zip(&self.affected)
                .enumerate()
                .filter(|(_, (_, flag))| !**flag)
                .map(|(i, (a, _))| {
                    (
                        i,
                        depict::geometry::Point {
                            x: f64::from(a.position.x) / 28.,
                            y: -f64::from(a.position.y) / 28.,
                        },
                    )
                })
                .collect();
            let ranks = vec![None; n];
            #[cfg(test)]
            entered(1);
            let conformer = depict::compute_with_work(
                state,
                &ranks,
                (!fixed.is_empty()).then_some(&fixed),
                depict::Options {
                    bond_length: f64::from(doc.drawing_style.bond_length_world) / 28.,
                    canonical_orientation: true,
                    use_ring_templates: true,
                    work_limit: work.remaining,
                },
                &mut work.remaining,
            )
            .map_err(|e| format!("Generated structure layout failed: {e}"))?;
            if conformer.positions.len() != n {
                return Err("Generated layout lost atom coordinates".into());
            }
            for (i, p) in conformer.positions.iter().enumerate() {
                if *at(&self.affected, i)? {
                    let position = Point::new((p.x * 28.) as f32, (-p.y * 28.) as f32);
                    finite(position)?;
                    at_mut(&mut doc.atoms, i)?.position = position;
                }
            }
        }
        // Hydrogen refresh is measurement-only. Saved chemical fields and
        // display/cache preferences remain exactly those of the generated clone.
        let mut preview = doc.clone();
        for (i, a) in preview.atoms.iter_mut().enumerate() {
            a.label_h = u32::from(at(&state.graph.atoms, i)?.explicit_hydrogens)
                .checked_add(at(&state.valences, i)?.implicit_hydrogens)
                .ok_or("Generated hydrogen label overflow")?;
        }
        #[cfg(test)]
        entered(2);
        let mut boxes = saved_and_refreshed_boxes(doc, &preview)?;
        let margin = doc.drawing_style.world(doc.drawing_style.margin_width_pt);
        let bond_length = doc.drawing_style.bond_length_world;
        let gap = (bond_length * 0.4).max(margin * 2. + 1.);
        let edges: HashSet<_> = doc
            .bonds
            .iter()
            .map(|b| {
                Ok(super::pair(
                    *self.indices.get(&b.a).ok_or("Missing generated endpoint")? as u64,
                    *self.indices.get(&b.b).ok_or("Missing generated endpoint")? as u64,
                ))
            })
            .collect::<Result<_, String>>()?;
        for component in &self.components {
            let mut scale = 1_f32;
            for (k, &i) in component.iter().enumerate() {
                for &j in component.iter().skip(k + 1) {
                    let p = at(&doc.atoms, i)?.position;
                    let q = at(&doc.atoms, j)?.position;
                    let distance = p.distance(q);
                    if !distance.is_finite() || distance < 0.01 {
                        return Err("Generated layout has coincident atoms".into());
                    }
                    let ux = (q.x - p.x) / distance;
                    let uy = (q.y - p.y) / distance;
                    let clearance = if edges.contains(&super::pair(i as u64, j as u64)) {
                        gap
                    } else {
                        margin * 2. + 1.
                    };
                    scale = scale.max(
                        (reach(p, *at(&boxes, i)?, ux, uy)
                            + reach(q, *at(&boxes, j)?, -ux, -uy)
                            + clearance)
                            / distance,
                    );
                }
            }
            if !scale.is_finite() {
                return Err("Generated label clearance is nonfinite".into());
            }
            // One uniform enlargement keeps every chain angle/ring closure and
            // every forced or automatic H direction. No retry/cleanup loop.
            let center = at(
                &doc.atoms,
                *component.first().ok_or("Empty generated component")?,
            )?
            .position;
            for &i in component {
                let old = at(&doc.atoms, i)?.position;
                let new = Point::new(
                    center.x + (old.x - center.x) * scale,
                    center.y + (old.y - center.y) * scale,
                );
                finite(new)?;
                move_box(at_mut(&mut boxes, i)?, old, new);
                at_mut(&mut doc.atoms, i)?.position = new;
            }
        }
        // Fixed disconnected molecules/annotations stay put. Place each moved
        // component after their actual label/object extents, then after the
        // previous moved component; coordinate-only translation is bounded.
        let mut occupied = object_bounds(doc)?;
        for (i, a) in doc.atoms.iter().enumerate() {
            if !*at(&self.affected, i)? {
                include(&mut occupied, a.position, a.position);
                if let Some((lo, hi)) = *at(&boxes, i)? {
                    include(&mut occupied, lo, hi);
                }
            }
        }
        for component in &self.components {
            let mut bounds = None;
            for &i in component {
                let p = at(&doc.atoms, i)?.position;
                include(&mut bounds, p, p);
                if let Some((lo, hi)) = *at(&boxes, i)? {
                    include(&mut bounds, lo, hi);
                }
            }
            let (lo, hi) = bounds.ok_or("Generated layout has an empty component")?;
            let shift = occupied.map_or(Point::default(), |(_, end)| {
                Point::new(end.x + bond_length - lo.x, 0.)
            });
            for &i in component {
                let old = at(&doc.atoms, i)?.position;
                let new = old.offset(shift.x, shift.y);
                finite(new)?;
                move_box(at_mut(&mut boxes, i)?, old, new);
                at_mut(&mut doc.atoms, i)?.position = new;
            }
            include(
                &mut occupied,
                lo.offset(shift.x, shift.y),
                hi.offset(shift.x, shift.y),
            );
        }
        for component in &self.components {
            for (k, &i) in component.iter().enumerate() {
                for &j in component.iter().skip(k + 1) {
                    let p = at(&doc.atoms, i)?.position;
                    let q = at(&doc.atoms, j)?.position;
                    let distance = p.distance(q);
                    let ux = (q.x - p.x) / distance;
                    let uy = (q.y - p.y) / distance;
                    let required = reach(p, *at(&boxes, i)?, ux, uy)
                        + reach(q, *at(&boxes, j)?, -ux, -uy)
                        + if edges.contains(&super::pair(i as u64, j as u64)) {
                            gap
                        } else {
                            margin * 2. + 1.
                        };
                    if distance + 0.01 < required {
                        return Err(
                            "Generated layout cannot clear the preserved atom labels".into()
                        );
                    }
                }
            }
        }
        // Ordinary hidden saturated carbons need a visible vertex. A solver
        // failure may not turn a multi-carbon repeat back into one long line.
        for (i, a) in doc.atoms.iter().enumerate() {
            let neighbors = at(&self.adjacent, i)?;
            if !*at(&self.affected, i)?
                || a.element != "C"
                || neighbors.len() != 2
                || crate::atom_labels::visible_with_degree(a, doc, || neighbors.len())
                || doc
                    .bonds
                    .iter()
                    .any(|b| (b.a == a.id || b.b == a.id) && b.order != 1)
            {
                continue;
            }
            let p = at(&doc.atoms, *at(neighbors, 0)?)?.position;
            let q = at(&doc.atoms, *at(neighbors, 1)?)?.position;
            let ux = p.x - a.position.x;
            let uy = p.y - a.position.y;
            let vx = q.x - a.position.x;
            let vy = q.y - a.position.y;
            let turn = (ux * vy - uy * vx).abs() / (ux.hypot(uy) * vx.hypot(vy));
            if !turn.is_finite() || turn < 0.15 {
                return Err("Generated layout hides a saturated carbon vertex".into());
            }
        }
        preserve(&before, doc, &self.affected)?;
        measured_bounds(doc, &boxes)
    }
}

fn measured_bounds(
    doc: &Document,
    boxes: &[Option<(Point, Point)>],
) -> Result<(Point, Point), String> {
    let mut bounds = object_bounds(doc)?;
    for (i, a) in doc.atoms.iter().enumerate() {
        include(&mut bounds, a.position, a.position);
        if let Some((lo, hi)) = *at(boxes, i)? {
            include(&mut bounds, lo, hi);
        }
        if crate::highlights::atom_color(doc, a).is_some() {
            let radius = highlight_radius(&doc.drawing_style);
            // These cached boxes contain the renderer's label/mark ink
            // (and may also contain indicators). A larger box only grows
            // its elliptical capsule; it cannot exclude the real pill.
            let (left, right, rx, ry) = if let Some((lo, hi)) = *at(boxes, i)? {
                let ry = (hi.y - lo.y + radius) * 0.5;
                let y = (lo.y + hi.y) * 0.5;
                (Point::new(lo.x, y), Point::new(hi.x, y), ry * 0.5, ry)
            } else {
                (a.position, a.position, radius, radius)
            };
            include(
                &mut bounds,
                left.offset(-rx * CAPSULE_SCALE, -ry * CAPSULE_SCALE),
                right.offset(rx * CAPSULE_SCALE, ry * CAPSULE_SCALE),
            );
        }
    }
    let (lo, hi) = bounds.ok_or("Expanded structure has no positioned ink")?;
    let pad = bond_paint_radius(&doc.drawing_style);
    let lo = lo.offset(-pad, -pad);
    let hi = hi.offset(pad, pad);
    finite(lo)?;
    finite(hi)?;
    Ok((lo, hi))
}

/// Normalize only generated detached arrow geometry, under refreshed labels.
/// This is the same native reconciliation rule, paid before the preview copy.
/// Arrow targets/styles and all other native fields remain unchanged. Endpoint,
/// neighboring control, offset and legacy quadratic-to-cubic normalization are
/// explicitly allowed generated-output changes; the live source stays intact.
pub(super) fn normalize_arrows(
    doc: &mut Document,
    hydrogens: &[u32],
    source: &Document,
    presentation: &Presentation,
    work: &mut Work,
) -> Result<(), String> {
    if doc
        .arrows
        .iter()
        .all(|a| a.start_anchor.is_none() && a.end_anchor.is_none())
    {
        return Ok(());
    }
    presentation.charge_measurement(source, doc, work)?;
    let mut preview = refreshed_preview(doc, hydrogens)?;
    normalize_native_states(doc, &mut preview)?;
    Ok(())
}
fn normalize_native_states(saved: &mut Document, refreshed: &mut Document) -> Result<(), String> {
    // Public from_json resolves the saved label_h before from_native_file
    // clears only CIP. A later label refresh may show different H ink. Take
    // outward clearance across both states, then prove neither can move again.
    crate::arrow_anchors::reconcile(refreshed);
    saved.arrows = std::mem::take(&mut refreshed.arrows);
    crate::arrow_anchors::reconcile(saved);
    crate::arrow_anchors::certify_resolved(saved)?;
    refreshed.arrows = saved.arrows.clone();
    crate::arrow_anchors::certify_resolved(refreshed)
}
fn saved_and_refreshed_boxes(
    saved: &Document,
    refreshed: &Document,
) -> Result<Vec<Option<(Point, Point)>>, String> {
    if saved.atoms.len() != refreshed.atoms.len() {
        return Err("Prepared label snapshots differ in size".into());
    }
    saved
        .atoms
        .iter()
        .zip(&refreshed.atoms)
        .map(|(a, b)| {
            let mut bounds = atom_bounds(a, saved)?;
            if let Some((lo, hi)) = atom_bounds(b, refreshed)? {
                include(&mut bounds, lo, hi);
            }
            Ok(bounds)
        })
        .collect()
}
fn refreshed_preview(doc: &Document, hydrogens: &[u32]) -> Result<Document, String> {
    if hydrogens.len() != doc.atoms.len() {
        return Err("Prepared tile lost its cached hydrogen state".into());
    }
    let mut preview = doc.clone();
    for (a, &h) in preview.atoms.iter_mut().zip(hydrogens) {
        a.label_h = h;
    }
    Ok(preview)
}
/// Bound actual installed-tile ink, after its actual f32 remap/translation and
/// refreshed native arrow normalization. No cached bounds are translated as a
/// substitute, and no molecule/identifier preparation runs again.
pub(super) fn measure_translated(
    doc: &mut Document,
    hydrogens: &[u32],
    source: &Document,
    presentation: &Presentation,
    work: &mut Work,
) -> Result<(Point, Point), String> {
    presentation.charge_measurement(source, doc, work)?;
    let mut preview = refreshed_preview(doc, hydrogens)?;
    normalize_native_states(doc, &mut preview)?;
    let boxes = saved_and_refreshed_boxes(doc, &preview)?;
    measured_bounds(doc, &boxes)
}

// The renderer uses four cubic circle segments. Their maximum radial
// overshoot is 0.0273%; highlights.rs reserves the same outward allowance.
const CAPSULE_SCALE: f32 = 1.0003;
fn highlight_radius(style: &crate::style::DrawingStyle) -> f32 {
    style.bond_length_world * style.bond_spacing_ratio * 1.5 + style.line_width() * 0.5
}
fn bond_paint_radius(style: &crate::style::DrawingStyle) -> f32 {
    let spacing = style.bond_length_world * style.bond_spacing_ratio;
    let bold = style.world(style.bold_width_pt);
    let line = style.line_width();
    // Quadruple rails are at most 1.5 spacings from their centerline, waves
    // reach 1.25 line widths, and Joins::cap clamps every miter to four half
    // widths. Bold width >= line width is a validated document invariant.
    // Junction far points stay along their incident segment with <=bold/2
    // transverse width, so the same endpoint/segment envelope contains them.
    let envelope = (spacing * 1.5 + bold * 0.5)
        .max(line * 1.25 + bold * 0.5)
        .max(bold * 2.);
    let highlight = highlight_radius(style).max(envelope + line) * CAPSULE_SCALE;
    // The dative arrowhead's fixed 10 by 3.5 world-unit wings also fit here.
    highlight.max(10_f32.hypot(3.5) + line)
}

fn stereo_error() -> String {
    "Generated link/repeat layout involving specified or perceived component stereochemistry is unsupported; the source drawing is retained".into()
}
fn finite(p: Point) -> Result<(), String> {
    if p.x.is_finite() && p.y.is_finite() {
        Ok(())
    } else {
        Err("Generated layout produced nonfinite coordinates".into())
    }
}
fn at<T>(values: &[T], i: usize) -> Result<&T, String> {
    values
        .get(i)
        .ok_or_else(|| "Generated layout index is outside the drawing".into())
}
fn at_mut<T>(values: &mut [T], i: usize) -> Result<&mut T, String> {
    values
        .get_mut(i)
        .ok_or_else(|| "Generated layout index is outside the drawing".into())
}
fn atom_bounds(
    a: &crate::document::Atom,
    doc: &Document,
) -> Result<Option<(Point, Point)>, String> {
    let mut bounds = crate::scene::atom_label_ink_bounds(a, doc);
    for part in crate::scientific::styled_mark_parts(a, &doc.drawing_style) {
        for p in part
            .commands
            .iter()
            .flat_map(crate::graphics::PathCommand::iter_points)
        {
            let pad = part.style.width() * 0.5;
            include(&mut bounds, p.offset(-pad, -pad), p.offset(pad, pad));
        }
    }
    if let Some(number) = &a.display.number {
        indicator_bounds(
            &mut bounds,
            a.position,
            &number.text,
            number.offset,
            &number.style,
        );
    }
    if a.map_num != 0 && a.display.mapping.show.unwrap_or(doc.atom_labels.maps) {
        indicator_bounds(
            &mut bounds,
            a.position,
            &a.map_num.to_string(),
            a.display.mapping.offset,
            &a.display.mapping.style,
        );
    }
    if a.display.stereo.show.unwrap_or(doc.atom_labels.stereo) {
        cip_bounds(
            &mut bounds,
            a.position,
            a.cip_label.as_deref(),
            a.display.stereo.offset,
            &a.display.stereo.style,
        );
    }
    if let Some((lo, hi)) = bounds {
        finite(lo)?;
        finite(hi)?;
    }
    Ok(bounds)
}
// Completed chemistry descriptors are the fixed single-character set emitted
// by stereo/cip/label.rs. Native refresh may restore them after caches are
// cleared, including on unrelated fixed components. Bound every enabled style
// without assigning CIP, inventing a label, or altering persisted caches.
const FUTURE_CIP_LABELS: [&str; 12] = [
    "(R)", "(S)", "(r)", "(s)", "(E)", "(Z)", "(e)", "(z)", "(M)", "(P)", "(m)", "(p)",
];
fn cip_bounds(
    bounds: &mut Option<(Point, Point)>,
    anchor: Point,
    saved: Option<&str>,
    offset: Option<Point>,
    style: &crate::typography::TextStyle,
) {
    if let Some(label) = saved {
        indicator_bounds(bounds, anchor, &format!("({label})"), offset, style);
    }
    for label in FUTURE_CIP_LABELS {
        indicator_bounds(bounds, anchor, label, offset, style);
    }
}
fn indicator_bounds(
    bounds: &mut Option<(Point, Point)>,
    anchor: Point,
    text: &str,
    offset: Option<Point>,
    style: &crate::typography::TextStyle,
) {
    let height = crate::style::DEFAULT.world(style.size_pt);
    let width = crate::style::styled_text_width(text, height, style);
    // Automatic indicator candidates have five fixed radii, ending at 3.25h+6.
    // Their conservative envelope avoids a second label-placement pass.
    let radius = offset.map_or(height * 3.25 + 6., |_| 0.);
    let center = offset.map_or(anchor, |p| anchor.offset(p.x, p.y));
    include(
        bounds,
        center.offset(-width / 2. - radius, -height / 2. - radius),
        center.offset(width / 2. + radius, height / 2. + radius),
    );
    // Actual font ink may overhang its logical advance (italic/underline).
    // Automatic placement can choose any of the fixed candidate centers; grow
    // every actual ink box by the same maximum radius in each direction.
    let origin = center.offset(-width / 2., -height / 2.);
    for (lo, hi) in crate::style::text_ink_boxes(text, height, style) {
        include(
            bounds,
            origin.offset(lo.x - radius, lo.y - radius),
            origin.offset(hi.x + radius, hi.y + radius),
        );
    }
}
fn reach(p: Point, bounds: Option<(Point, Point)>, ux: f32, uy: f32) -> f32 {
    // Projection of the positioned ink toward a neighbor. Using its direction
    // avoids needlessly lengthening bonds for hydrogens on the opposite side.
    bounds.map_or(0., |(lo, hi)| {
        let x = if ux >= 0. { hi.x } else { lo.x };
        let y = if uy >= 0. { hi.y } else { lo.y };
        ((x - p.x) * ux + (y - p.y) * uy).max(0.)
    })
}
fn move_box(bounds: &mut Option<(Point, Point)>, old: Point, new: Point) {
    if let Some((lo, hi)) = bounds {
        *lo = lo.offset(new.x - old.x, new.y - old.y);
        *hi = hi.offset(new.x - old.x, new.y - old.y);
    }
}
fn include(bounds: &mut Option<(Point, Point)>, lo: Point, hi: Point) {
    *bounds = Some(bounds.map_or((lo, hi), |(a, b)| {
        (
            Point::new(a.x.min(lo.x), a.y.min(lo.y)),
            Point::new(b.x.max(hi.x), b.y.max(hi.y)),
        )
    }));
}
fn object_bounds(doc: &Document) -> Result<Option<(Point, Point)>, String> {
    let mut bounds = None;
    for a in &doc.annotations {
        // Exactly one shared typography layout: wrapping, rich scripts,
        // alignment and family fallback match annotation/caption painting.
        let layout = crate::typography::layout(&a.text, &a.format);
        let hi = a.position.offset(layout.width, layout.height);
        finite(hi)?;
        include(&mut bounds, a.position, hi);
        for fragment in &layout.fragments {
            let origin = a.position.offset(fragment.position.x, fragment.position.y);
            for (lo, hi) in
                crate::style::text_ink_boxes(&fragment.text, fragment.style.size(), &fragment.style)
            {
                let lo = origin.offset(lo.x, lo.y);
                let hi = origin.offset(hi.x, hi.y);
                finite(lo)?;
                finite(hi)?;
                include(&mut bounds, lo, hi);
            }
        }
    }
    for a in &doc.arrows {
        // Bound the actual emitted cubic/Hermite control hull and head/mark
        // paths, including preset defaults. Round stroke joins/caps fit half
        // their width. Calling Arrow::bounds would adaptively flatten curves.
        for part in a.paths() {
            let pad = part.style.width() * 0.5;
            for p in part
                .commands
                .iter()
                .flat_map(crate::graphics::PathCommand::iter_points)
            {
                let lo = p.offset(-pad, -pad);
                let hi = p.offset(pad, pad);
                finite(lo)?;
                finite(hi)?;
                include(&mut bounds, lo, hi);
            }
        }
    }
    for b in &doc.bonds {
        if b.indicator.show.unwrap_or(doc.atom_labels.stereo) {
            let a = doc.atom(b.a).ok_or("Missing indicator endpoint")?.position;
            let z = doc.atom(b.b).ok_or("Missing indicator endpoint")?.position;
            cip_bounds(
                &mut bounds,
                Point::new((a.x + z.x) * 0.5, (a.y + z.y) * 0.5),
                b.cip_label.as_deref(),
                b.indicator.offset,
                &b.indicator.style,
            );
        }
    }
    for g in &doc.graphics {
        let (lo, hi) = g.bounds();
        finite(lo)?;
        finite(hi)?;
        include(&mut bounds, lo, hi);
    }
    Ok(bounds)
}
fn preserve(before: &Document, after: &Document, affected: &[bool]) -> Result<(), String> {
    if before.atoms.len() != after.atoms.len()
        || before.bonds != after.bonds
        || affected.len() != after.atoms.len()
    {
        return Err("Generated layout changed the molecular graph".into());
    }
    let mut restored = after.clone();
    for (i, (a, original)) in restored.atoms.iter_mut().zip(&before.atoms).enumerate() {
        if a.id != original.id || !*at(affected, i)? && a.position != original.position {
            return Err("Generated layout moved an unrelated atom or changed its identity".into());
        }
        a.position = original.position;
        if a != original {
            return Err("Generated layout changed an atom's chemistry or presentation".into());
        }
    }
    if &restored != before {
        return Err("Generated layout changed noncoordinate drawing data".into());
    }
    Ok(())
}

/// Independent equality proof for the strict assembly seam. Restore IDs and
/// positions in a detached comparison copy; require the exact common shift
/// first, so restoration cannot hide a geometry or default-style mutation.
pub(super) fn certify_translation(
    source: &Document,
    part: &Document,
    mapping: &HashMap<u64, u64>,
    shift: Point,
) -> Result<(), String> {
    if source.atoms.len() != part.atoms.len()
        || source.bonds.len() != part.bonds.len()
        || source.annotations.len() != part.annotations.len()
        || source.arrows.len() != part.arrows.len()
        || source.graphics.len() != part.graphics.len()
    {
        return Err("Prepared tile changed object counts".into());
    }
    let inverse: HashMap<_, _> = mapping.iter().map(|(&a, &b)| (b, a)).collect();
    if inverse.len() != mapping.len() {
        return Err("Prepared tile has an ambiguous ID remap".into());
    }
    let undo = |id: &mut u64| -> Result<(), String> {
        *id = *inverse.get(id).ok_or("Prepared tile has an unmapped ID")?;
        Ok(())
    };
    let check_shift = |old: Point, new: Point| -> Result<(), String> {
        finite(new)?;
        let error_x = (f64::from(new.x) - f64::from(old.x) - f64::from(shift.x)).abs();
        let error_y = (f64::from(new.y) - f64::from(old.y) - f64::from(shift.y)).abs();
        if error_x > 0.01 || error_y > 0.01 {
            return Err("Prepared tile loses local geometry at these finite coordinates; the source drawing is retained".into());
        }
        Ok(())
    };
    let relative = |a: Point, b: Point, x: Point, y: Point| -> Result<(), String> {
        let dx = ((f64::from(y.x) - f64::from(x.x)) - (f64::from(b.x) - f64::from(a.x))).abs();
        let dy = ((f64::from(y.y) - f64::from(x.y)) - (f64::from(b.y) - f64::from(a.y))).abs();
        if dx > 0.01 || dy > 0.01 {
            return Err("Prepared tile loses local geometry at these finite coordinates; the source drawing is retained".into());
        }
        Ok(())
    };
    // Checking only old+shift can hide cancellation even in f64. Independently
    // compare local bond/control/caption vectors, before restoration or repair.
    for bond in &source.bonds {
        let a = source
            .atom(bond.a)
            .ok_or("Missing original shape endpoint")?;
        let b = source
            .atom(bond.b)
            .ok_or("Missing original shape endpoint")?;
        let x = part
            .atom(*mapping.get(&bond.a).ok_or("Missing shape remap")?)
            .ok_or("Missing translated shape endpoint")?;
        let y = part
            .atom(*mapping.get(&bond.b).ok_or("Missing shape remap")?)
            .ok_or("Missing translated shape endpoint")?;
        relative(a.position, b.position, x.position, y.position)?;
    }
    for (old, new) in source.arrows.iter().zip(&part.arrows) {
        relative(old.start, old.end, new.start, new.end)?;
        if let Some((a, b)) = old.control.zip(new.control) {
            relative(old.start, a, new.start, b)?;
        }
        if let Some((a, b)) = old.cubic.zip(new.cubic) {
            for (a, b) in a.into_iter().zip(b) {
                relative(old.start, a, new.start, b)?;
            }
        }
    }
    for name in &source.molecule_names {
        let atom = *name.atoms.first().ok_or("Empty named shape")?;
        let old = source
            .annotations
            .iter()
            .find(|a| a.id == name.annotation)
            .ok_or("Missing named shape caption")?;
        let new_id = *mapping
            .get(&name.annotation)
            .ok_or("Missing named shape remap")?;
        let new = part
            .annotations
            .iter()
            .find(|a| a.id == new_id)
            .ok_or("Missing translated name caption")?;
        let a = source.atom(atom).ok_or("Missing named shape atom")?;
        let z = part
            .atom(*mapping.get(&atom).ok_or("Missing named atom remap")?)
            .ok_or("Missing translated named atom")?;
        relative(a.position, old.position, z.position, new.position)?;
    }
    let mut restored = part.clone();
    for (actual, old) in restored.atoms.iter_mut().zip(&source.atoms) {
        check_shift(old.position, actual.position)?;
        if actual.position != old.position.offset(shift.x, shift.y) {
            return Err("Prepared atom lost its common translation".into());
        }
        undo(&mut actual.id)?;
        actual.position = old.position;
        for id in &mut actual.centroid {
            undo(id)?;
        }
        if let Some(stereo) = &mut actual.stereo {
            for id in &mut stereo.neighbors {
                undo(id)?;
            }
        }
    }
    for b in &mut restored.bonds {
        undo(&mut b.a)?;
        undo(&mut b.b)?;
        for id in &mut b.stereo_atoms {
            undo(id)?;
        }
    }
    for (actual, old) in restored.annotations.iter_mut().zip(&source.annotations) {
        check_shift(old.position, actual.position)?;
        if actual.position != old.position.offset(shift.x, shift.y) {
            return Err("Prepared annotation lost its common translation".into());
        }
        undo(&mut actual.id)?;
        actual.position = old.position;
    }
    for (actual, old) in restored.arrows.iter_mut().zip(&source.arrows) {
        check_shift(old.start, actual.start)?;
        check_shift(old.end, actual.end)?;
        if let Some((a, b)) = old.control.zip(actual.control) {
            check_shift(a, b)?;
        }
        if let Some((a, b)) = old.cubic.zip(actual.cubic) {
            for (a, b) in a.into_iter().zip(b) {
                check_shift(a, b)?;
            }
        }
        if actual.start != old.start.offset(shift.x, shift.y)
            || actual.end != old.end.offset(shift.x, shift.y)
            || actual.control != old.control.map(|p| p.offset(shift.x, shift.y))
            || actual.cubic != old.cubic.map(|v| v.map(|p| p.offset(shift.x, shift.y)))
        {
            return Err(
                "Prepared arrow changed its explicit controls or common translation".into(),
            );
        }
        for (new, prior) in [
            (&mut actual.start_anchor, &old.start_anchor),
            (&mut actual.end_anchor, &old.end_anchor),
        ] {
            match (new.as_mut(), prior.as_ref()) {
                (Some(new), Some(prior)) => {
                    use crate::arrow_anchors::Target;
                    let target = match prior.target {
                        Target::Atom { atom } => Target::Atom {
                            atom: *mapping.get(&atom).ok_or("Missing anchor remap")?,
                        },
                        Target::LonePair { atom, mark } => Target::LonePair {
                            atom: *mapping.get(&atom).ok_or("Missing anchor remap")?,
                            mark,
                        },
                        Target::Bond { a, b, fraction } => {
                            let x = *mapping.get(&a).ok_or("Missing anchor remap")?;
                            let y = *mapping.get(&b).ok_or("Missing anchor remap")?;
                            if x < y {
                                Target::Bond {
                                    a: x,
                                    b: y,
                                    fraction,
                                }
                            } else {
                                Target::Bond {
                                    a: y,
                                    b: x,
                                    fraction: 1. - fraction,
                                }
                            }
                        }
                    };
                    if new.target != target
                        || new.offset != prior.offset
                        || new.direction != prior.direction
                        || new.gap != prior.gap
                    {
                        return Err("Prepared tile changed stable arrow attachment metadata".into());
                    }
                    new.target = prior.target.clone();
                }
                (None, None) => {}
                _ => return Err("Prepared tile changed arrow attachment presence".into()),
            }
        }
        undo(&mut actual.id)?;
        actual.start = old.start;
        actual.end = old.end;
        actual.control = old.control;
        actual.cubic = old.cubic;
    }
    for (actual, old) in restored.graphics.iter_mut().zip(&source.graphics) {
        check_shift(old.origin, actual.origin)?;
        if actual.origin != old.origin.offset(shift.x, shift.y) {
            return Err("Prepared graphic lost its common translation".into());
        }
        undo(&mut actual.id)?;
        actual.origin = old.origin;
    }
    for g in &mut restored.groups {
        undo(&mut g.id)?;
        for id in &mut g.members {
            undo(id)?;
        }
    }
    for g in &mut restored.abbreviations {
        undo(&mut g.anchor)?;
        for id in &mut g.members {
            undo(id)?;
        }
    }
    for f in &mut restored.ring_fills {
        for id in &mut f.atoms {
            undo(id)?;
        }
    }
    for r in &mut restored.reactions {
        r.remap(&inverse).ok_or("Missing reaction remap")?;
    }
    for n in &mut restored.molecule_names {
        undo(&mut n.annotation)?;
        for id in &mut n.atoms {
            undo(id)?;
        }
    }
    restored.depth_appearance =
        crate::depth_appearance::remap(&restored.depth_appearance, &inverse);
    restored
        .generic
        .remap(&inverse)
        .ok_or("Missing generic remap")?;
    if &restored != source {
        return Err("Prepared tile changed noncoordinate native data".into());
    }
    Ok(())
}
