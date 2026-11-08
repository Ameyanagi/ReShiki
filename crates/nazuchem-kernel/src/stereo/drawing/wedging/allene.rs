// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original nazuchem/ReShiki code; source history and separately attributed data/API context follow.
//   ReShiki, crates/chemistry/src/stereo/drawing/wedging/allene.rs @ be63a799aa03a83dc6ec74bb294f4d42b89298d3.

// Copied from crates/chemistry/src/stereo/drawing/wedging/allene.rs at be63a799aa03a83dc6ec74bb294f4d42b89298d3; changed: pass an explicit default preparation budget in the inline test
use super::*;
use crate::stereo::allene;

impl Context<'_> {
    pub(super) fn choose_allene(&mut self, work: &mut Work) -> Result<(), String> {
        work.spend(self.state.metadata.atoms.len())?;
        for (center, meta) in self.state.metadata.atoms.iter().enumerate() {
            if meta.chiral_tag != 5 || !matches!(meta.chiral_permutation, Some(1 | 2)) {
                continue;
            }
            // Topology and carrier helpers scan insertion-ordered bonds; debit
            // those scans to the existing molecule-wide wedging allowance.
            work.spend(self.state.graph.bonds.len().saturating_mul(9))?;
            let Some(ends) = allene::terminals(&self.state.graph, center) else {
                continue;
            };
            // A drawing's own one-ended wedges already define its allene.
            if self
                .state
                .graph
                .bonds
                .iter()
                .zip(&self.state.directions)
                .any(|(b, &d)| b.order == 1 && ends.contains(&b.a) && wedged(d))
            {
                continue;
            }
            let carriers =
                allene::carriers(&self.state.graph, center).ok_or("Missing allene carriers")?;
            let axis = planar(self.point(ends[1])?).sub(planar(self.point(ends[0])?));
            let mut vectors = [Point3::default(); 2];
            for ((&end, pair), vector) in ends
                .iter()
                .zip(carriers.as_chunks::<2>().0)
                .zip(&mut vectors)
            {
                let first = pair[0].ok_or("Missing allene substituent")?;
                *vector = planar(self.point(first)?).sub(planar(self.point(end)?));
            }
            let mut selected = None;
            for (i, carrier) in carriers.iter().enumerate() {
                let Some(carrier) = carrier else { continue };
                let end = *at(&ends, i / 2)?;
                let bond = at(&self.adjacent, end)?
                    .iter()
                    .copied()
                    .find(|&b| self.other(b, end).ok() == Some(*carrier))
                    .ok_or("Missing allene wedge bond")?;
                if self.choices.contains_key(&bond) || self.direction(bond)? != Direction::None {
                    continue;
                }
                let mut trial = vectors;
                trial.get_mut(i / 2).ok_or("Missing allene endpoint")?.z =
                    if i % 2 == 0 { 1. } else { -1. };
                let volume = trial[0].cross(trial[1]).dot(axis);
                if volume.abs() < 1e-7 {
                    continue;
                }
                let direction = if (volume > 0.) == (meta.chiral_permutation == Some(1)) {
                    Direction::Wedge
                } else {
                    Direction::Hash
                };
                selected = Some((bond, end, direction));
                break;
            }
            let (bond, end, direction) =
                selected.ok_or("No bond available to depict allene stereo")?;
            self.choices.insert(bond, Choice::Allene(end, direction));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_allenes_choose_separate_wedges() {
        let mut state = crate::smiles::prepare("CC=C=CC=C=CC", &crate::budget::Budget::default())
            .unwrap()
            .state;
        // Make the shared terminal bond the first candidate for both axes.
        state.graph.bonds.swap(1, 2);
        for center in [2, 5] {
            state.metadata.atoms[center].chiral_tag = 5;
            state.metadata.atoms[center].chiral_permutation = Some(1);
        }
        let positions = [
            (-3., 1.),
            (-2., 0.),
            (-1., 0.),
            (0., 0.),
            (1., 1.),
            (2., 1.),
            (3., 1.),
            (4., 2.),
        ]
        .map(|(x, y)| Point3 { x, y, z: 0. })
        .to_vec();
        let conf = Conformer {
            positions,
            is_3d: false,
        };
        let input = WedgeState {
            graph: state.graph,
            metadata: state.metadata,
            directions: state.directions,
            rings: state.rings,
        };
        let properties = WedgeProperties {
            valences: state.valences,
            attachment_points: vec![false; 8],
        };
        let output = wedge_molecule(&input, &properties, Some(&conf), false).unwrap();
        let drawn = super::super::super::from_directions(
            &output.graph,
            &Metadata::unspecified(&output.graph),
            &output.directions,
            Some(&conf.positions),
            true,
        )
        .unwrap();
        for center in [2, 5] {
            assert_eq!(drawn.metadata.atoms[center].chiral_tag, 5);
            assert_eq!(drawn.metadata.atoms[center].chiral_permutation, Some(1));
        }
    }
}
