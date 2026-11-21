//! Explicit mutation of fitted source families, before a candidate is shown.
use super::{Layout, nominal_document, parameters, valid};
use crate::{
    cycles::{Step, center_of},
    document::{Document, Point},
    graphics::Graphic,
};

fn shift(doc: &mut Document, part: &[u64], target: Point) -> bool {
    let nominal = nominal_document(doc);
    let Some(ink) = crate::scene::primary_selections_bounds(&nominal, &[part.to_vec()])
        .pop()
        .flatten()
    else {
        return false;
    };
    let from = center_of(ink);
    let delta = Point::new(target.x - from.x, target.y - from.y);
    let ids = crate::molecule_names::include_annotations(doc, part);
    if !target.is_finite() || !crate::cycles::resize::rigid_bonds(doc, &ids, delta) {
        return false;
    }
    doc.translate(&ids, delta.x, delta.y);
    true
}

/// Rebase a live source after a host/outer move. Selected cuts slide only
/// where their ink still meets their ordered source; ordinary take-out owns
/// detachment. Rigid whole-group moves already moved every cut once.
pub(in crate::cycles) fn moved(before: &Document, after: &mut Document, moved: &[u64]) -> bool {
    let old_nominal = nominal_document(before);
    let new_nominal = nominal_document(after);
    let mut rebase = vec![];
    let mut active = vec![];
    for old in before
        .graphics
        .iter()
        .filter(|g| g.cycle.as_deref().is_some_and(|c| !c.split_runs.is_empty()))
    {
        let Some(new) = after.graphics.iter().find(|g| g.id == old.id) else {
            continue;
        };
        let (Some(a), Some(b)) = (old.cycle.as_deref(), new.cycle.as_deref()) else {
            continue;
        };
        active.push(old.id);
        let old_bounds = crate::scene::primary_selections_bounds(&old_nominal, &a.intermediates);
        let new_bounds = crate::scene::primary_selections_bounds(&new_nominal, &b.intermediates);
        let whole =
            moved.contains(&old.id) && a.parts().flat_map(|(_, p)| p).all(|id| moved.contains(id));
        let gram = |g: &Graphic| {
            [
                g.axis_x.x * g.axis_x.x + g.axis_x.y * g.axis_x.y,
                g.axis_y.x * g.axis_y.x + g.axis_y.y * g.axis_y.y,
                g.axis_x.x * g.axis_y.x + g.axis_x.y * g.axis_y.y,
            ]
        };
        let rigid = whole
            && gram(old)
                .into_iter()
                .zip(gram(new))
                .all(|(a, b)| (a - b).abs() <= 1e-5 * a.abs().max(b.abs()).max(1.));
        for (r, run) in a.split_runs.iter().enumerate() {
            let Some(next) = b
                .split_runs
                .iter()
                .position(|next| next.first_step == run.first_step)
            else {
                continue;
            };
            let next_run = &b.split_runs[next];
            let slots = [run.first_step, run.end_slot(a.intermediates.len())];
            let next_slots = [
                next_run.first_step,
                next_run.end_slot(b.intermediates.len()),
            ];
            let outer_changed = slots
                .into_iter()
                .zip(next_slots)
                .any(|(i, j)| old_bounds.get(i) != new_bounds.get(j));
            let trims_changed = a
                .steps
                .get(run.first_step)
                .zip(b.steps.get(next_run.first_step))
                .is_some_and(|(a, b)| a.lead_pt != b.lead_pt)
                || a.steps
                    .get(run.last_step(a.steps.len()))
                    .zip(b.steps.get(next_run.last_step(b.steps.len())))
                    .is_some_and(|(a, b)| a.trail_pt != b.trail_pt);
            let geometry_changed = old.origin != new.origin
                || old.axis_x != new.axis_x
                || old.axis_y != new.axis_y
                || a.shape != b.shape
                || old.style.corner_pt != new.style.corner_pt
                || a.gap_pt != b.gap_pt;
            if rigid {
                let reflected = (old.frame_determinant() < 0.) != (new.frame_determinant() < 0.);
                let (start, span) = if reflected {
                    ((-run.source_start).rem_euclid(1.), -run.source_span)
                } else {
                    (run.source_start, run.source_span)
                };
                rebase.push((old.id, next, if start >= 1. { 0. } else { start }, span, r));
            } else if outer_changed || trims_changed || geometry_changed {
                let Some(outline) = crate::cycles::outline(new) else {
                    *after = before.clone();
                    return false;
                };
                let actual = crate::scene::selections_bounds(after, &b.intermediates);
                let endpoint = |i: usize| {
                    Some((
                        outline.project(center_of((*actual.get(i)?)?))?.s,
                        *actual.get(i)?,
                    ))
                };
                let source = Step {
                    bow_pt: next_run.source_bow_pt,
                    reversible: next_run.source_reversible,
                    lead_pt: b.steps.get(next_slots[0]).map_or(0., |s| s.lead_pt),
                    trail_pt: b
                        .steps
                        .get(next_run.last_step(b.steps.len()))
                        .map_or(0., |s| s.trail_pt),
                    ..Default::default()
                };
                let Some(p) = endpoint(next_slots[0])
                    .zip(endpoint(next_slots[1]))
                    .and_then(|(a, b)| {
                        parameters(
                            after,
                            new,
                            &outline,
                            &source,
                            [a, b],
                            next_slots[0] == next_slots[1],
                        )
                    })
                else {
                    *after = before.clone();
                    return false;
                };
                let start = (p.start / outline.length()).rem_euclid(1.);
                rebase.push((
                    old.id,
                    next,
                    if start >= 1. { 0. } else { start },
                    p.length / outline.length(),
                    r,
                ));
            }
        }
    }
    for &(id, r, start, span, _) in &rebase {
        if let Some(run) = after
            .graphics
            .iter_mut()
            .find(|g| g.id == id)
            .and_then(|g| g.cycle.as_deref_mut())
            .and_then(|c| c.split_runs.get_mut(r))
        {
            run.source_start = start;
            run.source_span = span;
        }
    }
    // Only an explicit cut move may update its logical station. Passive
    // bounds/label calculation never enters this path.
    let mut slides = vec![];
    for &id in &active {
        let Some(g) = after.graphics.iter().find(|g| g.id == id) else {
            continue;
        };
        let Some(c) = g.cycle.as_deref() else {
            continue;
        };
        let Some(layout) = Layout::graphic(after, g) else {
            continue;
        };
        for (r, run) in c.split_runs.iter().enumerate() {
            for (cut, &old) in run.cuts.iter().enumerate() {
                let slot = (run.first_step + cut + 1) % c.intermediates.len();
                let Some(part) = c.intermediates.get(slot) else {
                    continue;
                };
                if !part.iter().any(|id| moved.contains(id)) || moved.contains(&id) {
                    continue;
                }
                let Some(source) = layout.sources.get(r).and_then(Option::as_ref) else {
                    continue;
                };
                let Some(body) = crate::scene::primary_selections_bounds(
                    &new_nominal,
                    std::slice::from_ref(part),
                )
                .pop()
                .flatten() else {
                    continue;
                };
                if let Some(u) = source.projected_cut(cut, center_of(body)) {
                    let distance = source
                        .profiles
                        .center
                        .distance_at(old)
                        .zip(source.profiles.center.distance_at(u))
                        .map_or(f64::INFINITY, |(a, b)| (a - b).abs());
                    if distance > 0.01 {
                        slides.push((id, r, cut, slot, u));
                    }
                }
            }
        }
    }
    for &(id, r, cut, slot, u) in &slides {
        let mut candidate = after.clone();
        let Some(c) = candidate
            .graphics
            .iter_mut()
            .find(|g| g.id == id)
            .and_then(|g| g.cycle.as_deref_mut())
        else {
            continue;
        };
        let Some(value) = c.split_runs.get_mut(r).and_then(|r| r.cuts.get_mut(cut)) else {
            continue;
        };
        *value = u;
        if c.validate().is_err() {
            continue;
        }
        let keep = candidate
            .graphics
            .iter()
            .find(|g| g.id == id)
            .and_then(|g| Layout::graphic(&candidate, g))
            .is_some_and(|l| l.cut_intersects(slot) == Some(true));
        if keep {
            *after = candidate;
        }
    }
    let mut plans = vec![];
    for &(id, r, _, _, old_r) in &rebase {
        let Some(old_g) = before.graphics.iter().find(|g| g.id == id) else {
            continue;
        };
        let Some(new_g) = after.graphics.iter().find(|g| g.id == id) else {
            continue;
        };
        let (Some(old_layout), Some(new_layout)) = (
            Layout::graphic(before, old_g),
            Layout::graphic(after, new_g),
        ) else {
            *after = before.clone();
            return false;
        };
        let Some(c) = new_g.cycle.as_deref() else {
            continue;
        };
        let Some(run) = c.split_runs.get(r) else {
            continue;
        };
        for (cut, &u) in run.cuts.iter().enumerate() {
            let slot = (run.first_step + cut + 1) % c.intermediates.len();
            let Some(part) = c.intermediates.get(slot) else {
                continue;
            };
            if new_layout.cut_intersects(slot) == Some(true) {
                continue;
            }
            if part.iter().any(|id| moved.contains(id)) && !moved.contains(&id) {
                continue;
            }
            let Some(old_source) = old_layout.sources.get(old_r).and_then(Option::as_ref) else {
                *after = before.clone();
                return false;
            };
            let Some(next_source) = new_layout.sources.get(r).and_then(Option::as_ref) else {
                *after = before.clone();
                return false;
            };
            let Some(from) =
                crate::scene::primary_selections_bounds(&old_nominal, std::slice::from_ref(part))
                    .pop()
                    .flatten()
                    .map(center_of)
            else {
                continue;
            };
            let Some(to) = old_source.transport(next_source, u, from) else {
                *after = before.clone();
                return false;
            };
            plans.push((part.clone(), to));
        }
    }
    for (part, to) in plans {
        if !shift(after, &part, to) {
            *after = before.clone();
            return false;
        }
    }
    for id in active {
        if !valid(after, id) {
            let selected_cut = after
                .graphics
                .iter()
                .find(|g| g.id == id)
                .and_then(|g| g.cycle.as_deref())
                .is_some_and(|c| {
                    c.split_runs.iter().any(|run| {
                        run.cuts.iter().enumerate().any(|(cut, _)| {
                            c.intermediates
                                .get((run.first_step + cut + 1) % c.intermediates.len())
                                .is_some_and(|part| part.iter().any(|id| moved.contains(id)))
                        })
                    })
                });
            if !selected_cut {
                *after = before.clone();
                return false;
            }
        }
    }
    before != after
}

/// Transport only former cuts onto the actual native outline in cycle order.
/// This is used when a source is deliberately retired, never by the renderer.
pub(in crate::cycles) fn reflow_native(
    doc: &mut Document,
    id: u64,
    parts: &[(Vec<u64>, f64)],
) -> bool {
    let Some(g) = doc.graphics.iter().find(|g| g.id == id) else {
        return false;
    };
    let Some(cycle) = g.cycle.as_deref() else {
        return false;
    };
    let Some(outline) = crate::cycles::outline(g) else {
        return false;
    };
    let n = cycle.intermediates.len();
    if n == 0 {
        return true;
    }
    let slots: Vec<_> = parts
        .iter()
        .filter_map(|(ids, u)| {
            cycle
                .intermediates
                .iter()
                .position(|p| p.iter().any(|id| ids.contains(id)))
                .map(|i| (i, *u))
        })
        .collect();
    let chosen = |i: usize| slots.iter().any(|(slot, _)| *slot == i);
    let bounds = crate::scene::selections_bounds(doc, &cycle.intermediates);
    let anchors: Vec<_> = bounds
        .iter()
        .map(|ink| ink.and_then(|b| outline.project(center_of(b))).map(|p| p.s))
        .collect();
    let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
    let mut plans = vec![];
    for &(slot, _) in &slots {
        let prev = (1..=n).map(|k| (slot + n - k) % n).find(|i| !chosen(*i));
        let next = (1..=n).map(|k| (slot + k) % n).find(|i| !chosen(*i));
        let at = match (prev, next) {
            (Some(a), Some(b)) => {
                let (Some(first), Some(last)) = (
                    anchors.get(a).copied().flatten(),
                    anchors.get(b).copied().flatten(),
                ) else {
                    return false;
                };
                let mut span = (dir * (last - first)).rem_euclid(outline.length());
                if a == b {
                    span = outline.length();
                }
                if span == 0. {
                    return false;
                }
                let ordered: Vec<_> = (1..n)
                    .map(|k| (a + k) % n)
                    .take_while(|i| *i != b)
                    .filter(|i| chosen(*i))
                    .collect();
                let Some(rank) = ordered.iter().position(|i| *i == slot) else {
                    return false;
                };
                first + dir * span * (rank + 1) as f64 / (ordered.len() + 1) as f64
            }
            _ => dir * outline.length() * slot as f64 / n as f64,
        };
        let Some(target) = outline.point(at) else {
            return false;
        };
        let Some(part) = cycle.intermediates.get(slot) else {
            return false;
        };
        plans.push((part.clone(), target));
    }
    let mut candidate = doc.clone();
    for (part, target) in plans {
        if !shift(&mut candidate, &part, target) {
            return false;
        }
    }
    *doc = candidate;
    true
}

/// A property edit either keeps the source family, or explicitly retires the
/// affected run before independently changing a child's Bow/Circle display.
pub(in crate::cycles) fn edited(
    before: &Document,
    after: &mut Document,
    id: u64,
    bow_step: Option<usize>,
) -> Result<bool, String> {
    let Some(old) = before.graphics.iter().find(|g| g.id == id) else {
        return Ok(false);
    };
    let Some(old_cycle) = old.cycle.as_deref() else {
        return Ok(false);
    };
    if old_cycle.split_runs.is_empty() {
        return Ok(false);
    }
    let Some(new) = after.graphics.iter().find(|g| g.id == id) else {
        return Ok(false);
    };
    let circle = new
        .cycle
        .as_deref()
        .is_some_and(|c| c.shape == crate::cycles::Shape::Circle);
    let new_cycle = new.cycle.as_deref();
    let reversed = new_cycle.is_some_and(|c| c.counterclockwise != old_cycle.counterclockwise);
    let retire: Vec<_> = old_cycle
        .split_runs
        .iter()
        .filter(|run| {
            let reversible_changed = !reversed
                && (0..=run.cuts.len()).any(|child| {
                    let slot = (run.first_step + child) % old_cycle.steps.len();
                    old_cycle
                        .steps
                        .get(slot)
                        .zip(new_cycle.and_then(|c| c.steps.get(slot)))
                        .is_some_and(|(a, b)| a.reversible != b.reversible)
                });
            let bow_changed = bow_step.is_some_and(|s| {
                old_cycle
                    .steps
                    .get(s)
                    .zip(new_cycle.and_then(|c| c.steps.get(s)))
                    .is_some_and(|(a, b)| a.bow_pt != b.bow_pt)
                    && old_cycle
                        .split_run(s)
                        .is_some_and(|(owner, _)| std::ptr::eq(owner, *run))
            });
            circle || reversible_changed || bow_changed
        })
        .cloned()
        .collect();
    let mut changed_positions = false;
    if !retire.is_empty() {
        let n = old_cycle.intermediates.len();
        let parts: Vec<_> = retire
            .iter()
            .flat_map(|run| {
                run.cuts.iter().enumerate().filter_map(|(cut, &u)| {
                    Some((
                        old_cycle
                            .intermediates
                            .get((run.first_step + cut + 1) % n)?
                            .clone(),
                        u,
                    ))
                })
            })
            .collect();
        if let Some(cycle) = after
            .graphics
            .iter_mut()
            .find(|g| g.id == id)
            .and_then(|g| g.cycle.as_deref_mut())
        {
            cycle
                .split_runs
                .retain(|run| !retire.iter().any(|r| r.first_step == run.first_step));
        }
        if !reflow_native(after, id, &parts) {
            return Err("Cycle arrows need room between structures".into());
        }
        changed_positions = parts.iter().any(|(ids, _)| {
            crate::scene::selection_bounds(before, ids)
                != crate::scene::selection_bounds(after, ids)
        });
    }
    let retained = after
        .graphics
        .iter()
        .find(|g| g.id == id)
        .and_then(|g| g.cycle.as_deref())
        .is_some_and(|c| !c.split_runs.is_empty());
    if retained {
        // Mark the host as edited: unchanged intersecting cut ink stays put;
        // only cuts left behind by the new source are transported.
        let positions: Vec<_> = old_cycle
            .intermediates
            .iter()
            .map(|ids| crate::scene::selection_bounds(after, ids))
            .collect();
        let requested = *after != *before;
        moved(before, after, &[id]);
        if requested && after == before {
            return Err("Cycle arrows need room between structures".into());
        }
        changed_positions |= old_cycle
            .intermediates
            .iter()
            .zip(positions)
            .any(|(ids, b)| crate::scene::selection_bounds(after, ids) != b);
        if !valid(after, id) {
            return Err("Cycle arrows need room between structures".into());
        }
    }
    if after.validate().is_err() {
        return Err("Cycle arrows need room between structures".into());
    }
    let n = old_cycle.intermediates.len();
    changed_positions |= old_cycle.split_runs.iter().any(|run| {
        run.cuts.iter().enumerate().any(|(cut, _)| {
            let Some(ids) = old_cycle.intermediates.get((run.first_step + cut + 1) % n) else {
                return false;
            };
            crate::scene::selection_bounds(before, ids)
                != crate::scene::selection_bounds(after, ids)
        })
    });
    Ok(changed_positions)
}

/// Structural edits can invalidate a source family. Passive caches never
/// update its baseline/cuts; retire only an undrawable nominal family.
pub(in crate::cycles) fn repair(doc: &mut Document) {
    let hosts: Vec<_> = doc
        .graphics
        .iter()
        .filter(|g| g.cycle.as_deref().is_some_and(|c| !c.split_runs.is_empty()))
        .map(|g| g.id)
        .collect();
    if hosts.is_empty() {
        return;
    }
    for id in hosts {
        if valid(doc, id) {
            continue;
        }
        // Reconciliation can remove an outer member or replace its ink.
        // Retire only undrawable source families on a complete candidate;
        // never leave metadata that silently suppresses child shafts.
        let mut candidate = doc.clone();
        let Some(g) = candidate.graphics.iter().find(|g| g.id == id) else {
            continue;
        };
        let Some(cycle) = g.cycle.as_deref() else {
            continue;
        };
        let n = cycle.intermediates.len();
        let layout = Layout::graphic(&candidate, g);
        let bad: Vec<_> = cycle
            .split_runs
            .iter()
            .filter(|run| {
                (0..=run.cuts.len()).any(|child| {
                    let slot = (run.first_step + child) % n;
                    layout
                        .as_ref()
                        .and_then(|l| l.steps.get(slot))
                        .and_then(Option::as_ref)
                        .is_none()
                })
            })
            .cloned()
            .collect();
        let parts: Vec<_> = bad
            .iter()
            .flat_map(|run| {
                run.cuts.iter().enumerate().filter_map(|(cut, &u)| {
                    Some((
                        cycle
                            .intermediates
                            .get((run.first_step + cut + 1) % n)?
                            .clone(),
                        u,
                    ))
                })
            })
            .collect();
        if let Some(cycle) = candidate
            .graphics
            .iter_mut()
            .find(|g| g.id == id)
            .and_then(|g| g.cycle.as_deref_mut())
        {
            cycle
                .split_runs
                .retain(|run| !bad.iter().any(|bad| bad.first_step == run.first_step));
        }
        if reflow_native(&mut candidate, id, &parts) && valid(&candidate, id) {
            *doc = candidate;
        }
    }
}
