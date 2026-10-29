//! Expansion work allowance, drawing-size accounting and the prepaid
//! presentation, ink and clearance cost model.
use super::ink::{FUTURE_CIP_LABELS, LabelPlan};
use crate::document::Document;
use std::collections::HashSet;

pub(super) struct Work {
    pub(super) remaining: usize,
}
impl Work {
    pub(super) fn new(limit: usize) -> Self {
        Self { remaining: limit }
    }
    pub(super) fn charge(&mut self, amount: usize) -> Result<(), String> {
        self.remaining = self
            .remaining
            .checked_sub(amount)
            .ok_or("Generic expansion exceeds the work limit")?;
        Ok(())
    }
}
pub(super) fn drawing_size(doc: &Document) -> usize {
    doc.atoms
        .len()
        .saturating_add(doc.bonds.len())
        .saturating_add(doc.annotations.len())
        .saturating_add(doc.arrows.len())
        .saturating_add(doc.graphics.len())
        .saturating_add(
            doc.groups
                .iter()
                .map(|g| g.members.len() + 1)
                .sum::<usize>(),
        )
        .saturating_add(
            doc.abbreviations
                .iter()
                .map(|g| g.members.len())
                .sum::<usize>(),
        )
}

/// Shared native-validation admission. Lengths are read without traversing
/// members, and every counting pass is paid before it starts. Candidate and
/// accumulated documents use their actual grown metadata, never source counts.
pub(super) fn charge_validation(doc: &Document, work: &mut Work) -> Result<(), String> {
    let records = doc
        .atoms
        .len()
        .saturating_add(doc.bonds.len())
        .saturating_add(doc.annotations.len())
        .saturating_add(doc.graphics.len())
        .saturating_add(doc.groups.len())
        .saturating_add(doc.abbreviations.len())
        .saturating_add(doc.molecule_names.len())
        .saturating_add(doc.arrow_labels.len())
        .saturating_add(doc.reactions.len())
        .saturating_add(doc.arrows.len())
        .saturating_add(doc.ring_fills.len())
        .saturating_add(doc.depth_appearance.len())
        .saturating_add(doc.generic.alternatives.len())
        .saturating_add(doc.generic.links.len())
        .saturating_add(doc.generic.repeats.len());
    work.charge(records)?;
    let atoms = doc.atoms.len();
    let bonds = doc.bonds.len();
    let mut cost = records.saturating_mul(16);
    let mut group_members = 0usize;
    for group in &doc.groups {
        group_members = group_members.saturating_add(group.members.len());
    }
    // validate_groups compares every member with the other group's vector.
    // The total squared safely covers all unordered pairs and empty groups.
    cost = cost.saturating_add(group_members.saturating_mul(group_members));
    for atom in &doc.atoms {
        cost = cost
            .saturating_add(atom.centroid.len().saturating_mul(atoms.saturating_add(1)))
            .saturating_add(atom.marks.len())
            .saturating_add(atom.display.label_scripts.len())
            .saturating_add(atom.stereo.as_ref().map_or(0, |s| s.neighbors.len()));
        if atom.display.variable.is_some() {
            cost = cost.saturating_add(doc.generic.links.len());
        }
        if atom.attachment.is_some() {
            cost = cost.saturating_add(
                bonds.saturating_mul(atom.centroid.len().saturating_add(atoms).saturating_add(1)),
            );
        }
    }
    for reaction in &doc.reactions {
        let participants = reaction
            .reactants
            .len()
            .saturating_add(reaction.products.len())
            .saturating_add(reaction.agents.len());
        work.charge(participants)?;
        cost = cost
            .saturating_add(doc.arrows.len())
            .saturating_add(
                reaction
                    .annotations
                    .len()
                    .saturating_mul(doc.annotations.len().saturating_add(1)),
            )
            .saturating_add(participants.saturating_mul(bonds.saturating_add(1)));
        for participant in reaction
            .reactants
            .iter()
            .chain(&reaction.products)
            .chain(&reaction.agents)
        {
            // doc.atom scans atoms; each bond probes both participant endpoints.
            cost = cost.saturating_add(
                participant.atoms.len().saturating_mul(
                    atoms
                        .saturating_add(bonds.saturating_mul(2))
                        .saturating_add(1),
                ),
            );
        }
    }
    // Each link looks up its text, caption links and arrow.
    cost = cost.saturating_add(
        doc.arrow_labels.len().saturating_mul(
            doc.annotations
                .len()
                .saturating_add(doc.molecule_names.len())
                .saturating_add(doc.arrows.len())
                .saturating_add(1),
        ),
    );
    for name in &doc.molecule_names {
        cost = cost
            .saturating_add(doc.annotations.len())
            .saturating_add(name.atoms.len().saturating_mul(atoms.saturating_add(1)));
    }
    for abbreviation in &doc.abbreviations {
        // Connectivity is indexed once; each abbreviation's boundary check
        // and reachability can revisit every bond, with hashed member lookups.
        cost = cost
            .saturating_add(bonds.saturating_mul(4))
            .saturating_add(abbreviation.members.len().saturating_mul(4));
    }
    for arrow in &doc.arrows {
        let ends = usize::from(arrow.start_anchor.is_some())
            .saturating_add(usize::from(arrow.end_anchor.is_some()));
        cost = cost.saturating_add(
            ends.saturating_mul(
                atoms
                    .saturating_mul(2)
                    .saturating_add(bonds)
                    .saturating_add(12),
            ),
        );
    }
    for repeat in &doc.generic.repeats {
        cost = cost
            .saturating_add(bonds)
            .saturating_add(repeat.members.len().saturating_mul(4))
            .saturating_add(repeat.bracket_objects.len())
            .saturating_add(repeat.crossings.len().saturating_mul(4))
            .saturating_add(
                repeat
                    .crossing_brackets
                    .len()
                    .saturating_mul(repeat.bracket_objects.len().saturating_add(1)),
            );
    }
    for scope in &doc.depth_appearance {
        // BTreeMap membership is logarithmic; length squared is a safe bound
        // for member/key visits, including malformed maps before refusal.
        let members = scope.atoms.len();
        cost = cost
            .saturating_add(members.saturating_mul(members))
            .saturating_add(scope.weights.len())
            .saturating_add(scope.overrides.len())
            .saturating_add(scope.rear_weights.len())
            .saturating_add(scope.rear_overrides.len());
    }
    for fill in &doc.ring_fills {
        cost = cost.saturating_add(fill.atoms.len().saturating_mul(8));
    }
    work.charge(cost)
}

pub(super) fn validate(doc: &Document, work: &mut Work) -> Result<(), String> {
    charge_validation(doc, work)?;
    super::probe::enter(super::probe::Stage::Validate);
    doc.validate()
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
            arrow_labels,
            graphics,
            groups,
            reactions,
            models,
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
        work.charge(arrow_labels.len())?;
        copy = copy.saturating_add(records::<crate::arrow_labels::ArrowLabel>(
            arrow_labels.len(),
        ));
        for name in molecule_names {
            work.charge(1)?;
            named_members = named_members.saturating_add(name.atoms.len());
            copy = copy
                .saturating_add(std::mem::size_of::<crate::molecule_names::MoleculeName>())
                .saturating_add(records::<u64>(name.atoms.len()))
                .saturating_add(name.smiles.len())
                .saturating_add(name.text.len());
        }
        for m in models {
            work.charge(1)?;
            copy = copy
                .saturating_add(std::mem::size_of::<crate::models3d::Model3d>())
                .saturating_add(records::<crate::models3d::Placed>(m.atoms.len()))
                .saturating_add(records::<crate::models3d::Hydrogen>(m.hydrogens.len()))
                .saturating_add(records::<u64>(m.linked_atoms.len()))
                .saturating_add(m.fingerprint.as_ref().map_or(0, String::len));
            work.charge(m.linked_atoms.len())?;
            if let Some(generated) = &m.generated {
                work.charge(
                    generated
                        .elements
                        .len()
                        .saturating_add(generated.bonds.len())
                        .saturating_add(generated.sources.len()),
                )?;
                copy = copy
                    .saturating_add(records::<String>(generated.elements.len()))
                    .saturating_add(records::<[usize; 2]>(generated.bonds.len()))
                    .saturating_add(records::<Option<u64>>(generated.sources.len()));
                for element in &generated.elements {
                    copy = copy.saturating_add(element.len());
                }
            }
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
        let native_refs = source
            .atoms
            .iter()
            .map(|a| a.centroid.len())
            .chain(source.groups.iter().map(|g| g.members.len()))
            .chain(source.abbreviations.iter().map(|g| g.members.len()))
            .chain(source.molecule_names.iter().map(|n| n.atoms.len()))
            .chain(source.ring_fills.iter().map(|g| g.atoms.len()))
            .chain(source.depth_appearance.iter().map(|g| g.atoms.len()))
            .chain(source.reactions.iter().map(|r| {
                r.annotations.len().saturating_add(1).saturating_add(
                    r.reactants
                        .iter()
                        .chain(&r.products)
                        .chain(&r.agents)
                        .map(|p| p.atoms.len().saturating_add(1))
                        .sum::<usize>(),
                )
            }))
            .fold(0usize, usize::saturating_add);
        let lookups = native_refs
            .saturating_add(self.attached_ends.saturating_mul(3))
            .saturating_add(source.generic.links.len())
            .saturating_add(source.generic.repeats.len())
            .saturating_add(
                source
                    .generic
                    .repeats
                    .iter()
                    .map(|r| r.bracket_objects.len())
                    .sum::<usize>(),
            )
            .saturating_add(if source.atoms.iter().any(|a| a.attachment.is_some()) {
                source.bonds.len()
            } else {
                0
            });
        let refs = source
            .generic
            .repeats
            .iter()
            .map(|r| {
                r.members
                    .len()
                    .saturating_add(r.crossings.len().saturating_mul(2))
                    .saturating_add(r.bracket_objects.len())
                    .saturating_add(r.crossing_brackets.len())
            })
            .fold(native_refs, usize::saturating_add);
        // Pay owned snapshot buffers before the first clone. Selected inputs
        // additionally pay bounded closure/validation/dependency scans. The
        // strict selection never calls delete, identity, anchor reconciliation
        // or automatic rear-weight materialization. Repeat closure/boundary
        // scans run per definition, not once per atom, bond or selected ID.
        let selection = if selected.is_empty() {
            0
        } else {
            size.saturating_add(refs)
                .saturating_add(selected.len())
                .saturating_mul(lookups.saturating_add(32))
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

pub(super) fn charge_clearance(doc: &Document, work: &mut Work) -> Result<(), String> {
    // Covers all point/segment/box pairs, each with <=26 axes and <=8
    // projections, plus component indexing/buffers. Saturation fails closed.
    let size = doc
        .atoms
        .len()
        .saturating_add(doc.bonds.len())
        .saturating_add(1);
    work.charge(size.saturating_mul(size).saturating_mul(512))
}

pub(super) fn charge_ink_work(doc: &Document, work: &mut Work) -> Result<(), String> {
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
        let plan = LabelPlan::new(a, doc);
        if plan.skip() {
            continue;
        }
        reserve(plan.ink_work())?;
    }
    for b in &doc.bonds {
        if !crate::atom_labels::bond_indicator_shown(b, doc) {
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

#[cfg(test)]
mod tests;
