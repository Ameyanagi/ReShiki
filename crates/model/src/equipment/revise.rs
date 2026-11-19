//! Moving placed pieces to another revision of their definition, as a
//! library offers it. Nothing updates on its own: the user asks.
use super::{
    eval::{self, Values},
    format::Definition,
    placed::key,
    solve::{self, Seat},
};
use crate::document::Document;
use std::sync::Arc;

/// What an update took away.
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    /// Seats whose joints no longer fit.
    pub released: Vec<Seat>,
    /// Parameters, by ID, that went back to the new revision's default.
    pub reset: Vec<String>,
}

/// A value `next` takes as it is.
fn keeps(next: &Definition, k: &str, v: &eval::Value) -> bool {
    let one = Values::from([(k.to_owned(), v.clone())]);
    eval::resolve(next, &one).is_ok_and(|r| r.get(k) == Some(v))
}

/// What [`update`] would take away, without changing the drawing: the
/// update itself, on a copy of the drawing's equipment.
pub fn preview(doc: &Document, ids: &[u64], next: &Arc<Definition>) -> Outcome {
    let mut copy = Document {
        drawing_style: doc.drawing_style.clone(),
        graphics: (doc.graphics.iter())
            .filter(|g| g.piece.is_some())
            .cloned()
            .collect(),
        seats: doc.seats.clone(),
        ..Document::default()
    };
    update(&mut copy, ids, next)
}

/// Point the pieces `ids` at `next`. A value `next` cannot take goes back to
/// its default, and a seat whose joints no longer fit is released; the
/// pieces stay where they are.
pub fn update(doc: &mut Document, ids: &[u64], next: &Arc<Definition>) -> Outcome {
    let mut reset = Vec::new();
    for g in doc.graphics.iter_mut().filter(|g| ids.contains(&g.id)) {
        let Some(piece) = g.piece.as_mut() else {
            continue;
        };
        piece.params.retain(|k, v| {
            let keep = keeps(next, k, v);
            if !keep && !reset.contains(k) {
                reset.push(k.clone());
            }
            keep
        });
        piece.definition = key(next);
        piece.linked = Some(next.clone());
    }
    let before = doc.seats.clone();
    solve::check(doc, None);
    let released = before
        .into_iter()
        .filter(|s| !doc.seats.contains(s))
        .collect();
    Outcome { released, reset }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Point;
    use crate::equipment::{builtin, eval::Value, format, placed, solve::JointRef};

    fn place(doc: &mut Document, d: Arc<Definition>, at: Point) -> u64 {
        let n = doc.next_id();
        let scale = placed::world_per_mm(doc, placed::DEFAULT_PT_PER_MM);
        doc.graphics.push(placed::graphic(n, d, at, scale));
        n
    }

    /// A flask with a stopper seated in its neck, the flask a lab's
    /// revision 1, and a half-full level.
    fn setup() -> (Document, u64, Arc<Definition>) {
        let mut d = (*builtin::find("reshiki/rbf-100-15").unwrap()).clone();
        d.id = "lab/flask".into();
        let mut doc = Document::default();
        let flask = place(&mut doc, Arc::new(d.clone()), Point::new(200., 300.));
        let stopper = place(
            &mut doc,
            builtin::find("reshiki/stopper-15").unwrap(),
            Point::new(300., 100.),
        );
        let neck = JointRef {
            piece: flask,
            joint: "neck".into(),
        };
        let bottom = JointRef {
            piece: stopper,
            joint: "bottom".into(),
        };
        solve::connect(&mut doc, &[stopper], &neck, &bottom).unwrap();
        let g = doc.graphics.iter_mut().find(|g| g.id == flask).unwrap();
        let piece = g.piece.as_mut().unwrap();
        piece.params.insert("fill".into(), Value::Number(0.5));
        d.revision = 2;
        (doc, flask, Arc::new(d))
    }

    #[test]
    fn a_revision_with_the_same_joints_keeps_seats_and_values() {
        let (mut doc, flask, next) = setup();
        let mut next_def = (*next).clone();
        next_def.keywords.push("new".into());
        let next = Arc::new(next_def);
        assert_eq!(preview(&doc, &[flask], &next), Outcome::default());
        assert_eq!(update(&mut doc, &[flask], &next), Outcome::default());
        assert_eq!(doc.seats.len(), 1);
        let piece = doc.graphics[0].piece.as_ref().unwrap();
        assert_eq!(piece.definition, key(&next));
        assert_eq!(piece.params["fill"], Value::Number(0.5));
    }

    #[test]
    fn a_revision_that_moves_the_neck_releases_the_seat_and_resets_lost_values() {
        let (mut doc, flask, next) = setup();
        let mut next_def = (*next).clone();
        next_def.params.retain(|p| p.id() != "fill");
        next_def.liquids.clear();
        match &mut next_def.joints[0] {
            format::Joint::Taper { at, .. } => at[1] -= 5.,
            other => panic!("{other:?}"),
        }
        let next = Arc::new(next_def);
        let expected = preview(&doc, &[flask], &next);
        let outcome = update(&mut doc, &[flask], &next);
        assert_eq!(expected, outcome);
        assert_eq!(outcome.released.len(), 1);
        assert_eq!(outcome.reset, ["fill"]);
        assert!(doc.seats.is_empty());
    }
}
