//! Bounded Kekulé reassignment for fusion of simple aromatic rings.
//! Only neutral five/six-membered conjugated cycles are eligible. Other chemistry
//! keeps the regular attachment rules and returns a recoverable error.
use super::{Anchor, place_anchored};
use crate::{
    document::{Document, Point},
    editing,
};
use std::collections::{BTreeSet, HashMap};
type Edge = (u64, u64);
fn edge(a: u64, b: u64) -> Edge {
    (a.min(b), a.max(b))
}

fn ring(doc: &Document, a: u64, b: u64) -> Option<BTreeSet<Edge>> {
    let mut queue = vec![vec![a]];
    let mut budget = 2048;
    while let Some(path) = queue.pop() {
        if budget == 0 {
            break;
        }
        budget -= 1;
        let last = *path.last()?;
        if last == b {
            if !(5..=6).contains(&path.len()) {
                continue;
            }
            let mut edges: BTreeSet<_> = path
                .windows(2)
                .filter_map(|p| Some(edge(*p.first()?, *p.get(1)?)))
                .collect();
            edges.insert(edge(a, b));
            // Circle display stores aromatic order on every edge. Recognize a
            // complete cycle, never a mixed or partially aromatic path. Fused
            // aromatic systems need a larger assignment than this bounded ring.
            let circular = edges.iter().all(|&(a, b)| {
                doc.bonds
                    .iter()
                    .any(|e| edge(e.a, e.b) == edge(a, b) && e.order == 4)
            });
            let mut doubles = 0;
            let mut valid = true;
            for id in &path {
                let atom = doc.atom(*id)?;
                let bonds: Vec<_> = doc
                    .bonds
                    .iter()
                    .filter(|e| edges.contains(&edge(e.a, e.b)) && (e.a == *id || e.b == *id))
                    .collect();
                let donor = matches!(atom.element.as_str(), "O" | "S")
                    || (atom.element == "N" && atom.explicit_h == 1);
                let pi = if circular {
                    usize::from(!donor)
                } else {
                    bonds.iter().filter(|e| e.order == 2).count()
                };
                valid &= atom.charge == 0
                    && atom.stereo.is_none()
                    && atom.radical_electrons == 0
                    && bonds.len() == 2
                    && bonds.iter().all(|e| {
                        (if circular {
                            e.order == 4
                        } else {
                            matches!(e.order, 1 | 2)
                        }) && e.display == "plain"
                            && e.stereo.is_none()
                    })
                    && (!circular
                        || (atom.explicit_h == u32::from(atom.element == "N" && donor)
                            && !doc.bonds.iter().any(|e| {
                                e.order == 4
                                    && (e.a == *id || e.b == *id)
                                    && !edges.contains(&edge(e.a, e.b))
                            })))
                    && if donor {
                        pi == 0
                    } else {
                        matches!(atom.element.as_str(), "C" | "N") && pi == 1
                    };
                doubles += pi;
            }
            if valid && ((path.len() == 6 && doubles == 6) || (path.len() == 5 && doubles == 4)) {
                return Some(edges);
            }
            continue;
        }
        if path.len() >= 6 {
            continue;
        }
        for e in &doc.bonds {
            if edge(e.a, e.b) == edge(a, b) {
                continue;
            }
            let next = if e.a == last {
                e.b
            } else if e.b == last {
                e.a
            } else {
                continue;
            };
            if !path.contains(&next) {
                let mut extended = path.clone();
                extended.push(next);
                queue.push(extended);
            }
        }
    }
    None
}

fn matching(
    needed: &BTreeSet<u64>,
    edges: &BTreeSet<Edge>,
    budget: &mut usize,
) -> Option<Vec<Edge>> {
    if *budget == 0 {
        return None;
    }
    *budget -= 1;
    let Some(a) = needed
        .iter()
        .min_by_key(|a| {
            edges
                .iter()
                .filter(|(x, y)| {
                    (*x == **a && needed.contains(y)) || (*y == **a && needed.contains(x))
                })
                .count()
        })
        .copied()
    else {
        return Some(vec![]);
    };
    for &(x, y) in edges {
        let b = if x == a {
            y
        } else if y == a {
            x
        } else {
            continue;
        };
        if !needed.contains(&b) {
            continue;
        }
        let mut rest = needed.clone();
        rest.remove(&a);
        rest.remove(&b);
        if let Some(mut pairs) = matching(&rest, edges, budget) {
            pairs.push(edge(a, b));
            return Some(pairs);
        }
    }
    None
}

pub(super) fn fuse(
    doc: &Document,
    part: &Document,
    point: Point,
    direction: Option<Point>,
    radius: f32,
    anchor: Anchor,
) -> Option<(Document, Vec<u64>)> {
    let target = doc.bonds.get(editing::nearest_bond(doc, point, radius)?)?;
    let target_edges = ring(doc, target.a, target.b)?;
    for source in &part.bonds {
        if matches!(anchor, Anchor::Atom(_))
            || matches!(anchor, Anchor::Bond(a,b) if edge(a,b) != edge(source.a,source.b))
        {
            continue;
        }
        let Some(source_edges) = ring(part, source.a, source.b) else {
            continue;
        };
        let mut base = doc.clone();
        let mut fragment = part.clone();
        for b in &mut base.bonds {
            if target_edges.contains(&edge(b.a, b.b)) {
                b.order = 1;
            }
        }
        for b in &mut fragment.bonds {
            if source_edges.contains(&edge(b.a, b.b)) {
                b.order = 1;
            }
        }
        // Atom compatibility and attachment geometry are checked by the ordinary
        // placement routine; aromatic orders are restored before returning.
        let Ok((mut result, ids)) = place_anchored(
            &base,
            &fragment,
            point,
            direction,
            radius,
            Anchor::Bond(source.a, source.b),
        ) else {
            continue;
        };
        let mapping: HashMap<_, _> = part
            .all_ids()
            .into_iter()
            .zip(ids.iter().copied())
            .collect();
        let mut edges = target_edges.clone();
        for (a, b) in source_edges {
            edges.insert(edge(*mapping.get(&a)?, *mapping.get(&b)?));
        }
        let nodes: BTreeSet<_> = edges.iter().flat_map(|(a, b)| [*a, *b]).collect();
        let needed: BTreeSet<_> = nodes
            .iter()
            .filter(|id| {
                result
                    .atom(**id)
                    .is_some_and(|a| matches!(a.element.as_str(), "C" | "N") && a.explicit_h == 0)
            })
            .copied()
            .collect();
        let Some(pairs) = matching(&needed, &edges, &mut 4096) else {
            continue;
        };
        for b in &mut result.bonds {
            if pairs.contains(&edge(b.a, b.b)) {
                b.order = 2;
            }
        }
        if nodes.iter().any(|id| {
            result
                .atom(*id)
                .is_none_or(|a| super::valence(&result, *id) > super::capacity(a))
        }) {
            continue;
        }
        if result.validate().is_ok() {
            return Some((result, ids));
        }
    }
    None
}
