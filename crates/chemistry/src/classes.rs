//! A small, overlapping structural class catalog for prepared molecules.
//!
//! Class meanings follow the IUPAC Glossary of Class Names (Moss, Smith and
//! Tavernier, Pure Appl. Chem. 67, 1307–1375, 1995):
//! <https://iupac.qmul.ac.uk/class/>. The tooltips state the narrower carbonyl
//! scope of amides, esters and acyl halides used here. This is a local graph
//! classifier, not an exhaustive nomenclature or tautomer assignment engine.

use crate::{
    graph::{Atom, Graph},
    rings,
};
use std::collections::HashSet;

/// A recognized structural class and its short, human-readable definition.
#[derive(Debug)]
pub struct Class {
    pub term: &'static str,
    pub tooltip: &'static str,
    predicate: fn(&Molecule<'_>) -> bool,
}

impl Class {
    const fn new(
        term: &'static str,
        tooltip: &'static str,
        predicate: fn(&Molecule<'_>) -> bool,
    ) -> Self {
        Self {
            term,
            tooltip,
            predicate,
        }
    }
}

// Subclasses precede their carbonyl parents; the remaining families follow a
// fixed functional-group order. Membership overlaps rather than hiding parents.
const CATALOG: &[Class] = &[
    Class::new(
        "lactam",
        "Cyclic amide with the C(=O)-N bond in a ring, including unsaturated lactams.",
        |m| m.any(|i| m.amide(i, true)),
    ),
    Class::new(
        "amide",
        "Carbonyl C(=O)-N group; includes lactams, ureas and carbamates.",
        |m| m.any(|i| m.amide(i, false)),
    ),
    Class::new(
        "lactone",
        "Cyclic carboxylic ester with its C(=O)-O bond in a ring.",
        |m| m.any(|i| m.ester(i, 8, true)),
    ),
    Class::new(
        "ester",
        "Carboxylic ester C(=O)-O-C; includes lactones and carbamate esters.",
        |m| m.any(|i| m.ester(i, 8, false)),
    ),
    Class::new(
        "thioester",
        "Carbonyl joined to sulfur in C(=O)-S-C.",
        |m| m.any(|i| m.ester(i, 16, false)),
    ),
    Class::new(
        "carbamate",
        "Salt or ester of a carbamic acid, N-C(=O)-O− or N-C(=O)-O-C; also counted as amide, and esters as ester.",
        |m| {
            m.any(|i| {
                m.bonded(i, 7, 1)
                    && (m.ester(i, 8, false)
                        || m.neighbors(i)
                            .any(|(b, _)| b.single() && m.oxide_oxygen(b.atom)))
            })
        },
    ),
    Class::new(
        "urea",
        "Carbonyl flanked by two nitrogens, N-C(=O)-N; also counted as amide.",
        |m| m.any(|i| m.carbonyl(i) && m.count(i, 7, 1) == 2),
    ),
    Class::new(
        "acyl halide",
        "Carboxylic acyl halide with C(=O)-F, -Cl, -Br or -I.",
        |m| {
            m.any(|i| {
                m.carbonyl(i)
                    && m.neighbors(i)
                        .any(|(b, n)| b.single() && halogen(n.atom.atomic_number))
            })
        },
    ),
    Class::new(
        "carboxylic acid anhydride",
        "Two carbonyl groups linked by oxygen, C(=O)-O-C(=O).",
        |m| m.any(|i| m.bridge(i, 8, false) && m.neighbors(i).all(|(b, _)| m.carbonyl(b.atom))),
    ),
    Class::new(
        "carboxylic acid",
        "Neutral carboxy group R-C(=O)-OH, with R a carbon group or hydrogen.",
        |m| {
            m.any(|i| {
                m.carbonyl(i)
                    && m.neighbors(i)
                        .any(|(b, _)| b.single() && m.hydroxy(b.atom, 8))
                    && m.carbonyl_rest(i, &[1, 6, 8])
                    && (m.count(i, 6, 1) == 1 || m.hydrogens(i, 1))
            })
        },
    ),
    Class::new(
        "aldehyde",
        "Carbonyl bearing hydrogen, R-C(=O)-H, including formaldehyde.",
        |m| {
            m.any(|i| {
                m.carbonyl(i)
                    && m.node(i).is_some_and(|n| n.hydrogens > 0)
                    && m.carbonyl_rest(i, &[1, 6])
            })
        },
    ),
    Class::new(
        "ketone",
        "Carbonyl bonded to two carbon atoms, C-C(=O)-C.",
        |m| m.any(|i| m.carbonyl(i) && m.count(i, 6, 1) == 2),
    ),
    Class::new(
        "primary amine",
        "Neutral nonaromatic R-NH2; excludes amide and other nonamine nitrogens.",
        |m| m.any(|i| m.amine(i) == Some(1)),
    ),
    Class::new(
        "secondary amine",
        "Neutral nonaromatic R2NH; excludes amide and other nonamine nitrogens.",
        |m| m.any(|i| m.amine(i) == Some(2)),
    ),
    Class::new(
        "tertiary amine",
        "Neutral nonaromatic R3N; excludes amide and other nonamine nitrogens.",
        |m| m.any(|i| m.amine(i) == Some(3)),
    ),
    Class::new(
        "quaternary ammonium",
        "Positively charged nitrogen with four single bonds to carbon, R4N+.",
        |m| m.any(|i| m.organic_nitrogen(i, 1, 4)),
    ),
    Class::new(
        "alcohol",
        "Hydroxy group attached to a saturated carbon, C-OH.",
        |m| m.any(|i| m.saturated_carbon(i) && m.neighbors(i).any(|(b, _)| m.hydroxy(b.atom, 8))),
    ),
    Class::new(
        "phenol",
        "Hydroxy group attached directly to an aromatic carbon.",
        |m| {
            m.any(|i| {
                m.element(i, 6)
                    && m.node(i).is_some_and(|n| n.atom.aromatic)
                    && m.neighbors(i)
                        .any(|(b, _)| b.single() && m.hydroxy(b.atom, 8))
            })
        },
    ),
    Class::new(
        "ether",
        "Nonacyl C-O-C linkage; includes acetal, hemiacetal and epoxide ether bonds.",
        |m| m.any(|i| m.ether_oxygen(i)),
    ),
    Class::new(
        "thiol",
        "Neutral sulfanyl group C-SH with a nonacyl carbon substituent.",
        |m| {
            m.any(|i| {
                m.hydroxy(i, 16)
                    && m.neighbors(i)
                        .any(|(b, n)| n.atom.atomic_number == 6 && !m.carbonyl(b.atom))
            })
        },
    ),
    Class::new("nitrile", "Carbon-substituted cyano group, C-C≡N.", |m| {
        m.any(|i| {
            m.neutral(i, 6)
                && m.bonded(i, 6, 1)
                && m.neighbors(i).any(|(b, n)| {
                    b.order == 3
                        && n.atom.atomic_number == 7
                        && n.atom.charge == 0
                        && n.heavy_degree() == 1
                })
        })
    }),
    Class::new(
        "isocyanide",
        "Isocyanide linkage R-N+≡C−, including its hydrogen parent.",
        |m| m.any(|i| m.isocyanide(i)),
    ),
    Class::new(
        "imine",
        "Neutral nonaromatic C=N-R, with carbon or hydrogen substituents.",
        |m| m.any(|i| m.imine(i)),
    ),
    Class::new("oxime", "Aldehyde or ketone oxime, C=N-OH.", |m| {
        m.any(|i| m.oxime(i))
    }),
    Class::new(
        "nitrone",
        "Imine N-oxide, C=N+(O−)-R, including R = H.",
        |m| m.any(|i| m.nitrone(i)),
    ),
    Class::new(
        "amine oxide",
        "Nonaromatic amine N-oxide, R3N+-O−, including N-H analogues.",
        |m| m.any(|i| m.amine_oxide(i)),
    ),
    Class::new(
        "hemiaminal",
        "Saturated carbon bearing a hydroxy group and a nitrogen, C(OH)-N, including N-acyl hemiaminals.",
        |m| m.any(|i| m.hemiaminal(i)),
    ),
    Class::new(
        "acetal",
        "Saturated carbon bearing exactly two nonacyl alkoxy groups, C(OR)2.",
        |m| m.any(|i| m.acetal(i, false)),
    ),
    Class::new(
        "hemiacetal",
        "Saturated carbon bearing one hydroxy and one alkoxy group, C(OH)(OR).",
        |m| m.any(|i| m.acetal(i, true)),
    ),
    Class::new(
        "enamine",
        "Amine nitrogen attached directly to an alkene carbon, N-C=C.",
        |m| m.any(|i| m.amine(i).is_some() && m.neighbors(i).any(|(b, _)| m.alkene_carbon(b.atom))),
    ),
    Class::new(
        "enol",
        "Hydroxy group attached directly to an alkene carbon, HO-C=C.",
        |m| {
            m.any(|i| {
                m.alkene_carbon(i)
                    && m.neighbors(i)
                        .any(|(b, _)| b.single() && m.hydroxy(b.atom, 8))
            })
        },
    ),
    Class::new(
        "nitro compound",
        "Carbon-bound nitro group, C-N+(=O)-O−.",
        |m| m.any(|i| m.nitro(i)),
    ),
    Class::new(
        "azide",
        "Organic azide with a carbon-bound chain of three nitrogens, C-N3.",
        |m| m.any(|i| m.azide(i)),
    ),
    Class::new(
        "alkyl halide",
        "F, Cl, Br or I bonded to a saturated carbon.",
        |m| {
            m.any(|i| {
                m.saturated_carbon(i)
                    && m.neighbors(i)
                        .any(|(b, n)| b.single() && halogen(n.atom.atomic_number))
            })
        },
    ),
    Class::new(
        "aryl halide",
        "F, Cl, Br or I bonded directly to an aromatic carbon.",
        |m| {
            m.any(|i| {
                m.element(i, 6)
                    && m.node(i).is_some_and(|n| n.atom.aromatic)
                    && m.neighbors(i)
                        .any(|(b, n)| b.single() && halogen(n.atom.atomic_number))
            })
        },
    ),
    Class::new(
        "sulfonamide",
        "Sulfonyl group bonded to nitrogen, C-S(=O)2-N.",
        |m| m.any(|i| m.sulfur_oxo(i, 2) && m.count(i, 6, 1) == 1 && m.count(i, 7, 1) == 1),
    ),
    Class::new(
        "sulfone",
        "Sulfonyl group between two carbons, C-S(=O)2-C.",
        |m| m.any(|i| m.sulfur_oxo(i, 2) && m.count(i, 6, 1) == 2),
    ),
    Class::new(
        "sulfoxide",
        "Sulfinyl group between two carbons, C-S(=O)-C.",
        |m| m.any(|i| m.sulfur_oxo(i, 1) && m.count(i, 6, 1) == 2),
    ),
    Class::new(
        "epoxide",
        "Saturated three-membered cyclic ether containing two carbons and one oxygen.",
        |m| m.epoxide,
    ),
];

/// Return every matching class in catalog order, once per class.
///
/// The input must already have undergone the normal molecular preparation;
/// classification does not normalize resonance forms or change the graph.
pub fn classify(graph: &Graph) -> Result<Vec<&'static Class>, String> {
    let molecule = Molecule::new(graph)?;
    Ok(CATALOG
        .iter()
        .filter(|class| (class.predicate)(&molecule))
        .collect())
}

struct Neighbor {
    atom: usize,
    bond: usize,
    order: u8,
    aromatic: bool,
}

impl Neighbor {
    fn single(&self) -> bool {
        self.order == 1 && !self.aromatic
    }

    fn single_or_aromatic(&self) -> bool {
        self.single() || self.aromatic || self.order == 4
    }
}

struct Node<'a> {
    atom: &'a Atom,
    hydrogens: u32,
    neighbors: Vec<Neighbor>,
}

impl Node<'_> {
    fn heavy_degree(&self) -> usize {
        // Hydrogen neighbors are counted in total H, not heavy adjacency.
        self.neighbors.len()
    }
}

struct Molecule<'a> {
    nodes: Vec<Node<'a>>,
    ring_bonds: HashSet<usize>,
    epoxide: bool,
}

impl<'a> Molecule<'a> {
    fn new(graph: &'a Graph) -> Result<Self, String> {
        let valences = graph.valences()?;
        let mut nodes: Vec<_> = graph
            .atoms
            .iter()
            .zip(valences)
            .map(|(atom, valence)| Node {
                atom,
                hydrogens: u32::from(atom.explicit_hydrogens) + valence.implicit_hydrogens,
                neighbors: Vec::new(),
            })
            .collect();
        for (bond, edge) in graph.bonds.iter().enumerate() {
            for (a, b) in [(edge.a, edge.b), (edge.b, edge.a)] {
                let hydrogen = graph
                    .atoms
                    .get(b)
                    .is_some_and(|atom| atom.atomic_number == 1);
                let node = nodes.get_mut(a).ok_or("Invalid class topology")?;
                node.hydrogens += u32::from(hydrogen);
                if !hydrogen {
                    node.neighbors.push(Neighbor {
                        atom: b,
                        bond,
                        order: edge.order,
                        aromatic: edge.aromatic,
                    });
                }
            }
        }
        let rings =
            rings::perceive(graph, rings::Options::default()).map_err(|error| error.to_string())?;
        let mut result = Self {
            nodes,
            ring_bonds: rings.bonds.into_iter().flatten().collect(),
            epoxide: false,
        };
        result.epoxide = rings.atoms.iter().any(|ring| {
            ring.len() == 3
                && ring.iter().filter(|&&i| result.saturated_carbon(i)).count() == 2
                && ring.iter().filter(|&&i| result.ether_oxygen(i)).count() == 1
        });
        Ok(result)
    }

    fn node(&self, index: usize) -> Option<&Node<'a>> {
        self.nodes.get(index)
    }

    fn neighbors(&self, index: usize) -> impl Iterator<Item = (&Neighbor, &Node<'a>)> {
        self.node(index)
            .into_iter()
            .flat_map(|node| &node.neighbors)
            .filter_map(|bond| self.node(bond.atom).map(|node| (bond, node)))
    }

    fn any(&self, predicate: impl Fn(usize) -> bool) -> bool {
        self.nodes
            .iter()
            .enumerate()
            .any(|(index, _)| predicate(index))
    }

    fn element(&self, index: usize, number: u8) -> bool {
        self.node(index)
            .is_some_and(|node| node.atom.atomic_number == number)
    }

    fn neutral(&self, index: usize, number: u8) -> bool {
        self.node(index).is_some_and(|node| {
            node.atom.atomic_number == number
                && node.atom.charge == 0
                && node.atom.radical_electrons == 0
        })
    }

    fn hydrogens(&self, index: usize, hydrogens: u32) -> bool {
        self.node(index)
            .is_some_and(|node| node.hydrogens == hydrogens)
    }

    fn count(&self, index: usize, number: u8, order: u8) -> usize {
        self.neighbors(index)
            .filter(|(bond, node)| {
                bond.order == order && !bond.aromatic && node.atom.atomic_number == number
            })
            .count()
    }

    fn bonded(&self, index: usize, number: u8, order: u8) -> bool {
        self.count(index, number, order) != 0
    }

    fn carbonyl(&self, index: usize) -> bool {
        self.neutral(index, 6)
            && self.neighbors(index).any(|(bond, node)| {
                bond.order == 2
                    && !bond.aromatic
                    && node.atom.atomic_number == 8
                    && node.atom.charge == 0
            })
    }

    /// Carbon doubly bonded to O, S or N: the nitrogen of a carbonyl,
    /// thiocarbonyl or amidine group is not an amine nitrogen.
    fn acyl_like(&self, index: usize) -> bool {
        self.element(index, 6)
            && self.neighbors(index).any(|(bond, node)| {
                bond.order == 2 && !bond.aromatic && matches!(node.atom.atomic_number, 7 | 8 | 16)
            })
    }

    fn carbonyl_rest(&self, index: usize, allowed: &[u8]) -> bool {
        self.neighbors(index).all(|(bond, node)| {
            (bond.order == 2 && node.atom.atomic_number == 8)
                || (bond.single() && allowed.contains(&node.atom.atomic_number))
        })
    }

    fn saturated_carbon(&self, index: usize) -> bool {
        self.neutral(index, 6)
            && self.node(index).is_some_and(|node| !node.atom.aromatic)
            && self.neighbors(index).all(|(bond, _)| bond.single())
    }

    fn alkene_carbon(&self, index: usize) -> bool {
        self.neutral(index, 6)
            && self.node(index).is_some_and(|node| !node.atom.aromatic)
            && self.bonded(index, 6, 2)
    }

    fn hydroxy(&self, index: usize, number: u8) -> bool {
        self.neutral(index, number)
            && self.node(index).is_some_and(|node| {
                !node.atom.aromatic && node.hydrogens == 1 && node.heavy_degree() == 1
            })
            && self.neighbors(index).all(|(bond, _)| bond.single())
    }

    fn bridge(&self, index: usize, number: u8, aromatic: bool) -> bool {
        self.neutral(index, number)
            && self.node(index).is_some_and(|node| {
                (aromatic || !node.atom.aromatic) && node.hydrogens == 0 && node.heavy_degree() == 2
            })
            && self.neighbors(index).all(|(bond, node)| {
                (bond.single() || (aromatic && bond.single_or_aromatic()))
                    && node.atom.atomic_number == 6
            })
    }

    fn ether_oxygen(&self, index: usize) -> bool {
        self.bridge(index, 8, false)
            && self
                .neighbors(index)
                .all(|(bond, _)| !self.carbonyl(bond.atom))
    }

    fn amide(&self, index: usize, cyclic: bool) -> bool {
        self.carbonyl(index)
            && self.neighbors(index).any(|(bond, node)| {
                node.atom.atomic_number == 7
                    && bond.single_or_aromatic()
                    && (!cyclic || self.ring_bonds.contains(&bond.bond))
            })
    }

    fn ester(&self, index: usize, number: u8, cyclic: bool) -> bool {
        self.carbonyl(index)
            && self.neighbors(index).any(|(bond, _)| {
                bond.single_or_aromatic()
                    && self.bridge(bond.atom, number, true)
                    && (!cyclic || self.ring_bonds.contains(&bond.bond))
                    && self
                        .neighbors(bond.atom)
                        .any(|(other, _)| other.atom != index && !self.carbonyl(other.atom))
            })
    }

    fn organic_nitrogen(&self, index: usize, charge: i8, substituents: usize) -> bool {
        self.node(index).is_some_and(|node| {
            node.atom.atomic_number == 7
                && node.atom.charge == charge
                && !node.atom.aromatic
                && node.atom.radical_electrons == 0
                && node.heavy_degree() == substituents
        }) && self.neighbors(index).all(|(bond, node)| {
            bond.single() && node.atom.atomic_number == 6 && !self.acyl_like(bond.atom)
        }) && self.hydrogens(
            index,
            (if charge == 0 { 3 } else { 4 } - substituents) as u32,
        )
    }

    fn amine(&self, index: usize) -> Option<usize> {
        (1..=3).find(|&count| self.organic_nitrogen(index, 0, count))
    }

    fn imine_carbon(&self, index: usize, nitrogen: usize) -> bool {
        self.neutral(index, 6)
            && self.node(index).is_some_and(|node| !node.atom.aromatic)
            && self.neighbors(index).all(|(bond, node)| {
                (bond.atom == nitrogen && bond.order == 2 && !bond.aromatic)
                    || (bond.single() && node.atom.atomic_number == 6)
            })
    }

    fn imine(&self, index: usize) -> bool {
        self.neutral(index, 7)
            && self.node(index).is_some_and(|node| !node.atom.aromatic)
            && self
                .neighbors(index)
                .any(|(bond, _)| bond.order == 2 && self.imine_carbon(bond.atom, index))
            && self.neighbors(index).all(|(bond, node)| {
                node.atom.atomic_number == 6
                    && (bond.single() || (bond.order == 2 && !bond.aromatic))
            })
    }

    fn oxime(&self, index: usize) -> bool {
        self.neutral(index, 7)
            && self.hydrogens(index, 0)
            && self
                .node(index)
                .is_some_and(|node| node.heavy_degree() == 2)
            && self
                .neighbors(index)
                .any(|(bond, _)| bond.order == 2 && self.imine_carbon(bond.atom, index))
            && self
                .neighbors(index)
                .any(|(bond, _)| bond.single() && self.hydroxy(bond.atom, 8))
    }

    fn oxide_oxygen(&self, index: usize) -> bool {
        self.node(index).is_some_and(|node| {
            node.atom.atomic_number == 8
                && node.atom.charge == -1
                && node.atom.radical_electrons == 0
                && node.hydrogens == 0
                && node.heavy_degree() == 1
        })
    }

    fn nitrone(&self, index: usize) -> bool {
        self.node(index).is_some_and(|node| {
            node.atom.atomic_number == 7
                && node.atom.charge == 1
                && !node.atom.aromatic
                && node.atom.radical_electrons == 0
        }) && self
            .neighbors(index)
            .any(|(bond, _)| bond.single() && self.oxide_oxygen(bond.atom))
            && self
                .neighbors(index)
                .any(|(bond, _)| bond.order == 2 && self.imine_carbon(bond.atom, index))
            && self.neighbors(index).all(|(bond, node)| {
                (bond.single() && self.oxide_oxygen(bond.atom))
                    || (node.atom.atomic_number == 6 && (bond.single() || bond.order == 2))
            })
    }

    fn amine_oxide(&self, index: usize) -> bool {
        self.node(index).is_some_and(|node| {
            node.atom.atomic_number == 7
                && node.atom.charge == 1
                && !node.atom.aromatic
                && node.atom.radical_electrons == 0
        }) && self
            .neighbors(index)
            .any(|(bond, _)| bond.single() && self.oxide_oxygen(bond.atom))
            && self.neighbors(index).all(|(bond, node)| {
                bond.single()
                    && (self.oxide_oxygen(bond.atom)
                        || (node.atom.atomic_number == 6 && !self.carbonyl(bond.atom)))
            })
            && self
                .node(index)
                .is_some_and(|node| node.hydrogens + self.count(index, 6, 1) as u32 == 3)
    }

    fn isocyanide(&self, index: usize) -> bool {
        self.node(index).is_some_and(|node| {
            node.atom.atomic_number == 7 && node.atom.charge == 1 && !node.atom.aromatic
        }) && self.neighbors(index).any(|(bond, node)| {
            bond.order == 3
                && node.atom.atomic_number == 6
                && node.atom.charge == -1
                && node.heavy_degree() == 1
                && node.hydrogens == 0
        }) && self
            .neighbors(index)
            .all(|(bond, node)| node.atom.atomic_number == 6 && (bond.order == 3 || bond.single()))
    }

    fn acetal(&self, index: usize, hemi: bool) -> bool {
        if !self.saturated_carbon(index) || self.count(index, 8, 1) != 2 {
            return false;
        }
        let hydroxy = self
            .neighbors(index)
            .filter(|(bond, _)| self.hydroxy(bond.atom, 8))
            .count();
        let alkoxy = self
            .neighbors(index)
            .filter(|(bond, _)| self.ether_oxygen(bond.atom))
            .count();
        hydroxy == usize::from(hemi)
            && alkoxy == if hemi { 1 } else { 2 }
            && self
                .neighbors(index)
                .all(|(_, node)| matches!(node.atom.atomic_number, 6 | 8))
    }

    fn hemiaminal(&self, index: usize) -> bool {
        self.saturated_carbon(index)
            && self.count(index, 8, 1) == 1
            && self.count(index, 7, 1) == 1
            && self
                .neighbors(index)
                .any(|(bond, _)| self.hydroxy(bond.atom, 8))
            && self.neighbors(index).any(|(bond, node)| {
                bond.single()
                    && node.atom.atomic_number == 7
                    && node.atom.charge == 0
                    && !node.atom.aromatic
            })
            && self
                .neighbors(index)
                .all(|(_, node)| matches!(node.atom.atomic_number, 6..=8))
    }

    fn nitro(&self, index: usize) -> bool {
        self.node(index).is_some_and(|node| {
            node.atom.atomic_number == 7 && node.atom.charge == 1 && node.heavy_degree() == 3
        }) && self.count(index, 6, 1) == 1
            && self.count(index, 8, 2) == 1
            && self
                .neighbors(index)
                .any(|(bond, _)| bond.single() && self.oxide_oxygen(bond.atom))
    }

    fn azide(&self, index: usize) -> bool {
        self.node(index).is_some_and(|node| {
            node.atom.atomic_number == 7
                && node.atom.charge == 1
                && !node.atom.aromatic
                && node.hydrogens == 0
                && node.heavy_degree() == 2
        }) && self
            .neighbors(index)
            .all(|(_, node)| node.atom.atomic_number == 7)
            && self.neighbors(index).any(|(bond, node)| {
                matches!(bond.order, 2 | 3)
                    && node.heavy_degree() == 1
                    && node.hydrogens == 0
                    && i16::from(node.atom.charge) == i16::from(bond.order) - 3
            })
            && self.neighbors(index).any(|(bond, node)| {
                matches!(bond.order, 1 | 2)
                    && node.heavy_degree() == 2
                    && node.hydrogens == 0
                    && i16::from(node.atom.charge) == i16::from(bond.order) - 2
                    && self.count(bond.atom, 6, 1) == 1
            })
    }

    fn sulfur_oxo(&self, index: usize, oxygens: usize) -> bool {
        let Some(node) = self.node(index) else {
            return false;
        };
        if node.atom.atomic_number != 16
            || node.atom.aromatic
            || node.atom.radical_electrons != 0
            || node.hydrogens != 0
            || node.heavy_degree() != oxygens + 2
        {
            return false;
        }
        let oxo = self
            .neighbors(index)
            .filter(|(bond, other)| {
                other.atom.atomic_number == 8
                    && ((bond.order == 2 && other.atom.charge == 0)
                        || (bond.single() && other.atom.charge == -1 && node.atom.charge > 0))
            })
            .count();
        let separated = self
            .neighbors(index)
            .filter(|(bond, other)| {
                bond.single() && other.atom.atomic_number == 8 && other.atom.charge == -1
            })
            .count();
        oxo == oxygens && i16::from(node.atom.charge) == separated as i16
    }
}

fn halogen(number: u8) -> bool {
    matches!(number, 9 | 17 | 35 | 53)
}

#[cfg(test)]
mod tests;
