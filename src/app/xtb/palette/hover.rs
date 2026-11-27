//! Summary row feedback uses the drawing's existing palette highlight paint.
use super::{Action, App, Document, Message, Shown, View, is, results};
use iced::{Element, widget::mouse_area};
use reshiki::palette::{Color, Hue, Row};
use reshiki::qcdata::CalcResult;
use std::sync::{Arc, Weak};

/// Frame indices belong to both the saved result and its drawing attachment.
/// Copied molecules can share the same result with different atom IDs.
#[derive(Clone, Debug)]
pub(crate) struct BondHover {
    attachment: u64,
    result: Weak<CalcResult>,
    pair: [u32; 2],
}

impl BondHover {
    pub(super) fn new(shown: &Shown<'_>, pair: [u32; 2]) -> Self {
        Self {
            attachment: shown.stored.attachment,
            result: Arc::downgrade(&shown.stored.result),
            pair,
        }
    }

    fn same(&self, other: &Self) -> bool {
        self.attachment == other.attachment
            && self.result.ptr_eq(&other.result)
            && self.pair == other.pair
    }
}

pub(super) struct Preview {
    bond: BondHover,
    stamp: (u64, u64, u64),
    document: Box<Document>,
}

pub(super) fn row<'a, R: iced::advanced::Renderer + 'a>(
    content: impl Into<Element<'a, Message, iced::Theme, R>>,
    bond: BondHover,
) -> Element<'a, Message, iced::Theme, R> {
    let action = |entered| {
        Message::Xtb(super::super::Action::Palette(Action::HoverBond(
            bond.clone(),
            entered,
        )))
    };
    mouse_area(content)
        .on_enter(action(true))
        .on_exit(action(false))
        .into()
}

impl App {
    fn xtb_bond_row_visible(&self, bond: &BondHover) -> bool {
        self.xtb_palette_body_visible()
            && self.tab.xtb.run.is_none()
            && !self.xtb_palette_has_failure()
            && self.xtb_shown_entry().is_some_and(|(_, stored)| {
                super::view::effective_view(
                    self.tab.xtb.palette.view,
                    &View::available(&stored.result),
                ) == View::Summary
                    && stored.attachment == bond.attachment
                    && is(&bond.result, &stored.result)
            })
    }

    pub(in crate::app) fn xtb_hover_result_bond(&mut self, bond: BondHover, entered: bool) {
        if !entered {
            if self
                .tab
                .xtb
                .palette
                .hovered_bond
                .as_ref()
                .is_some_and(|hover| hover.bond.same(&bond))
            {
                self.tab.xtb.palette.hovered_bond = None;
            }
            return;
        }
        let preview = self.xtb_make_bond_hover(bond);
        self.tab.xtb.palette.hovered_bond = preview;
    }

    fn xtb_make_bond_hover(&self, bond: BondHover) -> Option<Preview> {
        if !self.xtb_bond_row_visible(&bond) {
            return None;
        }
        let shown = self.xtb_shown()?;
        if !results::displayed_bond_orders(&shown.stored.result)
            .any(|order| [order.a, order.b] == bond.pair)
        {
            return None;
        }
        let [a, b] = bond
            .pair
            .map(|index| shown.atoms.get(index as usize).copied().flatten());
        let (a, b) = (a?, b?);
        let doc = &self.tab.doc;
        let (index, _) = doc
            .bond_between(a, b)
            .filter(|(_, bond)| bond.order != 0 && doc.bond_visible(a, b))?;
        let mut document = Box::new(doc.clone());
        document.bonds.get_mut(index)?.highlight = Some(Color::Palette(Hue::Teal, Row::Tint));
        Some(Preview {
            bond,
            stamp: self.xtb_bond_hover_stamp(),
            document,
        })
    }

    /// Label refreshes change the drawing without a new revision.
    fn xtb_bond_hover_stamp(&self) -> (u64, u64, u64) {
        (
            self.tab.file_epoch,
            self.tab.revision,
            self.tab.label_generation,
        )
    }

    /// A removed row cannot send an exit; its preview ends with its context.
    /// Only a label refresh of the same drawing redraws a hovered row: an
    /// edit or replacement can move the row away without an exit.
    pub(in crate::app) fn xtb_invalidate_bond_hover(&mut self) {
        let Some(hover) = self.tab.xtb.palette.hovered_bond.take() else {
            return;
        };
        let (epoch, revision, _) = self.xtb_bond_hover_stamp();
        self.tab.xtb.palette.hovered_bond = if self.xtb_bond_hover_preview_of(&hover).is_some() {
            Some(hover)
        } else if hover.stamp.0 == epoch && hover.stamp.1 == revision {
            self.xtb_make_bond_hover(hover.bond)
        } else {
            None
        };
    }

    pub(in crate::app) fn xtb_clear_bond_hover(&mut self) {
        self.tab.xtb.palette.hovered_bond = None;
    }

    pub(in crate::app::xtb) fn xtb_bond_hover_preview(&self) -> Option<&Document> {
        self.xtb_bond_hover_preview_of(self.tab.xtb.palette.hovered_bond.as_ref()?)
    }

    fn xtb_bond_hover_preview_of<'a>(&self, hover: &'a Preview) -> Option<&'a Document> {
        (hover.stamp == self.xtb_bond_hover_stamp() && self.xtb_bond_row_visible(&hover.bond))
            .then_some(hover.document.as_ref())
    }
}

#[cfg(test)]
mod tests;
