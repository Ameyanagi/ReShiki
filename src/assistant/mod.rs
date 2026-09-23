//! Reviewed, editable drawing proposals. Model output never mutates the document.
mod branching;
pub mod canvas_tools;
pub mod codex;
pub mod composition;
mod layout;
pub mod review;
pub mod settings;
pub mod sketch;
use crate::{
    document::{Document, Point},
    engine::LocalEngine,
    typography::TextFormat,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Molecule {
    pub smiles: String,
    pub label: String,
    #[serde(default = "one")]
    pub coefficient: u16,
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub compact: bool,
}
impl Default for Molecule {
    fn default() -> Self {
        Self {
            smiles: String::new(),
            label: String::new(),
            coefficient: 1,
            rotation: 0.,
            compact: false,
        }
    }
}
fn one() -> u16 {
    1
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub reactants: Vec<Molecule>,
    pub products: Vec<Molecule>,
    pub conditions: String,
    pub arrow: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub role: composition::Role,
    #[serde(default)]
    pub direction: Option<f32>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub explanation: String,
    #[serde(default)]
    pub replace_ids: Vec<u64>,
    pub molecules: Vec<Molecule>,
    pub reactions: Vec<Step>,
    #[serde(default)]
    pub composition: composition::Composition,
    /// A bounded editable diagram for drawings that SMILES cannot depict faithfully.
    #[serde(default)]
    pub sketch: Option<sketch::Sketch>,
}
impl Proposal {
    pub fn validate(&self) -> Result<(), String> {
        self.composition.validate()?;
        if let Some(sketch) = &self.sketch {
            if !self.molecules.is_empty() || !self.reactions.is_empty() {
                return Err("Use either a diagram or SMILES molecules/reactions, not both".into());
            }
            sketch.validate()?;
        }
        if self.explanation.len() > 12_000
            || self.reactions.len() > 8
            || self.replace_ids.len() > 5000
            || self
                .replace_ids
                .iter()
                .any(|id| *id == 0 || *id == u64::MAX)
            || self
                .replace_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != self.replace_ids.len()
        {
            return Err("The proposal is too large; request a smaller drawing".into());
        }
        let molecules: Vec<_> = self
            .molecules
            .iter()
            .chain(
                self.reactions
                    .iter()
                    .flat_map(|r| r.reactants.iter().chain(&r.products)),
            )
            .collect();
        if molecules.len() > 32
            || molecules.iter().any(|m| {
                m.smiles.trim().is_empty()
                    || (self.composition.preserve_details && m.compact)
                    || m.smiles.len() > 4000
                    || m.label.len() > 300
                    || !(1..=99).contains(&m.coefficient)
                    || !m.rotation.is_finite()
                    || m.rotation.abs() > 360.
                    || (m.rotation / 30. - (m.rotation / 30.).round()).abs() > 0.001
            })
        {
            return Err("Use at most 32 molecules with short labels and valid SMILES".into());
        }
        for r in &self.reactions {
            if r.reactants.is_empty()
                || r.products.is_empty()
                || r.conditions.len() > 1200
                || r.title.len() > 200
                || r.direction
                    .is_some_and(|v| !v.is_finite() || v.abs() > 360.)
                || !["forward", "equilibrium", "retro"].contains(&r.arrow.as_str())
            {
                return Err("Each reaction needs reactants, products, short conditions and a supported arrow".into());
            }
        }
        if self.composition.arrangement == composition::Arrangement::Branching {
            let source = self.reactions.first().and_then(|r| r.reactants.first());
            if !self.molecules.is_empty()
                || self.reactions.len() < 2
                || self.reactions.iter().any(|r| {
                    r.reactants.len() != 1
                        || r.reactants.first().zip(source).is_none_or(|(a, b)| {
                            a.smiles != b.smiles
                                || a.coefficient != b.coefficient
                                || a.compact != b.compact
                        })
                })
            {
                return Err("A branching scheme needs at least two reactions sharing the same single central reactant (identical SMILES, coefficient and compact setting). Put reagents in conditions.".into());
            }
        }
        Ok(())
    }
    pub fn has_drawing(&self) -> bool {
        self.sketch.is_some() || !self.molecules.is_empty() || !self.reactions.is_empty()
    }
}
pub fn schema() -> Value {
    let molecule = json!({"type":"object","additionalProperties":false,"properties":{"smiles":{"type":"string"},"label":{"type":"string"},"coefficient":{"type":"integer","minimum":1,"maximum":99},"rotation":{"type":"number","enum":[-360,-330,-300,-270,-240,-210,-180,-150,-120,-90,-60,-30,0,30,60,90,120,150,180,210,240,270,300,330,360],"description":"Rotate in 30-degree increments to orient functional groups on the conventional bond grid."},"compact":{"type":"boolean","description":"Condense long carbon chains into expandable formula labels while retaining the full molecular graph. False when explicit chain detail is requested."}},"required":["smiles","label","coefficient","rotation","compact"]});
    let molecules = json!({"type":"array","items":molecule});
    json!({"type":"object","additionalProperties":false,"properties":{
        "explanation":{"type":"string","description":"Brief explanation or clarification question. Do not claim the drawing was applied."},
        "replace_ids":{"type":"array","items":{"type":"integer","minimum":1},"description":"Existing object IDs from canvas_inspect to replace when the user asks to revise existing content. Empty for additions. Preserve unrelated objects."},
        "molecules":molecules,
        "composition":composition::schema(),
        "sketch":sketch::schema(),
        "reactions":{"type":"array","items":{"type":"object","additionalProperties":false,"properties":{
            "reactants":molecules,"products":molecules,"conditions":{"type":"string"},"arrow":{"type":"string","enum":["forward","equilibrium","retro"]},"title":{"type":"string"},"direction":{"type":["number","null"],"description":"Branch arrow direction in degrees: 0 right, 90 down, 180 left, -90 up. Null distributes branches automatically. Used for branching arrangements."},"role":{"type":"string","enum":["main","example","reaction"]}
        },"required":["reactants","products","conditions","arrow","title","role","direction"]}}
    },"required":["explanation","replace_ids","molecules","reactions","composition","sketch"]})
}

#[derive(Debug, Clone)]
pub struct DrawingSettings {
    pub drawing_style: crate::style::DrawingStyle,
    pub format: TextFormat,
    pub bond_length: f32,
    pub bond_color: [u8; 3],
    pub arrow_style: crate::arrows::ArrowStyle,
    pub labels: crate::atom_labels::Settings,
}
impl Default for DrawingSettings {
    fn default() -> Self {
        Self {
            drawing_style: Default::default(),
            format: TextFormat::default(),
            bond_length: crate::style::DEFAULT.bond_length_world,
            bond_color: [0; 3],
            arrow_style: Default::default(),
            labels: Default::default(),
        }
    }
}
/// Validate and compose an editable scheme using the active drawing style.
pub async fn render(
    engine: &LocalEngine,
    proposal: &Proposal,
    settings: &DrawingSettings,
) -> Result<Document, String> {
    layout::render(engine, proposal, settings).await
}

pub fn candidate(
    base: &Document,
    fragment: &Document,
    replace: &[u64],
) -> Result<(Document, Vec<u64>), String> {
    base.validate()?;
    fragment.validate()?;
    let target = if replace.is_empty() {
        None
    } else {
        crate::scene::selection_bounds(base, replace)
    };
    let mut doc = base.clone();
    if !replace.is_empty() {
        doc.delete(replace);
    }
    let (lo, hi) = crate::scene::selection_bounds(fragment, &fragment.all_ids())
        .ok_or("There are no drawing objects to apply")?;
    let origin = if let Some((a, b)) = target {
        Point::new(
            (a.x + b.x - lo.x - hi.x) / 2.,
            (a.y + b.y - lo.y - hi.y) / 2.,
        )
    } else if let Some((a, b)) = crate::scene::selection_bounds(base, &base.all_ids()) {
        Point::new(
            a.x - lo.x,
            b.y + crate::style::DEFAULT.bond_length_world * 2. - lo.y,
        )
    } else {
        Point::new(-lo.x, -lo.y)
    };
    let ids = crate::editing::append(&mut doc, fragment, origin);
    if ids.is_empty() {
        return Err("Could not insert the proposed drawing".into());
    }
    doc.validate()?;
    Ok((doc, ids))
}
