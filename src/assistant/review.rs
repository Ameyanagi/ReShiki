use super::{
    Proposal,
    composition::{self, Composition, Role},
};
use crate::{
    document::{Document, Point},
    editing, scene,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Report {
    pub passes: usize,
    pub summary: String,
    #[serde(default)]
    pub changes: Vec<String>,
    pub issues: Vec<String>,
    pub verified: bool,
}
impl Report {
    pub fn can_auto_apply(&self) -> bool {
        self.verified && self.issues.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub proposal: Proposal,
    pub document: Document,
    pub review: Report,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Critique {
    pub summary: String,
    pub issues: Vec<String>,
    pub edits: Vec<Edit>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Move {
        target: String,
        dx_pt: f32,
        dy_pt: f32,
    },
    Rotate {
        target: String,
        degrees: f32,
    },
    ArrowLength {
        target: String,
        length_pt: f32,
    },
    Compact {
        target: String,
    },
    Arrange {
        composition: Composition,
        main_reaction: usize,
    },
}
pub fn schema() -> Value {
    let target = json!({"type":"string","description":"Exact target name from editable_targets; never an invented object ID."});
    let number = json!({"type":"number","minimum":-1800,"maximum":1800});
    json!({"type":"object","additionalProperties":false,"properties":{
        "summary":{"type":"string","description":"Brief visible explanation of improvements or why the exact image is ready."},
        "issues":{"type":"array","items":{"type":"string"},"description":"Unresolved visual or chemistry problems in this exact image. Empty only when ready. Do not claim chemistry correctness solely from visual appearance."},
        "edits":{"type":"array","items":{"anyOf":[
            {"type":"object","additionalProperties":false,"properties":{"action":{"const":"move","type":"string"},"target":target,"dx_pt":number,"dy_pt":number},"required":["action","target","dx_pt","dy_pt"]},
            {"type":"object","additionalProperties":false,"properties":{"action":{"const":"rotate","type":"string"},"target":target,"degrees":{"type":"number","enum":[-360,-330,-300,-270,-240,-210,-180,-150,-120,-90,-60,-30,0,30,60,90,120,150,180,210,240,270,300,330,360]}},"required":["action","target","degrees"]},
            {"type":"object","additionalProperties":false,"properties":{"action":{"const":"arrow_length","type":"string"},"target":target,"length_pt":{"type":"number","minimum":12,"maximum":400}},"required":["action","target","length_pt"]},
            {"type":"object","additionalProperties":false,"properties":{"action":{"const":"compact","type":"string"},"target":target},"required":["action","target"]},
            {"type":"object","additionalProperties":false,"properties":{"action":{"const":"arrange","type":"string"},"composition":composition::schema(),"main_reaction":{"type":"integer","minimum":0,"maximum":7}},"required":["action","composition","main_reaction"]}
        ]}}
    },"required":["summary","issues","edits"]})
}

#[derive(Debug, Clone, Serialize)]
pub struct Target {
    pub name: String,
    pub kind: String,
    pub ids: Vec<u64>,
    pub bounds_pt: [f32; 4],
    pub text: String,
}
impl Target {
    pub fn label(&self) -> String {
        match self.kind.as_str() {
            "panel" => format!(
                "Reaction {}",
                self.name
                    .trim_start_matches("reaction:")
                    .parse::<usize>()
                    .unwrap_or(0)
                    + 1
            ),
            "molecule" => format!(
                "Molecule {}",
                self.name
                    .trim_start_matches("molecule:")
                    .parse::<usize>()
                    .unwrap_or(0)
                    + 1
            ),
            "caption" => format!(
                "Caption: {} ({})",
                self.text.chars().take(32).collect::<String>(),
                self.ids.first().copied().unwrap_or(0)
            ),
            _ => format!("Arrow {}", self.ids.first().copied().unwrap_or(0)),
        }
    }
}
/// A layout review moves complete chemical units, including all members of selected reactions.
pub fn scope(doc: &Document, selected: &[u64]) -> Vec<u64> {
    if selected.is_empty() {
        return doc.all_ids();
    }
    let mut ids = doc.expand_abbreviation_selection(selected);
    loop {
        let before = ids.len();
        for reaction in &doc.reactions {
            if reaction.ids().iter().any(|id| ids.contains(id)) {
                ids.extend(reaction.ids());
            }
        }
        let atoms: Vec<_> = crate::reactions::molecules(doc, &ids)
            .into_iter()
            .flatten()
            .collect();
        ids.extend(atoms);
        ids.sort_unstable();
        ids.dedup();
        if ids.len() == before {
            break;
        }
    }
    ids
}

/// Isolate complete chemical units for inspection without invalidating their rendered labels.
pub fn fragment(doc: &Document, ids: &[u64]) -> Document {
    let mut part = editing::selection(doc, ids);
    for atom in &mut part.atoms {
        if let Some(original) = doc.atom(atom.id) {
            *atom = original.clone();
        }
    }
    for bond in &mut part.bonds {
        if let Some(original) = doc.bonds.iter().find(|b| b.a == bond.a && b.b == bond.b) {
            *bond = original.clone();
        }
    }
    part
}

pub fn targets(doc: &Document) -> Vec<Target> {
    let mut result = Vec::new();
    let mut add = |name: String, kind: &str, ids: Vec<u64>, text: String| {
        if let Some((lo, hi)) = scene::selection_bounds(doc, &ids) {
            let k = crate::style::DEFAULT.points_per_world();
            result.push(Target {
                name,
                kind: kind.into(),
                ids,
                bounds_pt: [lo.x * k, lo.y * k, hi.x * k, hi.y * k],
                text,
            });
        }
    };
    for (i, r) in doc.reactions.iter().enumerate() {
        let ids = if doc.reactions.len() > 1 && !super::branching::shared_source(doc).is_empty() {
            r.ids()
                .into_iter()
                .filter(|id| {
                    doc.reactions
                        .iter()
                        .filter(|r| r.ids().contains(id))
                        .count()
                        == 1
                })
                .collect()
        } else {
            r.ids()
        };
        add(format!("reaction:{i}"), "panel", ids, String::new());
    }
    for (i, ids) in crate::reactions::molecules(doc, &doc.all_ids())
        .into_iter()
        .enumerate()
    {
        add(format!("molecule:{i}"), "molecule", ids, String::new());
    }
    for a in &doc.arrows {
        add(
            format!("arrow:{}", a.id),
            "arrow",
            vec![a.id],
            a.kind.clone(),
        );
    }
    for a in &doc.annotations {
        add(
            format!("caption:{}", a.id),
            "caption",
            vec![a.id],
            a.text.clone(),
        );
    }
    result
}

/// Review operations can change presentation, never atoms, bonds, coefficients or caption text.
pub fn apply(doc: &Document, edits: &[Edit], compact_allowed: bool) -> Result<Document, String> {
    if edits.len() > 40 {
        return Err("Limit visual corrections to 40 operations".into());
    }
    let mut candidate = doc.clone();
    for edit in edits {
        if let Edit::Arrange {
            composition,
            main_reaction,
        } = edit
        {
            composition.validate()?;
            let shared = candidate.reactions.len() > 1
                && !super::branching::shared_source(&candidate).is_empty();
            if shared != (composition.arrangement == composition::Arrangement::Branching) {
                return Err("Keep the shared-source branching arrangement; changing reaction connectivity requires a new proposal".into());
            }
            if *main_reaction >= candidate.reactions.len() {
                return Err("Unknown main reaction".into());
            }
            let panels = candidate
                .reactions
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    (
                        r.ids(),
                        if i == *main_reaction {
                            Role::Main
                        } else {
                            Role::Example
                        },
                    )
                })
                .collect::<Vec<_>>();
            composition::arrange(&mut candidate, &panels, composition);
            continue;
        }
        let name = match edit {
            Edit::Move { target, .. }
            | Edit::Rotate { target, .. }
            | Edit::ArrowLength { target, .. }
            | Edit::Compact { target } => target,
            _ => continue,
        };
        let target = targets(&candidate)
            .into_iter()
            .find(|t| &t.name == name)
            .ok_or_else(|| format!("Unknown review target: {name}"))?;
        let bounded = |v: f32| v.is_finite() && v.abs() <= 1800.;
        match edit {
            Edit::Move { dx_pt, dy_pt, .. } if bounded(*dx_pt) && bounded(*dy_pt) => candidate
                .translate(
                    &target.ids,
                    crate::style::DEFAULT.world(*dx_pt),
                    crate::style::DEFAULT.world(*dy_pt),
                ),
            Edit::Rotate { degrees, .. }
                if bounded(*degrees)
                    && degrees.abs() <= 360.
                    && (*degrees / 30. - (*degrees / 30.).round()).abs() < 0.001
                    && target.kind == "molecule" =>
            {
                editing::transform(
                    &mut candidate,
                    &target.ids,
                    editing::Transform::Rotate(*degrees),
                )
            }
            Edit::ArrowLength { length_pt, .. }
                if length_pt.is_finite()
                    && (12. ..=400.).contains(length_pt)
                    && target.kind == "arrow" =>
            {
                if let Some(a) = candidate
                    .arrows
                    .iter_mut()
                    .find(|a| target.ids.contains(&a.id))
                {
                    let old = a.start.distance(a.end).max(0.001);
                    let factor = crate::style::DEFAULT.world(*length_pt) / old;
                    let center = Point::new((a.start.x + a.end.x) / 2., (a.start.y + a.end.y) / 2.);
                    let u = Point::new((a.end.x - a.start.x) / old, (a.end.y - a.start.y) / old);
                    a.map_points(|p| {
                        let d = (p.x - center.x) * u.x + (p.y - center.y) * u.y;
                        p.offset(u.x * d * (factor - 1.), u.y * d * (factor - 1.))
                    });
                }
            }
            Edit::Compact { .. } if compact_allowed && target.kind == "molecule" => {
                composition::compact_chains(&mut candidate, &target.ids)?;
            }
            _ => {
                return Err(
                    "Invalid visual edit or requested structural detail would be hidden".into(),
                );
            }
        }
    }
    candidate.validate()?;
    // Every mutation above keeps the molecular graph and all reaction membership intact.
    if candidate.atoms.len() != doc.atoms.len()
        || candidate.atoms.iter().zip(&doc.atoms).any(|(a, b)| {
            let mut original_position = a.clone();
            original_position.position = b.position;
            &original_position != b
        })
        || candidate.bonds != doc.bonds
        || candidate.reactions != doc.reactions
        || candidate
            .annotations
            .iter()
            .zip(&doc.annotations)
            .any(|(a, b)| a.text != b.text)
    {
        return Err("Visual correction changed chemical data".into());
    }
    Ok(candidate)
}

pub fn quality(doc: &Document, composition: &Composition) -> Vec<String> {
    let mut issues = Vec::new();
    let ts = targets(doc);
    for (i, a) in ts.iter().enumerate() {
        if a.kind == "panel" {
            continue;
        }
        for b in ts.iter().skip(i + 1).filter(|b| b.kind != "panel") {
            let overlap_x = a.bounds_pt[2].min(b.bounds_pt[2]) - a.bounds_pt[0].max(b.bounds_pt[0]);
            let overlap_y = a.bounds_pt[3].min(b.bounds_pt[3]) - a.bounds_pt[1].max(b.bounds_pt[1]);
            if overlap_x > 0.5 && overlap_y > 0.5 {
                issues.push(format!("{} overlaps {}", a.label(), b.label()));
            }
        }
    }
    if let Some((lo, hi)) = scene::selection_bounds(doc, &doc.all_ids()) {
        let width = (hi.x - lo.x) * crate::style::DEFAULT.points_per_world();
        if width > composition.width_pt + 2. {
            issues.push(format!("Figure is {width:.0} pt wide; target is {:.0} pt. Recompose or compact long chains; preserve bond lengths.", composition.width_pt));
        }
    }
    // Captions used for names should remain near some molecular structure.
    for caption in ts
        .iter()
        .filter(|t| t.kind == "caption" && t.text.chars().any(char::is_alphabetic))
    {
        let near = ts
            .iter()
            .filter(|t| t.kind == "molecule" || t.kind == "arrow")
            .any(|t| {
                let dx = (t.bounds_pt[0] - caption.bounds_pt[2])
                    .max(caption.bounds_pt[0] - t.bounds_pt[2])
                    .max(0.);
                let dy = (t.bounds_pt[1] - caption.bounds_pt[3])
                    .max(caption.bounds_pt[1] - t.bounds_pt[3])
                    .max(0.);
                dx.hypot(dy) <= doc.drawing_style.bond_length_pt * 3.
            });
        if !near {
            issues.push(format!(
                "{} is far from its structure or arrow",
                caption.label()
            ));
        }
    }
    for (index, r) in doc.reactions.iter().enumerate() {
        if !r.ready() {
            continue;
        }
        let totals = |parts: &[crate::reactions::Participant]| {
            let mut elements = std::collections::BTreeMap::<String, i64>::new();
            let mut charge = 0i64;
            for part in parts {
                for atom in part.atoms.iter().filter_map(|id| doc.atom(*id)) {
                    let coefficient = i64::from(part.coefficient);
                    let element = if atom.element == "*" {
                        format!("R{}", atom.map_num)
                    } else {
                        atom.element.clone()
                    };
                    *elements.entry(element).or_default() += coefficient;
                    if atom.label_h > 0 {
                        *elements.entry("H".into()).or_default() +=
                            i64::from(atom.label_h) * coefficient;
                    }
                    charge += i64::from(atom.charge) * coefficient;
                }
            }
            (elements, charge)
        };
        if totals(&r.reactants) != totals(&r.products) {
            issues.push(format!("Reaction {}: displayed participants are not element/charge balanced. Check coefficients and any reagents shown only in conditions.", index + 1));
        }
    }
    issues.truncate(30);
    issues
}

pub fn images(doc: &Document) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut images = vec![("Complete scheme".into(), super::canvas_tools::image(doc)?)];
    for (i, r) in doc.reactions.iter().enumerate() {
        images.push((
            format!("Reaction panel {}", i + 1),
            super::canvas_tools::image(&fragment(doc, &r.ids()))?,
        ));
    }
    if doc.reactions.is_empty() && doc.atoms.len() > 40 {
        for t in targets(doc)
            .into_iter()
            .filter(|t| t.kind == "molecule")
            .take(8)
        {
            images.push((t.name, super::canvas_tools::image(&fragment(doc, &t.ids))?));
        }
    }
    Ok(images)
}
