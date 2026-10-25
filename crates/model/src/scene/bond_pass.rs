//! Bond preparation has two stages: inputs own any resolved/projection copies,
//! then a pass borrows them. Joins borrows its document, so combining that index
//! with an owned document in one struct would make it self-referential.
//! Every consumer uses the drawn dative projection geometry, after collecting
//! labels from the chemical document.
//! Preparation never emits ink; callers retain their own ring/emission stages.

use super::{
    AtomLabels, BondScene, FaceIndex, RingStrokes, collect_atom_labels, projection_render_copy,
};
use crate::{bond_joins::Joins, document::Document, rear_opacity::Paint, style::DrawingStyle};
use std::borrow::Cow;

/// Occlusion keeps its supplied document; scene/picking resolve display colors.
pub(crate) enum Resolve {
    Scene,
    AsGiven,
}

/// Stage one, owned by the caller: the resolved document, its atom labels and
/// the optional dative projection copy shared by drawing, occlusion and picking.
pub(crate) struct BondInputs<'d> {
    doc: Cow<'d, Document>,
    labels: AtomLabels,
    projection: Option<Document>,
}

impl<'d> BondInputs<'d> {
    pub(crate) fn prepare(doc: &'d Document, opacity: &Paint, resolve: Resolve) -> Self {
        let doc = match resolve {
            Resolve::Scene => {
                let paint = crate::depth_appearance::Paint::new(doc);
                let painted = paint.materialize(doc);
                let canonical = crate::canvas_theme::canonical_document(&painted);
                let owned = match canonical {
                    Cow::Owned(doc) => Some(doc),
                    Cow::Borrowed(_) => None,
                };
                owned.map_or(painted, Cow::Owned)
            }
            Resolve::AsGiven => Cow::Borrowed(doc),
        };
        // Reversing a dative graph changes donor hydrogen counts. Labels must
        // use the resolved chemical document, before projection normalization.
        let labels = collect_atom_labels(&doc, opacity);
        let projection = match projection_render_copy(&doc) {
            Cow::Owned(doc) => Some(doc),
            Cow::Borrowed(_) => None,
        };
        Self {
            doc,
            labels,
            projection,
        }
    }

    pub(crate) fn doc(&self) -> &Document {
        &self.doc
    }

    pub(crate) fn bond_doc(&self) -> &Document {
        self.projection.as_ref().unwrap_or(&self.doc)
    }

    pub(crate) fn labels(&self) -> &AtomLabels {
        &self.labels
    }
}

/// Stage two, borrowed from [`BondInputs`]: the bond document with its joins
/// and its projected-face index, both built once per pass.
pub(crate) struct BondPass<'a> {
    pub(crate) doc: &'a Document,
    pub(crate) labels: &'a AtomLabels,
    pub(crate) joins: Joins<'a>,
    faces: FaceIndex<'a>,
}

impl<'a> BondPass<'a> {
    pub(crate) fn new(inputs: &'a BondInputs<'_>) -> Self {
        let doc = inputs.bond_doc();
        let faces = FaceIndex::new(doc);
        let joins = Joins::with_faces(doc, &faces);
        Self {
            doc,
            labels: inputs.labels(),
            joins,
            faces,
        }
    }

    pub(super) fn scene<'s>(
        &'s self,
        style: &'s DrawingStyle,
        rings: &'s RingStrokes,
        opacity: &'s Paint,
    ) -> BondScene<'s, 'a> {
        BondScene {
            doc: self.doc,
            style,
            arcs: &rings.arcs,
            circles: &rings.circles,
            label_bounds: &self.labels.bounds,
            label_runs: &self.labels.runs,
            joins: &self.joins,
            faces: &self.faces,
            crossing_gaps: &rings.crossing_gaps,
            opacity,
        }
    }
}
