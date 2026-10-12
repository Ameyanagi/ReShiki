//! Coordinate-only depiction and bounded variable-substituent placement.
use super::{
    Work, drawing_size,
    probe::{self, Stage},
};
use crate::{
    chemistry::{depict, document::Molecule},
    document::{Document, Point},
};
use std::collections::{HashMap, HashSet};

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
        let Document {
            generic,
            canvas_theme: _,
            color_theme: _,
            custom_theme,
            drawing_style,
            page_layout,
            abbreviations,
            atom_labels: _,
            ring_fills,
            depth_appearance,
            version: _,
            atoms,
            bonds,
            annotations,
            molecule_names,
            arrows,
            graphics,
            groups,
            reactions,
            recent_colors,
        } = doc;
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
            .saturating_add(drawing_style.name.len())
            .saturating_add(drawing_style.font_family.len());
        let mut atom = std::mem::size_of::<crate::document::Atom>() + 128;
        let mut bond = std::mem::size_of::<crate::document::Bond>() + 128;
        let mut retained_pictures = HashSet::new();
        let mut named_members = 0usize;
        let mut attached_ends = 0usize;
        for a in atoms {
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
        for b in bonds {
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
        for a in annotations {
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
        for a in arrows {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<crate::document::Arrow>())
                .saturating_add(a.kind.len());
            // Cubic controls and attachment targets/offsets/directions are inline.
            attached_ends = attached_ends
                .saturating_add(usize::from(a.start_anchor.is_some()))
                .saturating_add(usize::from(a.end_anchor.is_some()));
        }
        for name in molecule_names {
            work.charge(1)?;
            named_members = named_members.saturating_add(name.atoms.len());
            copy = copy
                .saturating_add(std::mem::size_of::<crate::molecule_names::MoleculeName>())
                .saturating_add(records::<u64>(name.atoms.len()))
                .saturating_add(name.smiles.len())
                .saturating_add(name.text.len());
        }
        for g in graphics {
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
        for g in groups {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<crate::grouping::Group>())
                .saturating_add(records::<u64>(g.members.len()));
        }
        for g in abbreviations {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<crate::abbreviations::Abbreviation>())
                .saturating_add(g.label.len())
                .saturating_add(g.reverse_label.len())
                .saturating_add(optional_style(g.label_style.as_ref()))
                .saturating_add(records::<u64>(g.members.len()));
        }
        for f in ring_fills {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<crate::ring_fills::RingFill>())
                .saturating_add(records::<u64>(f.atoms.len()));
        }
        for d in depth_appearance {
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
            for map in d.member_maps() {
                if !map.is_empty() {
                    copy = copy.saturating_add(256);
                }
            }
        }
        for r in reactions {
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
            .saturating_add(records::<crate::color_contrast::Rgb>(recent_colors.len()))
            .saturating_add(
                page_layout
                    .as_ref()
                    .map_or(0, |_| std::mem::size_of::<crate::pages::Layout>()),
            );
        for s in &generic.alternatives {
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
        copy = copy.saturating_add(records::<super::LinkNode>(generic.links.len()));
        for r in &generic.repeats {
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
        if let Some(t) = custom_theme {
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
            names: molecule_names.len(),
            attached_ends,
        };
        work.charge(copy.saturating_add(result.reference_work(
            doc,
            atoms.len(),
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
    for a in doc.atoms.iter().filter(|a| a.is_variable_attachment()) {
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
    anchor.target.atoms().any(|id| ids.contains(&id))
}
fn reference_error(kind: &str) -> String {
    format!(
        "Generic drawing expansion involving {kind} on a changed component is unsupported; the source drawing is retained"
    )
}

/// Pose one detachable substituent before replacing its virtual handle bond.
/// The host stays fixed. A supporting half-plane separates its ring/backbone;
/// fixed exterior branches additionally require a prepaid clearance proof.
/// Shared/bridged or ambiguous placements retain the source instead of invoking
/// whole-component cleanup. All scans, storage and transforms are prepaid.
pub(super) fn pose_variable(
    doc: &mut Document,
    handle: u64,
    target: u64,
    root: u64,
    work: &mut Work,
) -> Result<(), String> {
    let references = doc
        .atoms
        .iter()
        .map(|a| {
            a.centroid
                .len()
                .saturating_add(a.stereo.as_ref().map_or(0, |s| s.neighbors.len()))
        })
        .chain(doc.bonds.iter().map(|b| b.stereo_atoms.len()))
        .fold(0usize, usize::saturating_add);
    work.charge(
        doc.atoms
            .len()
            .saturating_add(doc.bonds.len())
            .saturating_add(references)
            .saturating_mul(64),
    )?;
    let shared = || {
        "Variable attachment requires a detachable substituent with one anchor; the source drawing is retained".to_owned()
    };
    let exterior = || {
        "Variable attachment has no supported exterior placement; the source drawing is retained"
            .to_owned()
    };
    let indices: HashMap<_, _> = doc
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect();
    let h = *indices.get(&handle).ok_or("Missing variable handle")?;
    let t = *indices.get(&target).ok_or("Missing attachment target")?;
    let r = *indices.get(&root).ok_or("Missing substituent root")?;
    if h == t || h == r || t == r {
        return Err(shared());
    }
    let mut adjacent = vec![Vec::new(); doc.atoms.len()];
    for b in &doc.bonds {
        let a = *indices.get(&b.a).ok_or("Missing variable bond endpoint")?;
        let z = *indices.get(&b.b).ok_or("Missing variable bond endpoint")?;
        if a != h && z != h {
            at_mut(&mut adjacent, a)?.push(z);
            at_mut(&mut adjacent, z)?.push(a);
        }
    }
    let component = |start| -> Result<Vec<bool>, String> {
        let mut found = vec![false; doc.atoms.len()];
        let mut pending = vec![start];
        *at_mut(&mut found, start)? = true;
        while let Some(i) = pending.pop() {
            for &j in at(&adjacent, i)? {
                if !*at(&found, j)? {
                    *at_mut(&mut found, j)? = true;
                    pending.push(j);
                }
            }
        }
        Ok(found)
    };
    let moving = component(r)?;
    for id in &at(&doc.atoms, h)?.centroid {
        let i = *indices.get(id).ok_or("Missing variable candidate")?;
        if *at(&moving, i)? {
            return Err(shared());
        }
    }
    if *at(&moving, t)? {
        return Err(shared());
    }
    // Another semantic handle or a derived centroid sharing the moving branch
    // would need a joint placement policy, not independent one-anchor poses.
    for (i, atom) in doc.atoms.iter().enumerate() {
        if i == h {
            continue;
        }
        if *at(&moving, i)? && (atom.attachment.is_some() || !atom.centroid.is_empty()) {
            return Err(shared());
        }
        for id in &atom.centroid {
            let j = *indices.get(id).ok_or("Missing centroid member")?;
            if *at(&moving, j)? {
                return Err(shared());
            }
        }
    }
    let moving_ids: Vec<_> = doc
        .atoms
        .iter()
        .zip(&moving)
        .filter(|(_, moved)| **moved)
        .map(|(a, _)| a.id)
        .collect();
    super::reject_affected_stereo(doc, &moving_ids)?;
    let delta = |a: Point, b: Point| {
        (
            f64::from(a.x) - f64::from(b.x),
            f64::from(a.y) - f64::from(b.y),
        )
    };
    let unit = |(x, y): (f64, f64)| -> Result<(f64, f64), String> {
        let length = x.hypot(y);
        if !length.is_finite() || length < 0.000001 {
            return Err(exterior());
        }
        Ok((x / length, y / length))
    };
    let center = at(&doc.atoms, t)?.position;
    let old_root = at(&doc.atoms, r)?.position;
    finite(center)?;
    finite(old_root)?;
    let authored_incoming = unit(delta(old_root, at(&doc.atoms, h)?.position))?;
    let lone_neighbor = match at(&adjacent, r)?.as_slice() {
        [neighbor] => Some(at(&doc.atoms, *neighbor)?),
        _ => None,
    };
    let incoming = if let Some(neighbor) = lone_neighbor
        && matches!(at(&doc.atoms, r)?.element.as_str(), "C" | "N" | "O" | "S")
        && doc.bonds.iter().any(|b| {
            b.order == 1
                && ((b.a == root && b.b == neighbor.id) || (b.b == root && b.a == neighbor.id))
        }) {
        // A virtual centroid is not a chemical bond direction. For an ordinary
        // two-coordinate root, use the conventional 120-degree drawing angle,
        // preserving the authored turn side by a proper rotation only. There
        // is no search, internal cleanup, reflection or bond-length change.
        let branch = unit(delta(neighbor.position, old_root))?;
        let turn = if authored_incoming.0 * branch.1 - authored_incoming.1 * branch.0 < 0. {
            -1.
        } else {
            1.
        };
        let sine = turn * (3_f64.sqrt() * 0.5);
        (
            branch.0 * 0.5 + branch.1 * sine,
            branch.1 * 0.5 - branch.0 * sine,
        )
    } else {
        authored_incoming
    };
    let outward = match at(&adjacent, t)?.as_slice() {
        [] => incoming,
        [a] => {
            let (x, y) = unit(delta(at(&doc.atoms, *a)?.position, center))?;
            (-x, -y)
        }
        [a, b] => {
            let first = unit(delta(at(&doc.atoms, *a)?.position, center))?;
            let second = unit(delta(at(&doc.atoms, *b)?.position, center))?;
            // A straight site has no strict inward certificate; refuse it
            // rather than accept a boundary/collinear contact as separated.
            unit((-first.0 - second.0, -first.1 - second.1))?
        }
        _ => return Err(exterior()),
    };
    let length = f64::from(doc.drawing_style.bond_length_world);
    if !length.is_finite() || length <= 0. {
        return Err(exterior());
    }
    let tolerance = (length * 0.00001).max(0.0001);
    let side = |position: Point| {
        let d = delta(position, center);
        let x = d.0 * outward.0;
        let y = d.1 * outward.1;
        // Outward's stored f64 coefficients define the separating line.
        // Bound subtraction, products and addition conservatively; a near-
        // boundary point is ambiguous, not admitted into a positive strip.
        let error = ((f64::from(position.x).abs() + f64::from(center.x).abs()) * outward.0.abs()
            + (f64::from(position.y).abs() + f64::from(center.y).abs()) * outward.1.abs()
            + x.abs()
            + y.abs())
            * (f64::EPSILON * 8.);
        (x + y, error)
    };
    let host = component(t)?;
    // Link/repeat depiction would later move this component again. There is
    // no simultaneous anchored pose policy: refuse rather than certify a pose
    // that a later whole-component depiction can invalidate.
    let mixed = |id: &u64| {
        indices
            .get(id)
            .is_some_and(|&i| host.get(i) == Some(&true) || moving.get(i) == Some(&true))
    };
    work.charge(
        doc.generic.links.len().saturating_add(
            doc.generic
                .repeats
                .iter()
                .map(|r| r.members.len())
                .fold(0usize, usize::saturating_add),
        ),
    )?;
    if doc.generic.links.iter().any(|link| mixed(&link.atom))
        || doc
            .generic
            .repeats
            .iter()
            .any(|repeat| repeat.members.iter().any(mixed))
    {
        return Err(shared());
    }
    let mut all_inward = true;
    for (i, (atom, fixed)) in doc.atoms.iter().zip(&host).enumerate() {
        if *fixed && i != t {
            finite(atom.position)?;
            let (distance, error) = side(atom.position);
            all_inward &= distance < -error;
        }
    }
    if !all_inward {
        // An adjacent ordinary substituent can lie on/outside the ring's
        // tangent without crossing the proposed branch. Peel only terminal
        // branches: the remaining ring/backbone must still be strictly inward.
        // An acyclic or concave/bridged ambiguous host gets no such exemption.
        let mut core = host.clone();
        let mut degrees: Vec<_> = adjacent.iter().map(Vec::len).collect();
        let mut leaves: Vec<_> = degrees
            .iter()
            .enumerate()
            .filter(|(i, degree)| core.get(*i) == Some(&true) && **degree < 2)
            .map(|(i, _)| i)
            .collect();
        while let Some(i) = leaves.pop() {
            if !*at(&core, i)? {
                continue;
            }
            *at_mut(&mut core, i)? = false;
            for &j in at(&adjacent, i)? {
                if *at(&core, j)? {
                    let degree = at_mut(&mut degrees, j)?;
                    *degree = degree.saturating_sub(1);
                    if *degree < 2 {
                        leaves.push(j);
                    }
                }
            }
        }
        if !*at(&core, t)? {
            return Err(exterior());
        }
        for (i, atom) in doc.atoms.iter().enumerate() {
            if i != t && *at(&core, i)? {
                let (distance, error) = side(atom.position);
                if distance >= -error {
                    return Err(exterior());
                }
            }
        }
        // Pay for all possible point/segment comparisons before constructing
        // destinations or examining any pair. The same aggregate Work is used.
        charge_clearance(doc, work)?;
    }
    let cosine = incoming.0 * outward.0 + incoming.1 * outward.1;
    let sine = incoming.0 * outward.1 - incoming.1 * outward.0;
    let new_root = (
        f64::from(center.x) + length * outward.0,
        f64::from(center.y) + length * outward.1,
    );
    let precision = || {
        "Variable attachment cannot preserve bond geometry at this coordinate precision; the source drawing is retained".to_owned()
    };
    let mut positions = vec![None; doc.atoms.len()];
    for (i, atom) in doc.atoms.iter().enumerate() {
        if !*at(&moving, i)? {
            continue;
        }
        let (x, y) = delta(atom.position, old_root);
        let ideal = (
            new_root.0 + cosine * x - sine * y,
            new_root.1 + sine * x + cosine * y,
        );
        let position = Point::new(ideal.0 as f32, ideal.1 as f32);
        finite(position)?;
        let rounding = (f64::from(position.x) - ideal.0).hypot(f64::from(position.y) - ideal.1);
        if rounding > tolerance {
            return Err(precision());
        }
        let (distance, error) = side(position);
        if distance <= error {
            return Err(exterior());
        }
        *at_mut(&mut positions, i)? = Some(position);
    }
    let installed_root = at(&positions, r)?.ok_or("Missing posed substituent root")?;
    let connecting = delta(installed_root, center);
    if (connecting.0.hypot(connecting.1) - length).abs() > tolerance {
        return Err(precision());
    }
    for bond in &doc.bonds {
        let a = *indices
            .get(&bond.a)
            .ok_or("Missing variable bond endpoint")?;
        let b = *indices
            .get(&bond.b)
            .ok_or("Missing variable bond endpoint")?;
        if *at(&moving, a)? && *at(&moving, b)? {
            let old = delta(at(&doc.atoms, a)?.position, at(&doc.atoms, b)?.position);
            let new = delta(
                at(&positions, a)?.ok_or("Missing posed branch endpoint")?,
                at(&positions, b)?.ok_or("Missing posed branch endpoint")?,
            );
            if (old.0.hypot(old.1) - new.0.hypot(new.1)).abs() > tolerance * 2. {
                return Err(precision());
            }
        }
    }
    if !all_inward {
        let radii: Vec<_> = doc
            .bonds
            .iter()
            .map(|b| variable_bond_radius(doc, b))
            .collect();
        let contact = doc
            .bonds
            .iter()
            .enumerate()
            .find(|(_, b)| (b.a == handle && b.b == root) || (b.b == handle && b.a == root))
            .ok_or("Missing variable connecting bond")?;
        let pad = *at(&radii, contact.0)?;
        let connector = Footprint::segment(center, installed_root, pad);
        for (i, atom) in doc.atoms.iter().enumerate() {
            if *at(&host, i)? && i != t && !connector.separated(Footprint::point(atom.position, 0.))
            {
                return Err(exterior());
            }
        }
        for (edge_index, edge) in doc.bonds.iter().enumerate() {
            let a = *indices.get(&edge.a).ok_or("Missing host endpoint")?;
            let b = *indices.get(&edge.b).ok_or("Missing host endpoint")?;
            if !*at(&host, a)? || !*at(&host, b)? {
                continue;
            }
            let fixed = Footprint::segment(
                at(&doc.atoms, a)?.position,
                at(&doc.atoms, b)?.position,
                *at(&radii, edge_index)?,
            );
            if a != t && b != t && !connector.separated(fixed) {
                return Err(exterior());
            }
            for (i, position) in positions.iter().enumerate() {
                if let Some(position) = position
                    && *at(&moving, i)?
                    && !Footprint::point(*position, 0.).separated(fixed)
                {
                    return Err(exterior());
                }
            }
            for (branch_index, branch) in doc.bonds.iter().enumerate() {
                let a = *indices.get(&branch.a).ok_or("Missing branch endpoint")?;
                let b = *indices.get(&branch.b).ok_or("Missing branch endpoint")?;
                if *at(&moving, a)?
                    && *at(&moving, b)?
                    && !Footprint::segment(
                        at(&positions, a)?.ok_or("Missing posed endpoint")?,
                        at(&positions, b)?.ok_or("Missing posed endpoint")?,
                        *at(&radii, branch_index)?,
                    )
                    .separated(fixed)
                {
                    return Err(exterior());
                }
            }
        }
    }
    // No fallible work remains once the first destination is installed.
    for (atom, position) in doc.atoms.iter_mut().zip(positions) {
        if let Some(position) = position {
            atom.position = position;
        }
    }
    Ok(())
}

// A successful separating-axis test is a conservative clearance certificate
// for padded convex footprints. Touching/collinear/uncertain pairs refuse;
// there is no positive-tolerance strip that could admit a crossing. Extra
// endpoint axes improve capsule clearance without a geometry solver/retry.
#[derive(Clone, Copy)]
struct Footprint {
    points: [Point; 4],
    count: usize,
    radius: f64,
}
impl Footprint {
    fn point(p: Point, radius: f64) -> Self {
        Self {
            points: [p; 4],
            count: 1,
            radius,
        }
    }
    fn segment(a: Point, b: Point, radius: f64) -> Self {
        Self {
            points: [a, b, b, a],
            count: 2,
            radius,
        }
    }
    fn rectangle((lo, hi): (Point, Point), radius: f64) -> Self {
        Self {
            points: [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)],
            count: 4,
            radius,
        }
    }
    fn separated(self, other: Self) -> bool {
        let (Some(points), Some(other_points)) = (
            self.points.get(..self.count),
            other.points.get(..other.count),
        ) else {
            return false;
        };
        let Some(&origin) = points.first() else {
            return false;
        };
        if other_points.is_empty() {
            return false;
        }
        let axis = |x: f64, y: f64| {
            let length = x.hypot(y);
            if !length.is_finite() || length == 0. {
                return false;
            }
            let project = |points: &[Point]| {
                let mut lo = f64::INFINITY;
                let mut hi = f64::NEG_INFINITY;
                let mut error = 0_f64;
                for p in points {
                    let px = (f64::from(p.x) - f64::from(origin.x)) * x;
                    let py = (f64::from(p.y) - f64::from(origin.y)) * y;
                    lo = lo.min(px + py);
                    hi = hi.max(px + py);
                    error = error.max(
                        (f64::from(p.x).abs() + f64::from(origin.x).abs()) * x.abs()
                            + (f64::from(p.y).abs() + f64::from(origin.y).abs()) * y.abs()
                            + px.abs()
                            + py.abs(),
                    );
                }
                (lo, hi, error)
            };
            let (alo, ahi, ae) = project(points);
            let (blo, bhi, be) = project(other_points);
            let pad = (self.radius + other.radius) * length;
            let error = (ae + be + pad.abs()) * (f64::EPSILON * 64.);
            let gap = (blo - ahi).max(alo - bhi);
            pad.is_finite() && gap > pad + error
        };
        if axis(1., 0.) || axis(0., 1.) {
            return true;
        }
        for points in [points, other_points] {
            for (a, b) in points.iter().zip(points.iter().cycle().skip(1)) {
                if axis(
                    f64::from(a.y) - f64::from(b.y),
                    f64::from(b.x) - f64::from(a.x),
                ) {
                    return true;
                }
            }
        }
        points.iter().any(|a| {
            other_points.iter().any(|b| {
                axis(
                    f64::from(a.x) - f64::from(b.x),
                    f64::from(a.y) - f64::from(b.y),
                )
            })
        })
    }
}
fn charge_clearance(doc: &Document, work: &mut Work) -> Result<(), String> {
    // Covers all point/segment/box pairs, each with <=26 axes and <=8
    // projections, plus component indexing/buffers. Saturation fails closed.
    let size = doc
        .atoms
        .len()
        .saturating_add(doc.bonds.len())
        .saturating_add(1);
    work.charge(size.saturating_mul(size).saturating_mul(512))
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
        // Match the measurement paths below. A hidden skeletal carbon returns
        // before font layout, and disabled maps/CIP never measure glyphs. The
        // indexing allowance above has already paid these visibility lookups.
        // Recheck each materialized/saved/refreshed tile, so a newly visible
        // element or enabled descriptor is prepaid before its measurement.
        let main_label = doc.abbreviation(a.id).is_some()
            || crate::atom_labels::visible(a, doc)
            || a.charge != 0;
        let map_label = a.map_num != 0 && a.display.mapping.show.unwrap_or(doc.atom_labels.maps);
        let stereo_label = a.display.stereo.show.unwrap_or(doc.atom_labels.stereo);
        if !main_label
            && !map_label
            && !stereo_label
            && a.display.number.is_none()
            && a.marks.is_empty()
        {
            continue;
        }
        let bytes = a.element.len()
            + a.cip_label.as_ref().map_or(0, String::len)
            + a.display.variable.as_ref().map_or(0, String::len)
            + a.display.number.as_ref().map_or(0, |v| v.text.len())
            + a.marks.len().saturating_mul(512);
        let number_chars = a.display.number.as_ref().map_or(0, |v| v.text.len());
        let main_font = if main_label {
            a.text_style
                .as_ref()
                .map_or(doc.drawing_style.font_family.len(), |s| s.family.len())
                .saturating_mul(64)
        } else {
            0
        };
        let map_font = if map_label {
            a.display.mapping.style.family.len().saturating_mul(10)
        } else {
            0
        };
        let stereo_font = if stereo_label {
            a.display.stereo.style.family.len().saturating_mul(
                a.cip_label
                    .as_ref()
                    .map_or(0, |v| v.len().saturating_add(2))
                    .saturating_add(FUTURE_CIP_LABELS.len().saturating_mul(3)),
            )
        } else {
            0
        };
        let fonts = main_font
            .saturating_add(map_font)
            .saturating_add(stereo_font)
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
        if !b.indicator.show.unwrap_or(doc.atom_labels.stereo) {
            continue;
        }
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
            || doc
                .bonds
                .iter()
                .any(|b| affected_ids.contains(&b.a) && b.specifies_stereo())
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
            probe::enter(Stage::Solver);
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
        probe::enter(Stage::Measure);
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

/// Normalize detached arrow geometry and certify variable-branch ink clearance.
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
    let variables = source
        .atoms
        .iter()
        .filter(|a| a.is_variable_attachment())
        .count();
    let arrows = doc.arrows.iter().any(|a| a.is_attached());
    if !arrows && variables == 0 {
        return Ok(());
    }
    if variables != 0 {
        work.charge(source.atoms.len())?;
        for _ in 0..variables {
            charge_clearance(doc, work)?;
        }
    }
    presentation.charge_measurement(source, doc, work)?;
    let mut preview = refreshed_preview(doc, hydrogens)?;
    if arrows {
        normalize_native_states(doc, &mut preview)?;
    }
    if variables != 0 {
        // Reuse the one already-prepared chemical state. Saved label caches
        // and exact refreshed H labels both contribute; no guessed H state or
        // second chemistry preparation is allowed at this geometry seam.
        let boxes = saved_and_refreshed_boxes(doc, &preview)?;
        variable_ink_clearance(source, doc, &boxes)?;
    }
    Ok(())
}
fn variable_ink_clearance(
    source: &Document,
    doc: &Document,
    boxes: &[Option<(Point, Point)>],
) -> Result<(), String> {
    let collision = || {
        "Variable attachment cannot clear the preserved bonds and atom labels; the source drawing is retained".to_owned()
    };
    let indices: HashMap<_, _> = doc
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect();
    let atoms: Vec<_> = doc
        .atoms
        .iter()
        .enumerate()
        .map(|(i, atom)| {
            let mut bounds = *at(boxes, i)?;
            if crate::highlights::atom_color(doc, atom).is_some() {
                let radius = highlight_radius(&doc.drawing_style);
                let (left, right, rx, ry) = if let Some((lo, hi)) = bounds {
                    let ry = (hi.y - lo.y + radius) * 0.5;
                    let y = (lo.y + hi.y) * 0.5;
                    (Point::new(lo.x, y), Point::new(hi.x, y), ry * 0.5, ry)
                } else {
                    (atom.position, atom.position, radius, radius)
                };
                include(
                    &mut bounds,
                    left.offset(-rx * CAPSULE_SCALE, -ry * CAPSULE_SCALE),
                    right.offset(rx * CAPSULE_SCALE, ry * CAPSULE_SCALE),
                );
            }
            if let Some((lo, hi)) = bounds {
                finite(lo)?;
                finite(hi)?;
            }
            Ok(bounds.map_or_else(
                || Footprint::point(atom.position, 0.),
                |bounds| Footprint::rectangle(bounds, 0.),
            ))
        })
        .collect::<Result<_, String>>()?;
    let mut adjacent = vec![Vec::new(); doc.atoms.len()];
    let mut edges = Vec::with_capacity(doc.bonds.len());
    for bond in &doc.bonds {
        let a = *indices.get(&bond.a).ok_or("Missing generated endpoint")?;
        let b = *indices.get(&bond.b).ok_or("Missing generated endpoint")?;
        at_mut(&mut adjacent, a)?.push(b);
        at_mut(&mut adjacent, b)?.push(a);
        edges.push((
            a,
            b,
            Footprint::segment(
                at(&doc.atoms, a)?.position,
                at(&doc.atoms, b)?.position,
                variable_bond_radius(doc, bond),
            ),
        ));
    }
    for variable in source.atoms.iter().filter(|a| a.is_variable_attachment()) {
        let contact = source
            .bonds
            .iter()
            .find(|b| b.a == variable.id || b.b == variable.id)
            .ok_or("Missing source variable bond")?;
        let root = if contact.a == variable.id {
            contact.b
        } else {
            contact.a
        };
        let root = *indices
            .get(&root)
            .ok_or("Variable root identity changed during expansion")?;
        let mut targets = at(&adjacent, root)?.iter().copied().filter(|&i| {
            doc.atoms
                .get(i)
                .is_some_and(|a| variable.centroid.contains(&a.id))
        });
        let target = targets.next().ok_or("Missing generated variable contact")?;
        if targets.next().is_some() {
            return Err(collision());
        }
        let component = |start| -> Result<Vec<bool>, String> {
            let mut found = vec![false; doc.atoms.len()];
            let mut pending = vec![start];
            *at_mut(&mut found, start)? = true;
            while let Some(i) = pending.pop() {
                for &j in at(&adjacent, i)? {
                    if (i == root && j == target) || (i == target && j == root) {
                        continue;
                    }
                    if !*at(&found, j)? {
                        *at_mut(&mut found, j)? = true;
                        pending.push(j);
                    }
                }
            }
            Ok(found)
        };
        let moving = component(root)?;
        let host = component(target)?;
        if *at(&moving, target)? {
            return Err(collision());
        }
        // Custom off-site marks/indicators are not clipped by a new bond like
        // the atom's chemical label. No supported junction policy exists.
        let junction = at(&doc.atoms, target)?;
        if !junction.marks.is_empty()
            || junction.display.number.is_some()
            || (junction.map_num != 0
                && junction
                    .display
                    .mapping
                    .show
                    .unwrap_or(doc.atom_labels.maps))
        {
            return Err(collision());
        }
        for (i, moved) in moving.iter().enumerate() {
            if !moved {
                continue;
            }
            for (j, fixed) in host.iter().enumerate() {
                if *fixed && !at(&atoms, i)?.separated(*at(&atoms, j)?) {
                    return Err(collision());
                }
            }
        }
        for &(a, b, branch) in &edges {
            let connector = (a == root && b == target) || (b == root && a == target);
            if !connector && !(*at(&moving, a)? && *at(&moving, b)?) {
                continue;
            }
            if connector {
                // The host half-plane cannot separate the connector from a
                // folded part of its own substituent. Check every nonroot ink
                // footprint and nonincident internal bond before publishing.
                // These linear scans fit the already prepaid full pair bound.
                for (i, moved) in moving.iter().enumerate() {
                    if *moved && i != root && !branch.separated(*at(&atoms, i)?) {
                        return Err(collision());
                    }
                }
                for &(c, d, internal) in &edges {
                    if *at(&moving, c)?
                        && *at(&moving, d)?
                        && c != root
                        && d != root
                        && !branch.separated(internal)
                    {
                        return Err(collision());
                    }
                }
            }
            for (i, fixed) in host.iter().enumerate() {
                // The chemical label is clipped at its own bond junction.
                if *fixed && !(connector && i == target) && !branch.separated(*at(&atoms, i)?) {
                    return Err(collision());
                }
            }
            for &(c, d, fixed) in &edges {
                if *at(&host, c)?
                    && *at(&host, d)?
                    && !(connector && (c == target || d == target))
                    && !branch.separated(fixed)
                {
                    return Err(collision());
                }
            }
        }
        for (i, moved) in moving.iter().enumerate() {
            if !moved {
                continue;
            }
            for &(a, b, fixed) in &edges {
                if *at(&host, a)? && *at(&host, b)? && !at(&atoms, i)?.separated(fixed) {
                    return Err(collision());
                }
            }
        }
    }
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

fn variable_bond_radius(doc: &Document, bond: &crate::document::Bond) -> f64 {
    let style = &doc.drawing_style;
    let ordinary = |b: &crate::document::Bond| {
        matches!(b.order, 1..=4 | 7)
            && b.display == "plain"
            && b.secondary_display.as_deref().is_none_or(|s| s == "plain")
            && b.highlight.is_none()
    };
    if !ordinary(bond)
        || doc.bonds.iter().any(|b| {
            (b.a == bond.a || b.a == bond.b || b.b == bond.a || b.b == bond.b) && !ordinary(b)
        })
    {
        return f64::from(bond_paint_radius(style));
    }
    // Plain joined outlines clamp miter distance to four half line-widths.
    // Double/triple/aromatic rails lie within one spacing plus half a line.
    // Broader/styled/highlighted incident bonds use the global bound above.
    let rail = if bond.order == 1 {
        0.
    } else {
        style.bond_length_world * style.bond_spacing_ratio
    };
    f64::from((rail + style.line_width() * 0.5).max(style.line_width() * 2.))
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
    // Fields bound to `_` are compared verbatim by the final document equality.
    let Document {
        generic,
        canvas_theme: _,
        color_theme: _,
        custom_theme: _,
        drawing_style: _,
        page_layout: _,
        abbreviations,
        atom_labels: _,
        ring_fills,
        depth_appearance,
        version: _,
        atoms,
        bonds,
        annotations,
        molecule_names,
        arrows,
        graphics,
        groups,
        reactions,
        recent_colors: _,
    } = &mut restored;
    for (actual, old) in atoms.iter_mut().zip(&source.atoms) {
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
    for b in bonds {
        undo(&mut b.a)?;
        undo(&mut b.b)?;
        for id in &mut b.stereo_atoms {
            undo(id)?;
        }
    }
    for (actual, old) in annotations.iter_mut().zip(&source.annotations) {
        check_shift(old.position, actual.position)?;
        if actual.position != old.position.offset(shift.x, shift.y) {
            return Err("Prepared annotation lost its common translation".into());
        }
        undo(&mut actual.id)?;
        actual.position = old.position;
    }
    for (actual, old) in arrows.iter_mut().zip(&source.arrows) {
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
    for (actual, old) in graphics.iter_mut().zip(&source.graphics) {
        check_shift(old.origin, actual.origin)?;
        if actual.origin != old.origin.offset(shift.x, shift.y) {
            return Err("Prepared graphic lost its common translation".into());
        }
        undo(&mut actual.id)?;
        actual.origin = old.origin;
    }
    for g in groups {
        undo(&mut g.id)?;
        for id in &mut g.members {
            undo(id)?;
        }
    }
    for g in abbreviations {
        undo(&mut g.anchor)?;
        for id in &mut g.members {
            undo(id)?;
        }
    }
    for f in ring_fills {
        for id in &mut f.atoms {
            undo(id)?;
        }
    }
    for r in reactions {
        r.remap(&inverse).ok_or("Missing reaction remap")?;
    }
    for n in molecule_names {
        undo(&mut n.annotation)?;
        for id in &mut n.atoms {
            undo(id)?;
        }
    }
    *depth_appearance = crate::depth_appearance::remap(depth_appearance, &inverse);
    generic.remap(&inverse).ok_or("Missing generic remap")?;
    if &restored != source {
        return Err("Prepared tile changed noncoordinate native data".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
