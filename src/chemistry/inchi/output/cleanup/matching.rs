use super::Context;
use crate::chemistry::inchi::output::{Error, at, invalid};
use std::collections::HashSet;

impl Context {
    /// Iterative form of the native global-visited DFS, retaining its evolving
    /// reverse path and first strictly shorter target. It is not a generic BFS.
    pub(super) fn path(
        &mut self,
        start: usize,
        number: u8,
        charge: i8,
        next: u8,
        ending: u8,
        max: usize,
    ) -> Result<Option<(usize, Vec<usize>)>, Error> {
        struct Frame {
            atom: usize,
            last: Option<usize>,
            next: u8,
            length: usize,
            max: usize,
            cursor: usize,
            entered: bool,
            target: Option<usize>,
        }
        let mut frames = vec![Frame {
            atom: start,
            last: None,
            next,
            length: 0,
            max,
            cursor: 0,
            entered: false,
            target: None,
        }];
        let mut visited = HashSet::new();
        let mut path = Vec::new();
        let mut result = None;
        while let Some(mut frame) = frames.pop() {
            self.work.spend(1)?;
            let mut finished = false;
            if !frame.entered {
                visited.insert(frame.atom);
                frame.entered = true;
                if let Some(last) = frame.last
                    && self.number(frame.atom)? == number
                    && self.order(last)? == ending
                    && self.charge(frame.atom)? == charge
                {
                    if path.is_empty() || path.len() > frame.length {
                        path.clear();
                        path.push(last);
                        frame.target = Some(frame.atom);
                    }
                    finished = true;
                } else if frame.max <= frame.length {
                    finished = true;
                }
            }
            if !finished {
                let neighbors = at(&self.topology.edges, frame.atom)?;
                if let Some(&(other, bond)) = neighbors.get(frame.cursor) {
                    frame.cursor += 1;
                    let order = self.order(bond)?;
                    let descend = !visited.contains(&other)
                        && (order == frame.next || (!matches!(ending, 1 | 2) && order == ending));
                    if descend {
                        let (next, max) = if order == frame.next {
                            (if frame.next == 1 { 2 } else { 1 }, frame.max)
                        } else {
                            (0, 0)
                        };
                        let length = frame.length + 1;
                        frames.push(frame);
                        frames.push(Frame {
                            atom: other,
                            last: Some(bond),
                            next,
                            length,
                            max,
                            cursor: 0,
                            entered: false,
                            target: None,
                        });
                    } else {
                        frames.push(frame);
                    }
                    continue;
                }
                if frame.target.is_some()
                    && let Some(last) = frame.last
                {
                    path.push(last);
                }
            }
            if let Some(target) = frame.target {
                if let Some(parent) = frames.last_mut() {
                    parent.target = Some(target);
                } else {
                    result = Some(target);
                }
            }
        }
        Ok(result.map(|target| (target, path)))
    }

    /// Unsorted VF2 order for the small connected literal-atom queries used by
    /// cleanUp. Native uniquification uses target atom sets and caps at 1000.
    pub(super) fn matches(
        &mut self,
        numbers: &[u8],
        bonds: &[(usize, usize, u8)],
    ) -> Result<Vec<Vec<usize>>, Error> {
        if numbers.is_empty() || numbers.len() > 7 {
            return Err(invalid("Invalid cleanup query"));
        }
        let mut mapping = vec![None; numbers.len()];
        let mut frames = vec![(None, 0usize)];
        let mut unique = HashSet::new();
        let mut result = Vec::new();
        while let Some((parent, cursor)) = frames.last_mut() {
            self.work.spend(1)?;
            let candidate = if let Some(parent) = *parent {
                at(&self.topology.edges, parent)?
                    .get(*cursor)
                    .map(|&(other, _)| other)
            } else {
                (*cursor < self.state.graph.atoms.len()).then_some(*cursor)
            };
            *cursor += 1;
            let depth = frames.len() - 1;
            *mapping
                .get_mut(depth)
                .ok_or_else(|| invalid("Missing query index"))? = None;
            let Some(candidate) = candidate else {
                frames.pop();
                continue;
            };
            if mapping.contains(&Some(candidate)) || self.number(candidate)? != *at(numbers, depth)?
            {
                continue;
            }
            let mut compatible = true;
            for &(a, b, order) in bonds {
                let other = if a == depth {
                    b
                } else if b == depth {
                    a
                } else {
                    continue;
                };
                let Some(mapped) = *at(&mapping, other)? else {
                    continue;
                };
                self.work
                    .spend(at(&self.topology.edges, candidate)?.len())?;
                let Some(edge) = self.topology.bond(candidate, mapped)? else {
                    compatible = false;
                    break;
                };
                if order != 0 && !*at(&self.unspecified, edge)? && self.order(edge)? != order {
                    compatible = false;
                    break;
                }
            }
            if !compatible {
                continue;
            }
            *mapping
                .get_mut(depth)
                .ok_or_else(|| invalid("Missing query index"))? = Some(candidate);
            if depth + 1 == numbers.len() {
                let complete = mapping
                    .iter()
                    .copied()
                    .map(|a| a.ok_or_else(|| invalid("Incomplete cleanup query")))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut key = complete.clone();
                key.sort_unstable();
                if unique.insert(key) {
                    result.push(complete);
                    if result.len() == 1000 {
                        break;
                    }
                }
            } else {
                let next = depth + 1;
                let parent = bonds
                    .iter()
                    .find_map(|&(a, b, _)| {
                        let other = if a == next {
                            b
                        } else if b == next {
                            a
                        } else {
                            return None;
                        };
                        mapping.get(other).copied().flatten()
                    })
                    .ok_or_else(|| invalid("Disconnected cleanup query"))?;
                frames.push((Some(parent), 0));
            }
        }
        Ok(result)
    }
}
