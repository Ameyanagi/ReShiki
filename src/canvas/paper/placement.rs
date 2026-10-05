//! Previews that place new structure onto the draft, stroking the conflict when a placement fails.

use super::Draft;
use crate::canvas::{Gesture, MoleculeCanvas, State, layered, rgb};
use iced::widget::canvas::{Path, Stroke};
use iced::{Point, Rectangle};
use reshiki::chains;
use std::borrow::Cow;

impl MoleculeCanvas<'_> {
    pub(in crate::canvas) fn preview_chain(
        &self,
        draft: &mut Draft<'_>,
        frame: &mut layered::Frame<'_>,
        state: &State,
        bounds: Rectangle,
    ) {
        let Draft {
            preview,
            ring_selection,
            chain_badge,
            ..
        } = draft;
        if let (
            Some(Gesture::Chain {
                start,
                pressed,
                source,
                points,
                snaking,
                dragged,
            }),
            Some(p),
        ) = (&state.gesture, state.cursor)
        {
            let cursor = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let (points, target) = self.chain_plan(
                (*start, *pressed),
                *source,
                points,
                (*snaking, *dragged),
                cursor,
                state.modifiers,
            );
            let endpoint = target
                .and_then(|id| self.doc.atom(id).map(|a| a.position))
                .or_else(|| points.last().copied())
                .unwrap_or(*start);
            let added = points
                .len()
                .saturating_sub(usize::from(source.is_some()))
                .saturating_sub(usize::from(target.is_some() && points.len() > 1));
            let cancelled = *dragged && points.len() < 2;
            let placement = if cancelled {
                Ok((self.doc.clone(), vec![]))
            } else {
                chains::place(self.doc, &points, *source, target, 10.0 / self.camera.zoom)
            };
            match placement {
                Ok((doc, ids)) => {
                    *preview = Cow::Owned(doc);
                    *ring_selection = Some(ids);
                    *chain_badge = Some((
                        endpoint,
                        if cancelled {
                            "Release to cancel".into()
                        } else {
                            format!("{added} new C · {} bonds", points.len().saturating_sub(1))
                        },
                        true,
                    ));
                }
                Err(_) => {
                    for pair in points.windows(2) {
                        let [a, b] = pair else { continue };
                        frame.stroke(
                            &Path::line(
                                self.camera.screen(*a, bounds),
                                self.camera.screen(*b, bounds),
                            ),
                            Stroke::default()
                                .with_width(1.5)
                                .with_color(rgb([182, 66, 61])),
                        );
                    }
                    *chain_badge = Some((endpoint, "Overlap · change direction".into(), false));
                }
            }
        }
    }
}
