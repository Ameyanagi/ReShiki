//! Exact numeric inputs for tube lengths. A draft is scoped to the drawing
//! revision and piece that supplied the field.
use super::{Action, App, Message};
use crate::app::{document_tab::TabId, workspace::command};
use iced::widget::{row, text};
use iced::{Alignment, Element, Length};
use reshiki::equipment::{eval::Value, format::Param};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    tab: TabId,
    epoch: u64,
    revision: u64,
    id: u64,
    param: String,
}

pub(in crate::app) struct Draft {
    target: Target,
    value: String,
}

pub(super) fn display_mm(value: f64) -> String {
    format!("{value:.4}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

impl App {
    pub(super) fn cut_length_input<'a>(
        &self,
        id: u64,
        param: &str,
        label: &str,
        current: f64,
    ) -> Element<'a, Message> {
        let target = self.cut_length_target(id, param);
        let value = self
            .tab
            .piece_lengths
            .iter()
            .find(|draft| draft.target == target)
            .map_or_else(|| display_mm(current), |draft| draft.value.clone());
        let edit = target.clone();
        let apply = Message::Equipment(Action::ApplyCutLength(target));
        let input = reshiki::accessibility::text_input(
            format!("equipment-param-{id}-{param}"),
            format!("{label} (mm)"),
            "",
            &value,
        )
        .size(12)
        .padding([4, 6])
        .width(Length::Fixed(78.))
        .on_input(move |value| Message::Equipment(Action::CutLengthInput(edit.clone(), value)))
        .on_submit(apply.clone());
        row![
            text(label.to_owned()).size(12).width(Length::Fill),
            input,
            text("mm").size(11),
            command("Set", apply),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    }

    fn cut_length_target(&self, id: u64, param: &str) -> Target {
        Target {
            tab: self.tab.id,
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            id,
            param: param.to_owned(),
        }
    }

    fn length_limits(&self, target: &Target) -> Option<(f64, f64)> {
        if self.cut_length_target(target.id, &target.param) != *target
            || !self.tab.selected.contains(&target.id)
        {
            return None;
        }
        let g = self.tab.doc.graphics.iter().find(|g| g.id == target.id)?;
        let d = g.piece.as_ref()?.linked.as_ref()?;
        if !d.tube_spec.as_ref()?.length.params.contains(&target.param) {
            return None;
        }
        d.params.iter().find_map(|param| match param {
            Param::Range { id, min, max, .. } if *id == target.param => Some((*min, *max)),
            _ => None,
        })
    }

    pub(super) fn edit_cut_length(&mut self, target: Target, value: String) {
        if self.length_limits(&target).is_none() {
            return;
        }
        let drafts = &mut self.tab.piece_lengths;
        drafts.retain(|draft| {
            draft.target.revision == target.revision && draft.target.epoch == target.epoch
        });
        if let Some(draft) = drafts.iter_mut().find(|draft| draft.target == target) {
            draft.value = value;
        } else {
            drafts.push(Draft { target, value });
        }
    }

    pub(super) fn apply_cut_length(&mut self, target: Target) -> bool {
        let Some((min, max)) = self.length_limits(&target) else {
            return false;
        };
        let Some(draft) = self
            .tab
            .piece_lengths
            .iter()
            .find(|draft| draft.target == target)
        else {
            return false;
        };
        let length = draft
            .value
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| (min..=max).contains(value));
        let Some(length) = length else {
            self.status = format!("Enter a length from {min} to {max} mm");
            self.error = true;
            return false;
        };
        self.tab
            .piece_lengths
            .retain(|draft| draft.target != target);
        let before = self.tab.doc.clone();
        self.set_piece_param(target.id, &target.param, Value::Number(length), before);
        if !self.error {
            for draft in &mut self.tab.piece_lengths {
                if draft.target.revision == target.revision && draft.target.epoch == target.epoch {
                    draft.target.revision = self.tab.revision;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::{
        document::Point,
        equipment::{builtin, placed},
    };

    #[test]
    fn cut_length_commits_an_exact_value_and_rejects_invalid_or_stale_drafts() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let tube = builtin::find("reshiki/compression-tube-60-1-4").unwrap();
        let id = app.tab.doc.next_id();
        let scale = placed::world_per_mm(&app.tab.doc, placed::DEFAULT_PT_PER_MM);
        app.tab
            .doc
            .graphics
            .push(placed::graphic(id, tube, Point::default(), scale));
        app.tab.selected = vec![id];
        let before = app.tab.doc.clone();

        let target = app.cut_length_target(id, "length");
        let _ = app.update(Message::Equipment(Action::CutLengthInput(
            target.clone(),
            "0".into(),
        )));
        let _ = app.update(Message::Equipment(Action::ApplyCutLength(target.clone())));
        assert!(app.error);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Equipment(Action::CutLengthInput(
            target.clone(),
            "82.125".into(),
        )));
        let _ = app.update(Message::Equipment(Action::ApplyCutLength(target.clone())));
        assert!(!app.error, "{}", app.status);
        let spec = reshiki::equipment::parts_list::tube_row(&app.tab.doc, id)
            .unwrap()
            .unwrap();
        assert_eq!(spec.cut_length_mm, 82.125);
        let edited = app.tab.doc.clone();
        let _ = app.update(Message::Equipment(Action::CutLengthInput(
            target.clone(),
            "85".into(),
        )));
        let _ = app.update(Message::Equipment(Action::ApplyCutLength(target)));
        assert_eq!(app.tab.doc, edited);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
    }

    #[test]
    fn setting_one_bend_leg_keeps_the_draft_for_the_other_leg() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let tube = builtin::find("reshiki/compression-tube-bend-90-1-4").unwrap();
        let id = app.tab.doc.next_id();
        let scale = placed::world_per_mm(&app.tab.doc, placed::DEFAULT_PT_PER_MM);
        app.tab
            .doc
            .graphics
            .push(placed::graphic(id, tube, Point::default(), scale));
        app.tab.selected = vec![id];
        let inlet = app.cut_length_target(id, "inlet_length");
        let outlet = app.cut_length_target(id, "outlet_length");
        app.edit_cut_length(inlet.clone(), "71.5".into());
        app.edit_cut_length(outlet, "83.25".into());
        assert!(app.apply_cut_length(inlet));
        let outlet = app.cut_length_target(id, "outlet_length");
        assert!(app.apply_cut_length(outlet));
        let report = reshiki::equipment::parts_list::tube_row(&app.tab.doc, id)
            .unwrap()
            .unwrap();
        assert!((report.cut_length_mm - 154.75 - 10. * std::f64::consts::PI).abs() < 1e-6);
    }
}
