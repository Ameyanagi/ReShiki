use super::{Error, Rule, invalid};
use crate::chemistry::stereo::cip::digraph::{Descriptor, Digraph, RING_DUPLICATE};
use std::cmp::Ordering;

pub(super) fn order<T: Ord>(a: T, b: T) -> i8 {
    match a.cmp(&b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}
fn descriptor_type(d: Descriptor) -> Result<u8, Error> {
    use Descriptor::*;
    match d {
        None | Unknown | Other => Ok(0),
        PseudoR | PseudoS | PseudoM | PseudoP | E | Z => Ok(1),
        R | S | M | P | SeqTrans | SeqCis => Ok(2),
        _ => Err(invalid("Invalid CIP stereo descriptor")),
    }
}
fn pseudo(d: Descriptor) -> u8 {
    match d {
        Descriptor::PseudoR | Descriptor::PseudoM => 2,
        Descriptor::PseudoS | Descriptor::PseudoP => 1,
        _ => 0,
    }
}
fn legacy(d: Descriptor) -> u8 {
    match d {
        Descriptor::R | Descriptor::M | Descriptor::SeqCis => 2,
        Descriptor::S | Descriptor::P | Descriptor::SeqTrans => 1,
        _ => 0,
    }
}
pub(super) fn compare(g: &Digraph<'_>, rule: Rule, a: usize, b: usize) -> Result<i8, Error> {
    let (ae, be) = (g.edge(a)?, g.edge(b)?);
    let (an, bn) = (g.node(ae.end)?, g.node(be.end)?);
    let ab = if ae.bond.is_some() {
        ae.aux
    } else {
        Descriptor::None
    };
    let bb = if be.bond.is_some() {
        be.aux
    } else {
        Descriptor::None
    };
    Ok(match rule {
        Rule::DescriptorPair | Rule::PseudoPair => {
            return Err(invalid("Pair rule requires graph traversal"));
        }
        Rule::AtomicNumber => order(
            u64::from(an.fraction.0) * u64::from(bn.fraction.1),
            u64::from(bn.fraction.0) * u64::from(an.fraction.1),
        ),
        Rule::RingDuplicate => {
            let (a, b) = (
                an.flags & RING_DUPLICATE != 0,
                bn.flags & RING_DUPLICATE != 0,
            );
            if a && b {
                order(bn.distance, an.distance)
            } else {
                order(a, b)
            }
        }
        Rule::Isotope => {
            if an.number == 0 || bn.number == 0 {
                order(an.number, bn.number)
            } else if an.isotope == 0 && bn.isotope == 0 {
                0
            } else if an.mass < bn.mass {
                -1
            } else if an.mass == bn.mass {
                0
            } else {
                1
            }
        }
        Rule::DoubleBondStereo => {
            let ord = |d| match d {
                Descriptor::E => 1,
                Descriptor::Z => 2,
                _ => 0,
            };
            order(ord(an.aux), ord(bn.aux))
        }
        Rule::DescriptorType => {
            let cmp = order(descriptor_type(ab)?, descriptor_type(bb)?);
            if cmp != 0 {
                cmp
            } else {
                order(descriptor_type(an.aux)?, descriptor_type(bn.aux)?)
            }
        }
        Rule::PseudoDescriptor => {
            let cmp = order(pseudo(ab), pseudo(bb));
            if cmp != 0 {
                cmp
            } else {
                order(pseudo(an.aux), pseudo(bn.aux))
            }
        }
        Rule::LegacyDescriptor => {
            let cmp = order(legacy(ab), legacy(bb));
            if cmp != 0 {
                cmp
            } else {
                2 * order(legacy(an.aux), legacy(bn.aux))
            }
        }
        Rule::ReferenceAtom => match g.rule6_reference() {
            Some(reference) => 2 * order(an.atom == Some(reference), bn.atom == Some(reference)),
            None => 0,
        },
    })
}
