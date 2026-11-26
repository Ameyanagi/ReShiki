//! Properties for an infrared figure placed as a group of drawing objects.
use super::{App, Message, document_tab::TabId, xtb::vibrations::figure::spectrum};
use iced::Task;
use reshiki::{
    document::Point,
    graphics::{BandShape, Columns, Frame, GraphicKind, IrFigure, Journal, YAxis},
    grouping::Group,
    scene::selection_bounds,
};
use std::{cmp::Reverse, collections::HashSet};

#[cfg(test)]
mod tests;
mod view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    tab: TabId,
    epoch: u64,
    revision: u64,
    graphic: u64,
    group: u64,
}

#[derive(Debug, Clone, Copy)]
pub enum Field {
    From,
    To,
    Fwhm,
    Scale,
}

impl Field {
    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Edit {
    Y(YAxis),
    MinorTicks(bool),
    Frame(Frame),
    Shape(BandShape),
    Labels(u8),
    VerticalLabels(bool),
    Peak(u32, bool),
    MethodLine(bool),
    Journal(Journal),
    Columns(Columns),
}

#[derive(Debug, Clone)]
pub enum Action {
    Draft(Target, Field, String),
    Apply(Target),
    Edit(Target, Edit),
    CopyCaption(Target),
    CaptionCopied(u64, Result<(), String>),
}

#[derive(Default)]
pub(super) struct State {
    target: Option<Target>,
    parameters: Option<IrFigure>,
    member_count: usize,
    fields: [String; 4],
    errors: [Option<&'static str>; 4],
    peaks: Vec<(u32, u32)>,
}

impl State {
    fn new(
        target: Target,
        figure: &IrFigure,
        member_count: usize,
        mut peaks: Vec<(u32, u32)>,
    ) -> Self {
        peaks.extend(
            figure
                .hidden_labels
                .iter()
                .map(|&key| (key, (f64::from(key) * figure.scale).round() as u32)),
        );
        peaks.sort_by_key(|&(key, _)| Reverse(key));
        peaks.dedup_by_key(|(key, _)| *key);
        Self {
            target: Some(target),
            parameters: Some(figure.clone()),
            member_count,
            fields: Self::displayed(figure),
            errors: [None; 4],
            peaks,
        }
    }

    fn displayed(figure: &IrFigure) -> [String; 4] {
        [
            figure.from.to_string(),
            figure.to.to_string(),
            figure.fwhm.to_string(),
            format!("{:.3}", figure.scale),
        ]
    }

    fn apply(&mut self, figure: &mut IrFigure) -> bool {
        let displayed = Self::displayed(figure);
        let original = [figure.from, figure.to, figure.fwhm, figure.scale];
        let mut input = [None; 4];
        for (((parsed, draft), displayed), original) in input
            .iter_mut()
            .zip(&self.fields)
            .zip(displayed)
            .zip(original)
        {
            let text = draft.trim();
            *parsed = if text == displayed {
                Some(original)
            } else {
                text.parse().ok()
            };
        }
        let [from, to, fwhm, scale] = input;
        let range_value = |n: &f64| (0. ..=10_000.).contains(n);
        let [from, to, fwhm, scale] = [
            from.filter(range_value),
            to.filter(range_value),
            fwhm.filter(|n| IrFigure::FWHM_RANGE.contains(n)),
            scale.filter(|n| IrFigure::SCALE_RANGE.contains(n)),
        ];
        self.errors = [
            from.is_none().then_some("Enter a number from 0 to 10000"),
            to.is_none().then_some("Enter a number from 0 to 10000"),
            fwhm.is_none().then_some("Enter a number from 1 to 200"),
            scale.is_none().then_some("Enter a number from 0.5 to 1.5"),
        ];
        if let (Some(from), Some(to)) = (from, to)
            && !IrFigure::valid_range(from, to)
        {
            if from != figure.from {
                self.errors[0] = Some("From must be at least 200 above To");
            }
            if to != figure.to {
                self.errors[1] = Some("To must be at least 200 below From");
            }
        }
        if let [Some(from), Some(to), Some(fwhm), Some(scale)] = [from, to, fwhm, scale]
            && self.errors.iter().all(Option::is_none)
        {
            (figure.from, figure.to, figure.fwhm, figure.scale) = (from, to, fwhm, scale);
            true
        } else {
            false
        }
    }
}

fn replace_members(list: &mut Vec<u64>, old: &[u64], new: &[u64]) {
    if let Some(first) = list.iter().position(|id| old.contains(id)) {
        list.retain(|id| !old.contains(id));
        list.splice(first..first, new.iter().copied());
    }
}

impl App {
    fn selected_ir_figure(&self) -> Option<(&IrFigure, &Group, u64)> {
        let doc = &self.tab.doc;
        doc.graphics.iter().find_map(|graphic| {
            let figure = graphic.ir.as_deref()?;
            if !self.tab.selected.contains(&graphic.id) {
                return None;
            }
            let group = doc
                .groups
                .iter()
                .filter(|group| group.members.contains(&graphic.id))
                .min_by_key(|group| group.members.len())?;
            let members: HashSet<_> = group.members.iter().copied().collect();
            if doc.groups.iter().any(|inner| {
                inner.id != group.id && inner.members.iter().all(|id| members.contains(id))
            }) {
                return None;
            }
            let annotations = doc.annotations.iter().filter(|a| members.contains(&a.id));
            let graphics = doc.graphics.iter().filter(|g| {
                members.contains(&g.id)
                    && matches!(g.kind, GraphicKind::Line | GraphicKind::Path)
                    && (g.id == graphic.id || g.ir.is_none())
                    && g.panel.is_none()
                    && g.table.is_none()
                    && g.tlc.is_none()
                    && g.piece.is_none()
                    && g.leader.is_none()
                    && g.heads.is_none()
                    && g.text.is_none()
            });
            if graphic.kind != GraphicKind::Path
                || annotations.count() + graphics.count() != group.members.len()
            {
                return None;
            }
            Some((figure, group, graphic.id))
        })
    }

    fn ir_figure_target(&self, graphic: u64, group: u64) -> Target {
        Target {
            tab: self.tab.id,
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            graphic,
            group,
        }
    }

    pub(super) fn current_ir_figure(&self, target: Target) -> Option<(&IrFigure, &Group)> {
        let state = &self.tab.ir_figure;
        if state.target != Some(target) {
            return None;
        }
        let (figure, group, graphic) = self.selected_ir_figure()?;
        (target == self.ir_figure_target(graphic, group.id)
            && state.parameters.as_ref() == Some(figure)
            && state.member_count == group.members.len())
        .then_some((figure, group))
    }

    pub(super) fn sync_ir_figure(&mut self) {
        let Some((figure, group, graphic)) = self.selected_ir_figure() else {
            self.tab.ir_figure = State::default();
            return;
        };
        let target = self.ir_figure_target(graphic, group.id);
        if self.tab.ir_figure.parameters.as_ref() == Some(figure)
            && self.tab.ir_figure.target.is_some_and(|old| {
                Target {
                    revision: target.revision,
                    ..old
                } == target
            })
        {
            self.tab.ir_figure.target = Some(target);
            return;
        }
        let rebuilt = spectrum(&self.tab.doc, figure, Point::default());
        self.tab.ir_figure = State::new(
            target,
            figure,
            rebuilt.group.members.len(),
            rebuilt.labelled_peaks,
        );
    }

    pub(super) fn ir_figure_action(&mut self, action: Action) -> Task<Message> {
        let target = match action {
            Action::Draft(target, ..)
            | Action::Apply(target)
            | Action::Edit(target, _)
            | Action::CopyCaption(target) => target,
            Action::CaptionCopied(epoch, result) => {
                self.copy_as_busy = false;
                if self.tab.file_epoch == epoch {
                    self.error = result.is_err();
                    self.status = result.map_or_else(
                        |error| format!("Could not copy the IR caption\n{error}"),
                        |()| "Copied the IR caption".into(),
                    );
                }
                return Task::none();
            }
        };
        let Some((stored, group)) = self.current_ir_figure(target) else {
            return Task::none();
        };
        let mut figure = match action {
            Action::Draft(_, field, value) => {
                if let Some((input, error)) = self
                    .tab
                    .ir_figure
                    .fields
                    .get_mut(field.index())
                    .zip(self.tab.ir_figure.errors.get_mut(field.index()))
                {
                    *input = value;
                    *error = None;
                }
                return Task::none();
            }
            Action::CopyCaption(_) => {
                #[cfg(not(target_arch = "wasm32"))]
                let write = reshiki::clipboard::write_text;
                #[cfg(target_arch = "wasm32")]
                let write = crate::web::write_text;
                return self.copy_ir_caption_with(write);
            }
            _ => stored.clone(),
        };
        let (group_id, members) = (group.id, group.members.clone());
        match action {
            Action::Apply(_) => {
                if !self.tab.ir_figure.apply(&mut figure) {
                    return Task::none();
                }
                self.tab.ir_figure.fields = State::displayed(&figure);
            }
            Action::Edit(_, edit) => match edit {
                Edit::Y(y) => figure.y = y,
                Edit::MinorTicks(on) => figure.minor_ticks = on,
                Edit::Frame(frame) => figure.frame = frame,
                Edit::Shape(shape) => figure.shape = shape,
                Edit::Labels(count) => figure.labels = count.min(IrFigure::MAX_LABELS),
                Edit::VerticalLabels(on) => figure.vertical_labels = on,
                Edit::Peak(key, visible) => {
                    if self
                        .tab
                        .ir_figure
                        .peaks
                        .iter()
                        .any(|&(peak, _)| peak == key)
                    {
                        if visible {
                            figure.hidden_labels.retain(|&hidden| hidden != key);
                        } else if !figure.hidden_labels.contains(&key) {
                            if figure.hidden_labels.len() >= IrFigure::MAX_HIDDEN_LABELS {
                                self.status = "An IR figure can have up to 64 hidden labels".into();
                                self.error = true;
                                return Task::none();
                            }
                            figure.hidden_labels.push(key);
                        }
                    }
                }
                Edit::MethodLine(on) => figure.method_line = on,
                Edit::Journal(journal) => figure.journal = journal,
                Edit::Columns(columns) => figure.columns = columns,
            },
            _ => return Task::none(),
        }
        if self
            .selected_ir_figure()
            .is_some_and(|(old, ..)| *old != figure)
        {
            self.rebuild_ir_figure(group_id, &members, &figure);
        }
        Task::none()
    }

    fn rebuild_ir_figure(&mut self, group: u64, members: &[u64], figure: &IrFigure) {
        let doc = &mut self.tab.doc;
        let Some((at, _)) = selection_bounds(doc, members) else {
            return;
        };
        // Allocate IDs while the old objects are still in the drawing.
        let rebuilt = spectrum(doc, figure, at);
        let ids = rebuilt.group.members;
        let Some(curve) = rebuilt.graphics.first().map(|graphic| graphic.id) else {
            return;
        };
        let before = doc.clone();
        // Groups store object IDs directly, including all members of nested groups.
        for entry in &mut doc.groups {
            if members.iter().all(|id| entry.members.contains(id)) {
                replace_members(&mut entry.members, members, &ids);
                if let Some(layout) = &mut entry.layout {
                    replace_members(&mut layout.order, members, &ids);
                }
            }
        }
        doc.graphics.extend(rebuilt.graphics);
        doc.annotations.extend(rebuilt.annotations);
        doc.delete(members);
        if let Some((lo, _)) = selection_bounds(doc, &ids) {
            doc.translate(&ids, at.x - lo.x, at.y - lo.y);
        }
        let mut state = State::new(
            self.ir_figure_target(curve, group),
            figure,
            ids.len(),
            rebuilt.labelled_peaks,
        );
        std::mem::swap(&mut state.fields, &mut self.tab.ir_figure.fields);
        std::mem::swap(&mut state.errors, &mut self.tab.ir_figure.errors);
        self.tab.ir_figure = state;
        self.set_selection(ids);
        self.tab.erase_stroke = false;
        self.changed_continuing_anchored(before, false, Some(curve));
    }

    fn copy_ir_caption_with<Write, Pending>(&mut self, write: Write) -> Task<Message>
    where
        Write: FnOnce(String) -> Pending,
        Pending: std::future::Future<Output = Result<(), String>>
            + iced_runtime::futures::MaybeSend
            + 'static,
    {
        let Some((figure, _)) = self
            .tab
            .ir_figure
            .target
            .and_then(|target| self.current_ir_figure(target))
        else {
            return Task::none();
        };
        let caption = figure.caption();
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        let epoch = self.tab.file_epoch;
        self.copy_as_busy = true;
        self.error = false;
        self.status = "Copying the IR caption…".into();
        Task::perform(write(caption), move |result| {
            Message::IrFigure(Action::CaptionCopied(epoch, result))
        })
    }
}
