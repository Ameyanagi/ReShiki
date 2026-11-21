//! Disposable pieces for testing the draft with the ordinary seating solver.
use reshiki::document::{Document, Point};
use reshiki::equipment::{builtin, format::Definition, placed, solve};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub(super) definition: Arc<Definition>,
    key: String,
}

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(super::super::equipment::name(&self.definition))
    }
}

pub(super) fn choices(doc: &Document, identity: &str) -> Vec<Choice> {
    let mut choices = Vec::new();
    let mut seen = BTreeSet::new();
    for definition in builtin::catalogue().iter().chain(&doc.piece_definitions) {
        if definition.id != identity && !definition.joints.is_empty() {
            let key = placed::key(definition);
            if seen.insert(key.clone()) {
                choices.push(Choice {
                    definition: definition.clone(),
                    key,
                });
            }
        }
    }
    choices
}

pub(super) struct Trial {
    pub choice: Choice,
    pub choices: Vec<Choice>,
    pub at: Point,
    pub turn: f32,
    pub scale: f32,
    pub reach: f32,
    pub epoch: u64,
    pub id: Option<u64>,
    pub snap: Option<solve::Snap>,
    drag_start: Option<DragStart>,
}

struct DragStart {
    at: Point,
    turn: f32,
    scale: f32,
    reach: f32,
    id: Option<u64>,
    snap: Option<solve::Snap>,
    shown: Document,
}

impl Trial {
    pub fn new(choice: Choice, choices: Vec<Choice>, at: Point, scale: f32, reach: f32) -> Self {
        let mut trial = Self {
            choice,
            choices,
            at,
            turn: 0.,
            scale,
            reach,
            epoch: 0,
            id: None,
            snap: None,
            drag_start: None,
        };
        trial.invalidate();
        trial
    }

    /// A cancellation restores the visible fit, including an adapter whose
    /// stopper pose alone may no longer reach the original target.
    pub fn begin_drag(&mut self, shown: &Document) {
        self.drag_start = Some(DragStart {
            at: self.at,
            turn: self.turn,
            scale: self.scale,
            reach: self.reach,
            id: self.id,
            snap: self.snap.clone(),
            shown: shown.clone(),
        });
        self.keep_pose(shown);
    }

    pub fn finish_drag(&mut self, shown: &Document) {
        self.keep_pose(shown);
        self.drag_start = None;
    }

    pub fn cancel_drag(&mut self) -> Option<Document> {
        let start = self.drag_start.take()?;
        self.at = start.at;
        self.turn = start.turn;
        self.scale = start.scale;
        self.reach = start.reach;
        self.id = start.id;
        self.snap = start.snap;
        Some(start.shown)
    }

    /// Keep only the painted test piece's pose between drags. The document,
    /// targets, adapters and seats remain disposable.
    fn keep_pose(&mut self, shown: &Document) {
        if let Some(g) = shown.graphics.iter().find(|g| Some(g.id) == self.id) {
            self.at = placed::middle(g);
            self.scale = g.axis_x.x.hypot(g.axis_x.y);
            self.turn = g.axis_x.y.atan2(g.axis_x.x).to_degrees();
        }
    }

    pub fn invalidate(&mut self) {
        static EPOCH: AtomicU64 = AtomicU64::new(1);
        self.epoch = EPOCH.fetch_add(1, Ordering::Relaxed);
        self.id = None;
        self.snap = None;
        self.drag_start = None;
    }

    /// Always start from the canonical draft. A previous trial's seats,
    /// adapters and moved targets cannot become inputs to the next one.
    pub fn render(&mut self, base: &Document) -> Document {
        let mut doc = base.clone();
        let id = doc.next_id();
        let mut piece = placed::graphic(id, self.choice.definition.clone(), self.at, self.scale);
        piece.style.effects.opacity = Some(0.5);
        if !doc.piece_definitions.iter().any(|d| {
            d.id == self.choice.definition.id
                && d.revision == self.choice.definition.revision
                && placed::key(d) == self.choice.key
        }) {
            doc.piece_definitions.push(self.choice.definition.clone());
        }
        doc.graphics.push(piece);
        reshiki::editing::transform_about(&mut doc, &[id], self.at, 1., self.turn);
        // A test drag uses the normal 40-degree alignment requirement.
        self.snap = solve::snap(&mut doc, &[id], self.reach, false);
        if let Some(adapter) = self.snap.as_ref().and_then(|s| s.added)
            && let Some(graphic) = doc.graphics.iter_mut().find(|g| g.id == adapter)
        {
            graphic.style.effects.opacity = Some(0.5);
        }
        self.id = Some(id);
        doc
    }
}
