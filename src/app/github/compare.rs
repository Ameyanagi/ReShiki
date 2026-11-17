//! Two versions of a drawing side by side, with what changed marked by kind:
//! the conflict dialog shows GitHub's and yours, Versions a version and the
//! drawing now. [`reshiki::compare`] tells chemistry from layout; it runs off
//! the UI thread, tagged with a generation so a late result never lands in a
//! newer comparison. Chemistry, Layout or Everything filters the marks and
//! the list; lined up, each moved object of the right side is shown where the
//! left side has it. The marks are a layer of their own over copies of
//! nothing: both drawings, and their own highlights, are drawn as they are.
use crate::app::{App, Message, workspace::muted_text};
use crate::canvas::{CompareMark, ComparePaint, ComparePlace, ComparePreview, CompareSwatch};
use iced::widget::{Space, checkbox, column, container, row, scrollable, text};
use iced::{Alignment, Border, Color, Element, Length, Task, Theme};
use reshiki::compare::{Category, Change, Chemistry, Detail, Kind, Report, Subject};
use reshiki::document::{Document, Point as World};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Every comparison started, so a result knows which one it belongs to.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Which changes show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    Chemistry,
    Layout,
    #[default]
    Everything,
}

impl Mode {
    const ALL: [(Self, &'static str); 3] = [
        (Self::Chemistry, "Chemistry"),
        (Self::Layout, "Layout"),
        (Self::Everything, "Everything"),
    ];

    fn shows(self, category: Category) -> bool {
        match self {
            Self::Chemistry => category == Category::Chemistry,
            Self::Layout => category == Category::Layout,
            Self::Everything => true,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    /// A comparison of that generation finished.
    Done(u64, Option<Arc<Shown>>),
    Mode(Mode),
    LineUp(bool),
    /// A change in the list, pointed at in both drawings; again lets go.
    Select(usize),
}

/// The text tag of a kind, readable without its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tag {
    Chem,
    Moved,
    Style,
    Text,
    Reaction,
    Added,
    Removed,
    Same,
    Unverified,
}

impl Tag {
    fn label(self) -> &'static str {
        match self {
            Self::Chem => "CHEM",
            Self::Moved => "MOVED",
            Self::Style => "STYLE",
            Self::Text => "TEXT",
            Self::Reaction => "RXN",
            Self::Added => "ADDED",
            Self::Removed => "REMOVED",
            Self::Same => "SAME",
            Self::Unverified => "?",
        }
    }

    /// Background and text, light then dark.
    fn colors(self, dark: bool) -> (Color, Color) {
        let ([bg, fg], [night_bg, night_fg]) = match self {
            Self::Chem => (
                [[251, 225, 207], [162, 74, 20]],
                [[74, 44, 26], [245, 170, 120]],
            ),
            Self::Moved => (
                [[219, 231, 251], [40, 86, 168]],
                [[30, 45, 74], [150, 182, 240]],
            ),
            Self::Style => (
                [[236, 226, 251], [106, 63, 176]],
                [[52, 38, 74], [192, 162, 236]],
            ),
            Self::Text => (
                [[247, 236, 200], [122, 90, 7]],
                [[66, 56, 24], [232, 202, 110]],
            ),
            Self::Reaction => (
                [[248, 220, 239], [155, 45, 120]],
                [[70, 30, 58], [232, 142, 202]],
            ),
            Self::Added | Self::Removed | Self::Same => (
                [[223, 241, 228], [31, 107, 59]],
                [[28, 56, 38], [132, 210, 160]],
            ),
            Self::Unverified => (
                [[236, 238, 236], [85, 85, 85]],
                [[48, 52, 50], [190, 196, 192]],
            ),
        };
        let rgb = |[r, g, b]: [u8; 3]| Color::from_rgb8(r, g, b);
        if dark {
            (rgb(night_bg), rgb(night_fg))
        } else {
            (rgb(bg), rgb(fg))
        }
    }
}

/// One line of "What changed".
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Row {
    /// The report's change, or `None` for the summary of what is the same.
    pub(super) change: Option<usize>,
    pub(super) tag: Tag,
    pub(super) title: String,
    pub(super) detail: String,
    /// SMILES before → after.
    pub(super) smiles: Option<String>,
    /// `None`: shown in every mode.
    pub(super) category: Option<Category>,
}

/// A finished comparison, ready to draw.
#[derive(Debug)]
pub struct Shown {
    pub(super) report: Report,
    /// The right side lined up with the left.
    pub(super) aligned: Document,
    pub(super) rows: Vec<Row>,
    pub(super) marks: Marks,
    /// Each change's objects: on the left, the right where it is, and lined up.
    pub(super) spots: Vec<[Option<(World, World)>; 3]>,
    /// Both sides where they are, and lined up.
    frame: (World, World),
    lined_frame: (World, World),
}

/// The marks on the left side, the right side where it is, and lined up.
#[derive(Debug)]
pub(super) struct Marks {
    pub(super) left: Vec<CompareMark>,
    pub(super) right: Vec<CompareMark>,
    pub(super) lined: Vec<CompareMark>,
}

/// Two drawings ready to show side by side.
#[derive(Debug)]
pub(in crate::app) struct Compare {
    /// `Err`: why there is nothing to show, such as `Deleted on GitHub`.
    pub(super) left: Result<Document, String>,
    pub(super) right: Document,
    pub(super) generation: u64,
    /// `None` while comparing.
    pub(super) shown: Option<Arc<Shown>>,
    /// The comparison stopped without a result.
    pub(super) failed: bool,
    pub(super) mode: Mode,
    pub(super) line_up: bool,
    /// The change pointed at.
    pub(super) selected: Option<usize>,
}

fn extent(doc: &Document) -> (World, World) {
    reshiki::scene::selection_bounds(doc, &doc.all_ids()).unwrap_or_else(|| doc.bounds())
}

fn union(a: (World, World), b: (World, World)) -> (World, World) {
    (
        World::new(a.0.x.min(b.0.x), a.0.y.min(b.0.y)),
        World::new(a.1.x.max(b.1.x), a.1.y.max(b.1.y)),
    )
}

/// `1.3 cm left, 0.5 cm down`, and how far it turned.
fn movement(doc: &Document, detail: &Detail) -> String {
    let Detail::Move { dx, dy, degrees } = *detail else {
        return String::new();
    };
    let per_cm = f64::from(doc.drawing_style.points_per_world()) * 2.54 / 72.;
    let mut parts = Vec::new();
    for (cm, (back, on)) in [
        (dx * per_cm, ("left", "right")),
        (dy * per_cm, ("up", "down")),
    ] {
        if cm.abs() >= 0.05 {
            let way = if cm < 0. { back } else { on };
            parts.push(format!("{:.1} cm {way}", cm.abs()));
        }
    }
    if degrees.abs() >= 0.5 {
        let way = if degrees < 0. {
            "counterclockwise"
        } else {
            "clockwise"
        };
        parts.push(format!("turned {:.0}° {way}", degrees.abs()));
    }
    parts.join(", ")
}

fn subject(subject: Subject) -> &'static str {
    match subject {
        Subject::Molecule => "Molecule",
        Subject::Arrow => "Arrow",
        Subject::Text => "Text",
        Subject::Graphic => "Shape",
        Subject::Scheme => "Scheme",
        Subject::Settings => "Drawing",
    }
}

fn plural(n: usize, one: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {one}s")
    }
}

/// The line of one change.
fn change_row(index: usize, change: &Change, left: &Document, right: &Document) -> Row {
    let name = subject(change.subject);
    let smiles = match (&change.before, &change.after) {
        (Some(a), Some(b)) => format!("{a} → {b}"),
        (Some(one), None) | (None, Some(one)) => one.clone(),
        (None, None) => String::new(),
    };
    let quoted = || {
        format!(
            "“{}” → “{}”",
            change.before.as_deref().unwrap_or_default(),
            change.after.as_deref().unwrap_or_default()
        )
    };
    let molecule = change.subject == Subject::Molecule;
    let (tag, title, detail, code) = match change.kind {
        Kind::Chemical => {
            let title = match &change.detail {
                Detail::Edit { stereo: true, .. } => "Stereo changed".to_owned(),
                Detail::Edit {
                    added,
                    removed,
                    changed,
                    ..
                } => {
                    let mut parts = Vec::new();
                    if !added.is_empty() {
                        parts.push(format!("{added} added"));
                    }
                    if !removed.is_empty() {
                        parts.push(format!("{removed} removed"));
                    }
                    if parts.is_empty() && *changed > 0 {
                        parts.push(format!("{} changed", plural(*changed, "atom")));
                    }
                    if parts.is_empty() {
                        "Bonds changed".to_owned()
                    } else {
                        parts.join(", ")
                    }
                }
                Detail::Reason(reason) => reason.clone(),
                _ => "Molecule changed".to_owned(),
            };
            (
                Tag::Chem,
                title,
                "Different chemistry".to_owned(),
                Some(smiles),
            )
        }
        Kind::Added if molecule => (
            Tag::Added,
            "Molecule added".into(),
            String::new(),
            Some(smiles),
        ),
        Kind::Removed if molecule => (
            Tag::Removed,
            "Molecule removed".into(),
            String::new(),
            Some(smiles),
        ),
        Kind::Added => (Tag::Added, format!("{name} added"), smiles, None),
        Kind::Removed => (Tag::Removed, format!("{name} removed"), smiles, None),
        Kind::Reaction if molecule => (
            Tag::Reaction,
            format!(
                "Reaction role: {} → {}",
                change.before.as_deref().unwrap_or_default(),
                change.after.as_deref().unwrap_or_default()
            ),
            String::new(),
            None,
        ),
        Kind::Reaction => (Tag::Reaction, "Arrow kind changed".into(), quoted(), None),
        Kind::Unverified => {
            let reason = match &change.detail {
                Detail::Reason(reason) => reason.clone(),
                _ => String::new(),
            };
            (
                Tag::Unverified,
                "Couldn't verify chemistry".into(),
                reason,
                Some(smiles),
            )
        }
        Kind::SchemeMoved => {
            let mut detail = movement(left, &change.detail);
            let molecules = change
                .left
                .objects
                .iter()
                .filter(|id| left.atom(**id).is_some())
                .count();
            if molecules > 0 {
                detail.push_str(" · everything together");
            }
            (Tag::Moved, "Whole scheme moved".into(), detail, None)
        }
        Kind::Moved => (
            Tag::Moved,
            format!("{name} moved"),
            movement(left, &change.detail),
            None,
        ),
        Kind::Rotated => (
            Tag::Moved,
            format!("{name} turned"),
            movement(left, &change.detail),
            None,
        ),
        Kind::Redrawn if molecule => (
            Tag::Moved,
            "Molecule redrawn".into(),
            "Its shape changed; the same molecule".into(),
            None,
        ),
        Kind::Redrawn => (Tag::Moved, format!("{name} reshaped"), String::new(), None),
        Kind::Style if change.subject == Subject::Settings => (
            Tag::Style,
            "Style or settings changed".into(),
            "Style only".into(),
            None,
        ),
        Kind::Style => (
            Tag::Style,
            format!("{name} style changed"),
            "Style only".into(),
            None,
        ),
        Kind::Text => {
            let condition = change.right.objects.iter().any(|id| {
                right
                    .arrow_labels
                    .iter()
                    .any(|label| label.annotation == *id)
            });
            let what = if condition { "Conditions" } else { "Text" };
            match (&change.before, &change.after) {
                (Some(_), Some(_)) => (
                    Tag::Text,
                    format!("{what}: {}", quoted()),
                    String::new(),
                    None,
                ),
                _ => (
                    Tag::Text,
                    format!("{what} attached differently"),
                    String::new(),
                    None,
                ),
            }
        }
    };
    Row {
        change: Some(index),
        tag,
        title,
        detail,
        smiles: code.filter(|code| !code.is_empty()),
        category: Some(change.category()),
    }
}

/// In list order: chemistry, text, layout, style.
fn rank(kind: Kind) -> u8 {
    match kind {
        Kind::Chemical | Kind::Removed | Kind::Added | Kind::Reaction | Kind::Unverified => 0,
        Kind::Text => 1,
        Kind::SchemeMoved | Kind::Moved | Kind::Rotated | Kind::Redrawn => 2,
        Kind::Style => 3,
    }
}

fn rows(report: &Report, left: &Document, right: &Document) -> Vec<Row> {
    let mut rows: Vec<Row> = report
        .changes
        .iter()
        .enumerate()
        .map(|(i, change)| change_row(i, change, left, right))
        .collect();
    rows.sort_by_key(|row| {
        row.change
            .and_then(|i| report.changes.get(i))
            .map(|change| rank(change.kind))
    });
    let same: Vec<&str> = report
        .molecules
        .iter()
        .filter(|pair| pair.chemistry == Chemistry::Same)
        .filter_map(|pair| pair.smiles[0].as_deref())
        .collect();
    let all = report.molecules.len();
    if all > 0 && !same.is_empty() {
        let chemistry = report
            .changes
            .iter()
            .any(|change| change.category() == Category::Chemistry);
        let title = if chemistry {
            format!("{} unchanged", plural(same.len(), "molecule"))
        } else {
            format!("{} of {} molecules match by SMILES", same.len(), all)
        };
        let mut detail = same.iter().take(3).copied().collect::<Vec<_>>().join(" · ");
        if same.len() > 3 {
            detail.push_str(" · …");
        }
        rows.push(Row {
            change: None,
            tag: Tag::Same,
            title,
            detail,
            smiles: None,
            category: None,
        });
    }
    rows
}

fn paint(change: &Change) -> ComparePaint {
    match change.kind {
        Kind::Chemical => ComparePaint::Chemical,
        Kind::Reaction => ComparePaint::Reaction,
        Kind::Added | Kind::Removed => ComparePaint::Added,
        Kind::Unverified => ComparePaint::Unverified,
        Kind::SchemeMoved | Kind::Moved | Kind::Rotated | Kind::Redrawn => ComparePaint::Moved,
        Kind::Style => ComparePaint::Style,
        Kind::Text => ComparePaint::Text,
    }
}

/// The marks of every change on one side, and where each change's objects
/// are there.
fn marks(
    doc: &Document,
    report: &Report,
    side: impl Fn(&Change) -> &reshiki::compare::Marks,
) -> (Vec<CompareMark>, Vec<Option<(World, World)>>) {
    // A collapsed label's hidden atoms are marked on the label.
    let shown_at: HashMap<u64, u64> = doc
        .abbreviations
        .iter()
        .flat_map(|group| group.members.iter().map(|id| (*id, group.anchor)))
        .collect();
    let at = |id: &u64| {
        let id = shown_at.get(id).unwrap_or(id);
        doc.atom(*id).map(|atom| atom.position)
    };
    let (mut found, mut spots) = (Vec::new(), Vec::new());
    for change in &report.changes {
        let marks = side(change);
        let spot = reshiki::scene::selection_bounds(doc, &marks.objects);
        spots.push(spot);
        let (paint, chemistry) = (paint(change), change.category() == Category::Chemistry);
        let mut mark = |place| {
            found.push(CompareMark {
                paint,
                chemistry,
                place,
            });
        };
        if paint == ComparePaint::Chemical {
            for position in marks.atoms.iter().filter_map(at) {
                mark(ComparePlace::Atom(position));
            }
            for (a, b) in &marks.bonds {
                // Inside a collapsed label, the label's mark shows it.
                match (at(a), at(b)) {
                    (Some(a), Some(b)) if a != b => mark(ComparePlace::Bond(a, b)),
                    (Some(a), Some(_)) => mark(ComparePlace::Atom(a)),
                    _ => {}
                }
            }
        } else if let Some((lo, hi)) = spot {
            mark(ComparePlace::Box(lo, hi));
        }
    }
    (found, spots)
}

/// Compares, and lays out what to draw. Slow for a big drawing.
fn build(left: &Document, right: &Document) -> Shown {
    let report = reshiki::compare::compare(left, right);
    let aligned = reshiki::compare::aligned(right, &report);
    let rows = rows(&report, left, right);
    let (on_left, at_left) = marks(left, &report, |change| &change.left);
    let (on_right, at_right) = marks(right, &report, |change| &change.right);
    let (lined, at_lined) = marks(&aligned, &report, |change| &change.right);
    let spots = at_left
        .into_iter()
        .zip(at_right)
        .zip(at_lined)
        .map(|((a, b), c)| [a, b, c])
        .collect();
    Shown {
        frame: union(extent(left), extent(right)),
        lined_frame: union(extent(left), extent(&aligned)),
        report,
        aligned,
        rows,
        marks: Marks {
            left: on_left,
            right: on_right,
            lined,
        },
        spots,
    }
}

/// The header's verdict: chemistry first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    Comparing,
    Failed,
    Same,
    Changed(usize),
    Unverified,
}

impl Compare {
    /// Shows both at once; [`Self::start`] compares them.
    pub(super) fn new(left: Result<Document, String>, right: Document) -> Self {
        Self {
            left,
            right,
            generation: GENERATION.fetch_add(1, Ordering::Relaxed) + 1,
            shown: None,
            failed: false,
            mode: Mode::default(),
            line_up: true,
            selected: None,
        }
    }

    /// Compares off the UI thread. Nothing to compare with, nothing to do.
    pub(super) fn start(&self) -> Task<Message> {
        let Ok(left) = &self.left else {
            return Task::none();
        };
        let (left, right, generation) = (left.clone(), self.right.clone(), self.generation);
        Task::perform(
            async move {
                reshiki::task::spawn_blocking(move || Arc::new(build(&left, &right)))
                    .await
                    .ok()
            },
            move |shown| Message::GitHub(super::Action::Compare(Action::Done(generation, shown))),
        )
    }

    /// Changes in each mode's list, the summary of what is the same left out.
    pub(super) fn count(&self, mode: Mode) -> Option<usize> {
        let shown = self.shown.as_ref()?;
        Some(
            shown
                .rows
                .iter()
                .filter(|row| row.category.is_some_and(|c| mode.shows(c)))
                .count(),
        )
    }

    pub(super) fn verdict(&self) -> Option<Verdict> {
        self.left.as_ref().ok()?;
        let Some(shown) = &self.shown else {
            return Some(if self.failed {
                Verdict::Failed
            } else {
                Verdict::Comparing
            });
        };
        let changed = shown.report.count(Category::Chemistry);
        Some(if changed > 0 {
            Verdict::Changed(changed)
        } else if shown.report.unverified() > 0 {
            Verdict::Unverified
        } else {
            Verdict::Same
        })
    }

    /// The rows the mode shows.
    pub(super) fn listed(&self) -> Vec<&Row> {
        self.shown.as_ref().map_or_else(Vec::new, |shown| {
            shown
                .rows
                .iter()
                .filter(|row| row.category.is_none_or(|c| self.mode.shows(c)))
                .collect()
        })
    }

    /// The right side as drawn: lined up, or where it is.
    pub(super) fn right_shown(&self) -> &Document {
        match &self.shown {
            Some(shown) if self.line_up => &shown.aligned,
            _ => &self.right,
        }
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::Done(generation, shown) => {
                if generation == self.generation && self.shown.is_none() {
                    self.failed = shown.is_none();
                    self.shown = shown;
                }
            }
            Action::Mode(mode) => {
                self.mode = mode;
                self.selected = None;
            }
            Action::LineUp(on) => self.line_up = on,
            Action::Select(change) => {
                self.selected = (self.selected != Some(change)).then_some(change);
            }
        }
    }

    /// The mode switch, the line-up switch, then both drawings and the list
    /// of what changed. `under` goes below each drawing.
    pub(super) fn view<'a>(
        &'a self,
        headers: [Element<'a, Message>; 2],
        under: [Option<Element<'a, Message>>; 2],
    ) -> Element<'a, Message> {
        let send = |action| Message::GitHub(super::Action::Compare(action));
        let mut modes = row![].spacing(2);
        for (mode, name) in Mode::ALL {
            let count = self.count(mode);
            let label = row![text(name).size(12)]
                .push(count.map(|n| text(n.to_string()).size(12).style(muted_text)))
                .spacing(5);
            let spoken = count.map_or_else(|| name.to_owned(), |n| format!("{name}, {n}"));
            modes = modes.push(
                reshiki::accessibility::button(
                    format!("github-compare-{}", name.to_lowercase()),
                    spoken,
                    label,
                )
                .checked(self.mode == mode)
                .padding([4, 10])
                .style(crate::app::workspace::segment(self.mode == mode))
                .on_press(send(Action::Mode(mode))),
            );
        }
        let bar = row![
            container(modes)
                .padding(2)
                .style(crate::app::workspace::segment_track),
            checkbox(self.line_up)
                .label("Line up moved molecules")
                .on_toggle(move |on| send(Action::LineUp(on)))
                .text_size(12),
        ]
        .spacing(16)
        .align_y(Alignment::Center);

        let shown = self.shown.as_deref();
        let show = (self.mode != Mode::Layout, self.mode != Mode::Chemistry);
        let lined = self.line_up && shown.is_some();
        let frame = shown.map_or_else(
            || match &self.left {
                Ok(left) => union(extent(left), extent(&self.right)),
                Err(_) => extent(&self.right),
            },
            |shown| {
                if lined {
                    shown.lined_frame
                } else {
                    shown.frame
                }
            },
        );
        let spot = |side: usize| {
            let shown = shown?;
            shown
                .spots
                .get(self.selected?)?
                .get(side)
                .copied()
                .flatten()
        };
        let none: &[CompareMark] = &[];
        let preview = |doc: &'a Document, marks: &'a [CompareMark], selected| {
            crate::canvas::layered::canvas(ComparePreview {
                doc,
                marks,
                show,
                selected,
                frame,
            })
            .width(Length::Fill)
            .height(Length::Fill)
        };
        let left: Element<'a, Message> = match &self.left {
            Ok(doc) => preview(doc, shown.map_or(none, |shown| &shown.marks.left), spot(0)).into(),
            Err(note) => container(text(note).size(13).style(muted_text))
                .center(Length::Fill)
                .into(),
        };
        let right: Element<'a, Message> = preview(
            self.right_shown(),
            shown.map_or(none, |shown| {
                if lined {
                    &shown.marks.lined
                } else {
                    &shown.marks.right
                }
            }),
            spot(if lined { 2 } else { 1 }),
        )
        .into();
        let [left_header, right_header] = headers;
        let [left_under, right_under] = under;
        let pane = |header: Element<'a, Message>, body: Element<'a, Message>, under| {
            column![
                container(header).clip(true),
                crate::app::inspector::card(container(body).height(Length::Fill)),
            ]
            .push(under)
            .spacing(6)
            .width(Length::FillPortion(1))
        };
        column![
            bar,
            row![
                pane(left_header, left, left_under),
                pane(right_header, right, right_under),
                self.list(),
            ]
            .spacing(10)
            .height(Length::Fill),
        ]
        .spacing(10)
        .into()
    }

    /// "What changed", then the legend.
    fn list(&self) -> Element<'_, Message> {
        let mut lines = column![].spacing(2);
        if self.left.is_ok() && self.shown.is_none() {
            let note = if self.failed {
                "Couldn't compare these versions."
            } else {
                "Comparing…"
            };
            lines = lines.push(text(note).size(12).style(muted_text));
        }
        if self.mode == Mode::Chemistry && self.count(Mode::Chemistry) == Some(0) {
            lines = lines.push(line(
                "github-compare-none",
                Tag::Same,
                "No chemical changes",
                "Layout or Everything shows what else changed.",
                None,
                false,
                None,
            ));
        }
        for (n, row) in self.listed().into_iter().enumerate() {
            let selected = row.change.is_some() && row.change == self.selected;
            lines = lines.push(line(
                format!("github-compare-row-{n}"),
                row.tag,
                &row.title,
                &row.detail,
                row.smiles.as_deref(),
                selected,
                row.change
                    .map(|change| Message::GitHub(super::Action::Compare(Action::Select(change)))),
            ));
        }
        let swatch = |paint, name: &'static str| {
            row![
                crate::canvas::layered::canvas(CompareSwatch(paint))
                    .width(18)
                    .height(10),
                text(name).size(11).style(muted_text),
            ]
            .spacing(5)
            .align_y(Alignment::Center)
        };
        let legend = iced::widget::Row::with_children([
            swatch(ComparePaint::Chemical, "Chemical").into(),
            swatch(ComparePaint::Moved, "Moved / redrawn").into(),
            swatch(ComparePaint::Style, "Style").into(),
            swatch(ComparePaint::Text, "Text").into(),
        ])
        .spacing(10)
        .wrap();
        container(crate::app::inspector::card(
            column![
                text("What changed").size(13).font(bold()),
                scrollable(lines).height(Length::Fill),
                legend,
            ]
            .spacing(8)
            .padding(10),
        ))
        .width(290)
        .into()
    }

    /// From Versions: a version beside the drawing now.
    pub(super) fn dialog<'a>(&'a self, label: &'a str) -> Element<'a, Message> {
        let button = |id: &'static str, name: &'static str, message: Message| {
            reshiki::accessibility::button(id, name, text(name).size(13))
                .padding([7, 12])
                .style(crate::appearance::secondary)
                .on_press(message)
        };
        column![
            row![text("Compare").size(18), Space::new().width(Length::Fill)]
                .push(self.verdict_badge())
                .align_y(Alignment::Center),
            self.view(
                [
                    text(label).size(13).wrapping(text::Wrapping::None).into(),
                    text("Now").size(13).into(),
                ],
                [None, None],
            ),
            row![
                text("Click a line to point at it in both drawings.")
                    .size(12)
                    .style(muted_text),
                Space::new().width(Length::Fill),
                button(
                    "github-compare-restore",
                    "Restore this",
                    Message::GitHub(super::Action::Versions(super::versions::Action::Restore)),
                ),
                button(
                    "github-compare-close",
                    "Close",
                    Message::GitHub(super::Action::Close)
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(12)
        .into()
    }

    /// "Chemically identical", "2 chemical changes", "Couldn't verify all
    /// chemistry" or "Comparing…".
    pub(super) fn verdict_badge(&self) -> Option<Element<'_, Message>> {
        let verdict = self.verdict()?;
        let (words, tag) = match verdict {
            Verdict::Comparing => ("Comparing…".to_owned(), Tag::Unverified),
            Verdict::Failed => ("Couldn't compare".to_owned(), Tag::Unverified),
            Verdict::Same => ("Chemically identical".to_owned(), Tag::Same),
            Verdict::Changed(n) => (plural(n, "chemical change"), Tag::Chem),
            Verdict::Unverified => ("Couldn't verify all chemistry".to_owned(), Tag::Unverified),
        };
        Some(
            container(text(words).size(12).font(bold()))
                .padding([3, 10])
                .style(move |theme: &Theme| pill(theme, tag, 12.))
                .into(),
        )
    }
}

fn bold() -> iced::Font {
    iced::Font {
        weight: iced::font::Weight::Semibold,
        ..iced::Font::DEFAULT
    }
}

fn pill(theme: &Theme, tag: Tag, radius: f32) -> container::Style {
    let (background, color) = tag.colors(crate::appearance::is_dark(theme));
    container::Style {
        background: Some(background.into()),
        text_color: Some(color),
        border: Border {
            radius: radius.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

/// One line of the list: its tag, title, detail and SMILES.
fn line<'a>(
    id: impl Into<String>,
    tag: Tag,
    title: &str,
    detail: &str,
    smiles: Option<&str>,
    selected: bool,
    press: Option<Message>,
) -> Element<'a, Message> {
    let badge = container(text(tag.label()).size(10).font(bold()))
        .padding([2, 5])
        .style(move |theme: &Theme| pill(theme, tag, 4.));
    let mut words = column![text(title.to_owned()).size(12).font(bold())].spacing(2);
    if !detail.is_empty() {
        words = words.push(text(detail.to_owned()).size(11).style(muted_text));
    }
    if let Some(smiles) = smiles {
        words = words.push(
            text(smiles.to_owned())
                .size(11)
                .font(iced::Font::MONOSPACE)
                .wrapping(text::Wrapping::Glyph),
        );
    }
    let name = format!("{}: {title}", tag.label());
    let content = row![badge, words.width(Length::Fill)].spacing(8);
    reshiki::accessibility::button(id, name, content)
        .checked(selected)
        .width(Length::Fill)
        .padding([6, 8])
        .style(crate::app::workspace::segment(selected))
        .on_press_maybe(press)
        .into()
}

impl App {
    /// A compare action, for the compare on show: in Versions' dialog or
    /// the conflict dialog.
    pub(super) fn github_compare(&mut self, action: Action) -> Task<Message> {
        let compare = match &mut self.github.dialog {
            Some(super::Dialog::Compare(compare, _)) => compare,
            Some(super::Dialog::Conflict(review)) => &mut review.compare,
            _ => return Task::none(),
        };
        compare.update(action);
        Task::none()
    }
}

#[cfg(test)]
mod tests;
