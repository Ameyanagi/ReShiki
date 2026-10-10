//! GUI-free edit finalization shared by the app and future API hosts.
//!
//! An edit mutates a [`Document`] directly and then hands the pre-edit snapshot
//! to [`reconcile`], which repairs derived state and either accepts the edit or
//! rolls the document back with a [`Rejection`]. [`Reconciled::commit`] then
//! clears computed labels after chemistry changes and records the undo frame.
//! [`apply`] runs both phases for hosts without observers.
//!
//! Hosts keep the UI bookkeeping: status and error text, revision, cleanup
//! preview, cached analysis, label refresh, observer context, selection pruning
//! and autosave.
//!
//! The candidate pipelines at chains.rs:262, editing.rs:1177, joining.rs:210,
//! rings.rs:236, templates.rs:271/729 and src/keyboard_drawing.rs:520
//! intentionally use their own order (groups before reactions) and are NOT
//! routed here.
use crate::document::{Document, History};

/// Whether the edit from `before` to `after` changes chemistry rather than only
/// its presentation.
///
/// Bond colors, highlights, z-order, double-bond placement, secondary display,
/// stereo indicators and CIP labels, and atom text styles, marks, label display,
/// CIP labels and label hydrogens are display-only. Aromatic bonds and matching
/// projection bonds also ignore their display and direction. Everything else
/// counts, including atom positions, so any atom move is a chemistry change.
pub fn chemistry_changed(before: &Document, after: &Document) -> bool {
    before.bonds.len() != after.bonds.len()
        || before.bonds.iter().zip(&after.bonds).any(|(a, b)| {
            let mut a = a.clone();
            let mut b = b.clone();
            a.z_order = 0;
            b.z_order = 0;
            a.color = Default::default();
            b.color = Default::default();
            a.highlight = None;
            b.highlight = None;
            a.double_position = Default::default();
            b.double_position = Default::default();
            a.secondary_display = None;
            b.secondary_display = None;
            a.indicator = Default::default();
            b.indicator = Default::default();
            a.cip_label = None;
            b.cip_label = None;
            if a.order == 4 && b.order == 4 || a.order == b.order && a.projection && b.projection {
                a.display = "plain".into();
                b.display = "plain".into();
                a.projection = false;
                b.projection = false;
                if a.a > a.b {
                    a.reverse();
                }
                if b.a > b.b {
                    b.reverse();
                }
            }
            a != b
        })
        || before.atoms.len() != after.atoms.len()
        || before.atoms.iter().zip(&after.atoms).any(|(a, b)| {
            let mut a = a.clone();
            let mut b = b.clone();
            a.text_style = None;
            b.text_style = None;
            a.marks.clear();
            b.marks.clear();
            a.mark_serial = 0;
            b.mark_serial = 0;
            a.display = Default::default();
            b.display = Default::default();
            a.cip_label = None;
            b.cip_label = None;
            a.label_h = 0;
            b.label_h = 0;
            a != b
        })
}

/// Why [`reconcile`] rolled an edit back.
///
/// The app shows a `Reactions` message as is and prefixes an `Invalid` message
/// with "Edit cancelled: ".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    /// The edit joins separate reaction participants.
    Reactions(String),
    /// The reconciled document fails [`Document::validate`].
    Invalid(String),
}

impl Rejection {
    /// The unprefixed message.
    pub fn message(&self) -> &str {
        match self {
            Self::Reactions(message) | Self::Invalid(message) => message,
        }
    }
}

/// An accepted, reconciled edit that still has to be committed.
///
/// Observers that mirror [`History`] must run between [`reconcile`] and
/// [`Reconciled::commit`]: they see [`Reconciled::before`] and the reconciled
/// document, whose computed labels are not cleared yet.
#[must_use]
pub struct Reconciled {
    before: Document,
}

/// What [`Reconciled::commit`] changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Committed {
    /// History recorded the edit; `false` when the document is unchanged.
    pub recorded: bool,
    /// [`chemistry_changed`] held, so computed labels were cleared.
    pub chemistry_changed: bool,
    /// The drawing style differs from the pre-edit snapshot.
    pub drawing_style_changed: bool,
}

/// Reconciles `doc` after an edit from `before`, or rolls it back to `before`.
///
/// The stages run in a fixed order:
/// 1. Centroids follow their members, and stale ring fills and depth scopes are
///    pruned.
/// 2. Abbreviations the edit broke are dropped before validation, which would
///    reject them.
/// 3. Reaction participants grow with their molecules. A join of separate
///    participants is rejected here, before validation.
/// 4. Only a changed document is validated. Molecule groups are reconciled after
///    validation, because their final prune would repair a stale member that
///    validation rejects.
pub fn reconcile(doc: &mut Document, before: Document) -> Result<Reconciled, Rejection> {
    crate::projection::sync_centroids(doc);
    crate::ring_fills::prune(doc);
    crate::depth_appearance::prune(doc);
    doc.reconcile_abbreviations(&before);
    crate::arrow_anchors::reconcile(doc);
    if let Err(error) = crate::reactions::reconcile(doc) {
        *doc = before;
        return Err(Rejection::Reactions(error));
    }
    crate::molecule_names::reconcile(doc, &before);
    if *doc != before {
        if let Err(error) = doc.validate() {
            *doc = before;
            return Err(Rejection::Invalid(error));
        }
        doc.reconcile_molecule_groups();
    }
    Ok(Reconciled { before })
}

impl Reconciled {
    /// The pre-edit snapshot, for observers that mirror [`History`].
    pub fn before(&self) -> &Document {
        &self.before
    }

    /// Clears computed labels after a chemistry change and records the edit.
    ///
    /// History records nothing when the document equals the snapshot. Otherwise
    /// a `continuing` frame keeps the newest undo snapshot, which is the start of
    /// the gesture, and pushes `before` only when the undo stack is empty; any
    /// recorded frame clears redo.
    pub fn commit(self, doc: &mut Document, history: &mut History, continuing: bool) -> Committed {
        let drawing_style_changed = self.before.drawing_style != doc.drawing_style;
        let chemistry_changed = chemistry_changed(&self.before, doc);
        if chemistry_changed {
            crate::atom_labels::clear_computed(doc);
        }
        let recorded = history.commit_continuing(self.before, doc, continuing);
        Committed {
            recorded,
            chemistry_changed,
            drawing_style_changed,
        }
    }
}

/// [`reconcile`], then [`Reconciled::commit`], for hosts without observers.
pub fn apply(
    doc: &mut Document,
    history: &mut History,
    before: Document,
    continuing: bool,
) -> Result<Committed, Rejection> {
    Ok(reconcile(doc, before)?.commit(doc, history, continuing))
}

#[cfg(test)]
mod tests;
