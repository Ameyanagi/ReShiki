//! Two versions of a drawing side by side, with what changed marked by kind:
//! the conflict dialog shows GitHub's and yours, Versions a version and the
//! drawing now. [`reshiki::compare`] tells chemistry from layout; it runs off
//! the UI thread, tagged with a generation so a late result never lands in a
//! newer comparison. Chemistry, Layout or Everything filters the marks and
//! the list; lined up, each moved object of the right side is shown where the
//! left side has it. The marks are a layer of their own over copies of
//! nothing: both drawings, and their own highlights, are drawn as they are.
use crate::app::{App, Message, workspace::muted_text};
use crate::canvas::{
    CompareFrame, CompareMark, ComparePaint, ComparePlace, ComparePreview, CompareSwatch,
    compare_zoom,
};
use iced::widget::{Space, checkbox, column, container, responsive, row, scrollable, text};
use iced::{Alignment, Border, Color, Element, Length, Size, Task, Theme};
use reshiki::compare::{
    Category, Change, Chemistry, Detail, Kind, Part, PartKind, Report, Subject,
};
use reshiki::document::{Document, Point as World};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Every comparison started, so a result knows which one it belongs to.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// The width of "What changed".
const LIST: f32 = 290.;
/// A pane's header, and the space between panes.
const HEADER: f32 = 30.;
const GAP: f32 = 10.;
/// What sits under a pane, such as the conflict dialog's Keep buttons.
const UNDER: f32 = 40.;
/// The dialog's buttons under everything.
const FOOTER: f32 = 32.;

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
    Mirrored,
    Scaled,
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
            Self::Mirrored => "MIRRORED",
            Self::Scaled => "SCALED",
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
            Self::Moved | Self::Mirrored | Self::Scaled => (
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
    /// Where each change is: on the left, the right where it is, and lined
    /// up.
    pub(super) spots: Vec<[Option<Spot>; 3]>,
    /// Both sides where they are, and lined up.
    frame: (World, World),
    lined_frame: (World, World),
}

/// Where a change is on one side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Spot {
    /// Outlined when selected: a chemical change's painted atoms and bonds,
    /// anything else its objects.
    pub(super) outline: (World, World),
    /// Zoomed to: its objects, such as the whole molecule, around it.
    pub(super) context: (World, World),
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

/// `flipped left–right`, `flipped top–bottom` or `mirrored`, by the mirror
/// line in degrees.
fn flip(axis: f64) -> &'static str {
    let off = |line: f64| (axis - line).abs().min(180. - (axis - line).abs());
    if off(90.) < 1. {
        "flipped left–right"
    } else if off(0.) < 1. {
        "flipped top–bottom"
    } else {
        "mirrored"
    }
}

/// `1.3 cm left, 0.5 cm down`, how far it turned, `flipped left–right` and
/// `scaled to 120%`.
fn movement(doc: &Document, detail: &Detail) -> String {
    let Detail::Move {
        dx,
        dy,
        degrees,
        scale,
        mirror,
    } = *detail
    else {
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
    if let Some(axis) = mirror {
        parts.push(flip(axis).to_owned());
    } else if degrees.abs() >= 0.5 {
        let way = if degrees < 0. {
            "counterclockwise"
        } else {
            "clockwise"
        };
        parts.push(format!("turned {:.0}° {way}", degrees.abs()));
    }
    if (scale * 100.).round() != 100. {
        parts.push(format!("scaled to {:.0}%", scale * 100.));
    }
    parts.join(", ")
}

/// One part of a chemical edit in words: `OH added at C4`, `Cl → Br at C2`,
/// `C1–C2 single → double`, `inverted at C3`.
fn part_text(part: &Part) -> String {
    let at = part.at.join(", ");
    let place = |word: &str| {
        if at.is_empty() {
            String::new()
        } else {
            format!(" {word} {at}")
        }
    };
    match part.kind {
        PartKind::Added => format!("{} added{}", part.what, place("at")),
        PartKind::Removed => format!("{} removed{}", part.what, place("from")),
        PartKind::Replaced | PartKind::Atom => format!("{}{}", part.what, place("at")),
        PartKind::Bond if part.at.is_empty() => format!("bond {}", part.what),
        PartKind::Bond => format!("{} {}", part.at.join("–"), part.what),
        PartKind::BondAdded | PartKind::BondRemoved => {
            let done = if part.kind == PartKind::BondAdded {
                "added"
            } else {
                "removed"
            };
            if part.at.is_empty() {
                format!("bond {done}")
            } else {
                format!("{} bond {done}", part.at.join("–"))
            }
        }
        PartKind::Inverted if part.at.is_empty() => "stereocenter inverted".into(),
        PartKind::Inverted => format!("inverted{}", place("at")),
        PartKind::Stereo if part.at.is_empty() => "stereo changed".into(),
        PartKind::Stereo => format!("stereo changed{}", place("at")),
    }
}

/// The first two parts, then how many more, the first letter capitalised.
fn parts_title(parts: &[Part]) -> String {
    let mut words: Vec<String> = parts.iter().take(2).map(part_text).collect();
    if parts.len() > 2 {
        words.push(format!("+{} more", parts.len() - 2));
    }
    let words = words.join(", ");
    let mut chars = words.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
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

/// The line of one change. `same` says the molecule's chemistry did not
/// change.
fn change_row(index: usize, change: &Change, left: &Document, right: &Document, same: bool) -> Row {
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
                Detail::Edit {
                    mirror_image: true, ..
                } => "Mirror image: every stereocenter inverted".to_owned(),
                Detail::Edit { parts, .. } if !parts.is_empty() => parts_title(parts),
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
            let detail = if matches!(
                change.detail,
                Detail::Edit {
                    by_structure: true,
                    ..
                }
            ) {
                "Different chemistry · matched by structure (ids changed)"
            } else {
                "Different chemistry"
            };
            (Tag::Chem, title, detail.to_owned(), Some(smiles))
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
        Kind::Mirrored | Kind::Scaled => {
            let (dx, dy, degrees, scale, mirror) = match change.detail {
                Detail::Move {
                    dx,
                    dy,
                    degrees,
                    scale,
                    mirror,
                } => (dx, dy, degrees, scale, mirror),
                _ => (0., 0., 0., 1., None),
            };
            let mirrored = change.kind == Kind::Mirrored;
            // The title says the flip or the scale, the detail the rest.
            let rest = Detail::Move {
                dx,
                dy,
                degrees: if mirrored { 0. } else { degrees },
                scale: if mirrored { scale } else { 1. },
                mirror: None,
            };
            let mut detail = movement(left, &rest);
            // The Flip tool swaps wedges so the configuration stays.
            let wedged = right.bonds.iter().any(|bond| {
                change.right.objects.contains(&bond.a)
                    && !bond.projection
                    && matches!(
                        bond.display.as_str(),
                        "wedge" | "hash" | "hollow_wedge" | "hashed" | "bold"
                    )
            });
            if mirrored && molecule && same && wedged {
                detail = [detail, "Wedges swapped; same configuration".into()]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
            }
            if mirrored {
                let title = format!("{name} {}", flip(mirror.unwrap_or(0.)));
                (Tag::Mirrored, title, detail, None)
            } else {
                let title = format!("{name} scaled to {:.0}%", scale * 100.);
                (Tag::Scaled, title, detail, None)
            }
        }
        Kind::Redrawn if molecule => (
            Tag::Moved,
            "Molecule redrawn".into(),
            "Its shape changed; the same molecule".into(),
            None,
        ),
        Kind::Redrawn => {
            let detail = match &change.detail {
                Detail::Reason(reason) => reason.clone(),
                _ => String::new(),
            };
            (Tag::Moved, format!("{name} redrawn"), detail, None)
        }
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
        Kind::SchemeMoved
        | Kind::Moved
        | Kind::Rotated
        | Kind::Mirrored
        | Kind::Scaled
        | Kind::Redrawn => 2,
        Kind::Style => 3,
    }
}

fn rows(report: &Report, left: &Document, right: &Document) -> Vec<Row> {
    let mut rows: Vec<Row> = report
        .changes
        .iter()
        .enumerate()
        .map(|(i, change)| {
            let same = report.molecules.iter().any(|pair| {
                pair.right == change.right.objects && pair.chemistry == Chemistry::Same
            });
            change_row(i, change, left, right, same)
        })
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
        Kind::SchemeMoved
        | Kind::Moved
        | Kind::Rotated
        | Kind::Mirrored
        | Kind::Scaled
        | Kind::Redrawn => ComparePaint::Moved,
        Kind::Style => ComparePaint::Style,
        Kind::Text => ComparePaint::Text,
    }
}

/// The marks of every change on one side, and where each change is there.
/// `side` gives a change's marks on this side and on the other.
fn marks(
    doc: &Document,
    report: &Report,
    side: impl Fn(&Change) -> [&reshiki::compare::Marks; 2],
) -> (Vec<CompareMark>, Vec<Option<Spot>>) {
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
        let [marks, other] = side(change);
        let objects = reshiki::scene::selection_bounds(doc, &marks.objects);
        let (paint, chemistry) = (paint(change), change.category() == Category::Chemistry);
        let mut mark = |place| {
            found.push(CompareMark {
                paint,
                chemistry,
                place,
            });
        };
        // A chemical change is where its painted atoms and bonds are, not
        // its whole molecule.
        let mut painted: Vec<World> = Vec::new();
        if paint == ComparePaint::Chemical {
            for position in marks.atoms.iter().filter_map(at) {
                mark(ComparePlace::Atom(position));
                painted.push(position);
            }
            for (a, b) in &marks.bonds {
                // Inside a collapsed label, the label's mark shows it.
                match (at(a), at(b)) {
                    (Some(a), Some(b)) if a != b => mark(ComparePlace::Bond(a, b)),
                    (Some(a), Some(_)) => mark(ComparePlace::Atom(a)),
                    _ => continue,
                }
                painted.extend(at(a).into_iter().chain(at(b)));
            }
            // Nothing painted here, as for an atom only added: where the
            // other side's painted atoms are here, such as the one it is on.
            if painted.is_empty() {
                let ends = other.bonds.iter().flat_map(|(a, b)| [a, b]);
                painted.extend(other.atoms.iter().chain(ends).filter_map(at));
            }
        } else if let Some((lo, hi)) = objects {
            mark(ComparePlace::Box(lo, hi));
        }
        let around = painted
            .iter()
            .fold(None, |bounds: Option<(World, World)>, p| {
                Some(bounds.map_or((*p, *p), |bounds| union(bounds, (*p, *p))))
            });
        let outline = around.or(objects);
        // Half a bond around it, so labels at its edge are not cut.
        let pad = doc.drawing_style.bond_length_world / 2.;
        spots.push(outline.map(|outline| {
            let (lo, hi) = objects.map_or(outline, |objects| union(objects, outline));
            Spot {
                outline,
                context: (lo.offset(-pad, -pad), hi.offset(pad, pad)),
            }
        }));
    }
    (found, spots)
}

/// Compares, and lays out what to draw. Slow for a big drawing.
fn build(left: &Document, right: &Document) -> Shown {
    let report = reshiki::compare::compare(left, right);
    let aligned = reshiki::compare::aligned(right, &report);
    let rows = rows(&report, left, right);
    let (on_left, at_left) = marks(left, &report, |change| [&change.left, &change.right]);
    let (on_right, at_right) = marks(right, &report, |change| [&change.right, &change.left]);
    let (lined, at_lined) = marks(&aligned, &report, |change| [&change.right, &change.left]);
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

/// History's sides: a save (`Saved 14:02`) or a commit's label, then now.
pub(super) fn history_sides(label: &str) -> [Side; 2] {
    let mut meta = label.to_owned();
    if let Some(rest) = label.strip_prefix("Saved") {
        meta = format!("saved{rest}");
    }
    [
        Side::new("Version", meta),
        Side::new("Your drawing", "open now"),
    ]
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

    /// The mode switch, the line-up switch, then both drawings, the compared
    /// version on the left and the drawing now on the right (see [`SIDES`]),
    /// and the list of what changed. A wide drawing in tall panes stacks them,
    /// the compared one on top. `under` goes below each drawing, and
    /// `footer`, given how the changes read, below them all.
    pub(super) fn view<'a>(
        &'a self,
        sides: [Side; 2],
        under: impl Fn() -> [Option<Element<'a, Message>>; 2] + 'a,
        footer: impl Fn(String) -> Element<'a, Message> + 'a,
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
        let spots = [spot(0), spot(if lined { 2 } else { 1 })];
        let [left_frame, right_frame] = zoomed(spots, frame).unwrap_or([frame; 2]);
        let spots = spots.map(|spot| spot.map(|spot| spot.outline));
        let none: &[CompareMark] = &[];
        let preview = move |doc: &'a Document, marks: &'a [CompareMark], selected, frame| {
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
        let right_marks = shown.map_or(none, |shown| {
            if lined {
                &shown.marks.lined
            } else {
                &shown.marks.right
            }
        });
        let panes = responsive(move |size| {
            let [left_under, right_under] = under();
            let chrome = HEADER + if left_under.is_some() { UNDER } else { 0. };
            let stacked = stacks(
                Size::new(size.width - LIST - GAP, size.height - FOOTER - 12.),
                frame,
                chrome,
            );
            let left: Element<'a, Message> = match &self.left {
                Ok(doc) => preview(
                    doc,
                    shown.map_or(none, |shown| &shown.marks.left),
                    spots[0],
                    left_frame,
                )
                .into(),
                Err(note) => container(text(note).size(13).style(muted_text))
                    .center(Length::Fill)
                    .into(),
            };
            let right = preview(self.right_shown(), right_marks, spots[1], right_frame);
            let [left_role, right_role] = SIDES;
            let [left_side, right_side] = sides.clone();
            let [left, right] = [
                pane(left_role, left_side, left, left_under),
                pane(right_role, right_side, right.into(), right_under),
            ];
            let panes: Element<'a, Message> = if stacked {
                column![left, right].spacing(GAP).into()
            } else {
                row![left, right].spacing(GAP).into()
            };
            column![
                row![panes, self.list()].spacing(GAP).height(Length::Fill),
                footer(reading(&sides, stacked)),
            ]
            .spacing(12)
            .into()
        });
        column![bar, panes].spacing(10).into()
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
            swatch(ComparePaint::Moved, "Moved, mirrored, scaled").into(),
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
        .width(LIST)
        .into()
    }

    /// From Versions: a version beside the drawing now.
    pub(super) fn dialog<'a>(&'a self, label: &'a str) -> Element<'a, Message> {
        let sides = history_sides(label);
        let button = |id: &'static str, name: &'static str, message: Message| {
            reshiki::accessibility::button(id, name, text(name).size(13))
                .padding([7, 12])
                .style(crate::appearance::secondary)
                .on_press(message)
        };
        let footer = move |reading: String| {
            row![
                text(reading).size(12).style(muted_text),
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
            .align_y(Alignment::Center)
            .into()
        };
        column![
            self.title("Compare".into()),
            self.view(sides, || [None, None], footer),
        ]
        .spacing(12)
        .into()
    }

    /// A dialog's title with the verdict beside it.
    pub(super) fn title(&self, title: String) -> iced::widget::Row<'_, Message> {
        row![text(title).size(18)]
            .push(self.verdict_badge())
            .spacing(10)
            .align_y(Alignment::Center)
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

/// What a pane shows: the version compared, or the drawing now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Role {
    Compared,
    Current,
}

/// The panes left to right: changes read from the version to the drawing now.
pub(super) const SIDES: [Role; 2] = [Role::Compared, Role::Current];

impl Role {
    pub(super) fn pill(self) -> &'static str {
        match self {
            Self::Compared => "COMPARED",
            Self::Current => "CURRENT",
        }
    }

    /// Frame, header background, pill background and pill text: green for
    /// the drawing now, grey for the version; neither is a change colour.
    fn colors(self, dark: bool) -> [Color; 4] {
        let rgb = |[r, g, b]: [u8; 3]| Color::from_rgb8(r, g, b);
        let [frame, header, pill, ink] = match (self, dark) {
            (Self::Current, false) => [
                [47, 122, 95],
                [233, 244, 238],
                [47, 122, 95],
                [255, 255, 255],
            ],
            (Self::Current, true) => [[127, 208, 172], [31, 52, 43], [127, 208, 172], [20, 23, 22]],
            (Self::Compared, false) => [
                [169, 176, 171],
                [244, 245, 244],
                [230, 232, 230],
                [74, 81, 77],
            ],
            (Self::Compared, true) => {
                [[110, 118, 113], [38, 43, 40], [52, 58, 54], [196, 203, 198]]
            }
        };
        [rgb(frame), rgb(header), rgb(pill), rgb(ink)]
    }
}

/// How a pane names its version: `Your drawing` and `open now`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Side {
    pub(super) name: String,
    pub(super) meta: String,
}

impl Side {
    pub(super) fn new(name: &str, meta: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            meta: meta.into(),
        }
    }

    /// `Current: Your drawing · open now`, for screen readers.
    pub(super) fn spoken(&self, role: Role) -> String {
        let role = match role {
            Role::Compared => "Compared",
            Role::Current => "Current",
        };
        if self.meta.is_empty() {
            format!("{role}: {}", self.name)
        } else {
            format!("{role}: {} · {}", self.name, self.meta)
        }
    }
}

/// `Changes read from version (left) to your drawing (right)`, or top to
/// bottom when the panes are stacked.
pub(super) fn reading(sides: &[Side; 2], stacked: bool) -> String {
    // `On GitHub` reads `on GitHub`: only the first letter is lowered.
    let [from, to] = sides.each_ref().map(|side| {
        let mut chars = side.name.chars();
        chars
            .next()
            .map(|first| first.to_lowercase().chain(chars).collect::<String>())
            .unwrap_or_default()
    });
    let (first, second) = if stacked {
        ("top", "bottom")
    } else {
        ("left", "right")
    };
    format!("Changes read from {from} ({first}) to {to} ({second})")
}

/// Whether the panes, in `size` beside the list, stack: when `frame` then
/// draws at least 10% larger. `chrome` is each pane's header and what is
/// under it.
fn stacks(size: Size, frame: (World, World), chrome: f32) -> bool {
    let beside = Size::new((size.width - GAP) / 2., size.height - chrome);
    let stacked = Size::new(size.width, (size.height - GAP) / 2. - chrome);
    compare_zoom(stacked, frame) >= 1.1 * compare_zoom(beside, frame)
}

/// Both panes zoomed to a change in its context: each centred where the
/// change is there, both one size so they keep one scale, and at most twice
/// as large as `frame`, the whole drawing, shows. A side without it looks
/// where the other has it.
fn zoomed([a, b]: [Option<Spot>; 2], (lo, hi): (World, World)) -> Option<[(World, World); 2]> {
    let (a, b) = (a.or(b)?.context, b.or(a)?.context);
    let w = (a.1.x - a.0.x).max(b.1.x - b.0.x).max((hi.x - lo.x) / 2.) / 2.;
    let h = (a.1.y - a.0.y).max(b.1.y - b.0.y).max((hi.y - lo.y) / 2.) / 2.;
    Some([a, b].map(|(lo, hi)| {
        let (x, y) = ((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
        (World::new(x - w, y - h), World::new(x + w, y + h))
    }))
}

/// SMILES that wraps between tokens only: a zero-width space between them,
/// and a word joiner inside each, so a bracket atom, `Cl`, `Br`, `->`, `<-`
/// or a ring bond (`1`, `%12`, `%(123)`, kept with the atom before it)
/// never breaks.
fn breakable(smiles: &str) -> String {
    let mut tokens: Vec<String> = Vec::new();
    let mut chars = smiles.chars().peekable();
    while let Some(c) = chars.next() {
        let mut token = String::from(c);
        let close = match c {
            '[' => Some(']'),
            '%' if chars.peek() == Some(&'(') => Some(')'),
            _ => None,
        };
        if let Some(close) = close {
            for inner in chars.by_ref() {
                token.push(inner);
                if inner == close {
                    break;
                }
            }
        } else if c == '%' {
            token.extend(chars.next_if(char::is_ascii_digit));
            token.extend(chars.next_if(char::is_ascii_digit));
        } else {
            token.extend(chars.next_if(|next| {
                matches!(
                    (c, *next),
                    ('C', 'l') | ('B', 'r') | ('-', '>') | ('<', '-')
                )
            }));
        }
        match tokens.last_mut() {
            Some(last) if c.is_ascii_digit() || c == '%' => last.push_str(&token),
            _ => tokens.push(token),
        }
    }
    let joined: Vec<String> = tokens
        .iter()
        .map(|token| {
            token
                .chars()
                .map(String::from)
                .collect::<Vec<_>>()
                .join("\u{2060}")
        })
        .collect();
    joined.join("\u{200B}")
}

/// One pane: its frame and header say which side it is. The frame is drawn
/// over the pane so the compared one can be dashed.
fn pane<'a>(
    role: Role,
    side: Side,
    body: Element<'a, Message>,
    under: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let spoken = side.spoken(role);
    let pill = container(text(role.pill()).size(9.5).font(bold()))
        .padding([2, 7])
        .style(move |theme: &Theme| {
            let [_, _, background, ink] = role.colors(crate::appearance::is_dark(theme));
            container::Style {
                background: Some(background.into()),
                text_color: Some(ink),
                border: Border {
                    radius: 99.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }
        });
    let name = text(side.name).size(13).font(bold());
    let mut words = row![pill, name.wrapping(text::Wrapping::None)]
        .spacing(6)
        .align_y(Alignment::Center);
    if !side.meta.is_empty() {
        words = words.push(
            text(format!("· {}", side.meta))
                .size(12)
                .style(muted_text)
                .wrapping(text::Wrapping::None),
        );
    }
    let header = container(words)
        .padding([6, 10])
        .width(Length::Fill)
        .clip(true)
        .style(move |theme: &Theme| container::Style {
            background: Some(role.colors(crate::appearance::is_dark(theme))[1].into()),
            border: Border {
                radius: iced::border::Radius::default().top(9.),
                ..Border::default()
            },
            ..container::Style::default()
        });
    let drawing = reshiki::accessibility::image(
        format!("github-compare-{}", role.pill().to_lowercase()),
        spoken,
        container(body).height(Length::Fill).width(Length::Fill),
    );
    let framed = iced::widget::stack![
        column![header, drawing],
        crate::canvas::layered::canvas(CompareFrame {
            colors: [role.colors(false)[0], role.colors(true)[0]],
            dashed: role == Role::Compared,
        })
        .width(Length::Fill)
        .height(Length::Fill),
    ];
    column![framed]
        .push(under)
        .spacing(6)
        .width(Length::FillPortion(1))
        .into()
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
    // Before, then `→ after` under it.
    let (before, after) = smiles.map_or((None, None), |smiles| match smiles.split_once(" → ") {
        Some((before, after)) => (Some(before), Some(after)),
        None => (Some(smiles), None),
    });
    for (arrow, code) in [("", before), ("→ ", after)] {
        words = words.push(code.map(|code| {
            text(format!("{arrow}{}", breakable(code)))
                .size(11)
                .font(iced::Font::MONOSPACE)
                .wrapping(text::Wrapping::WordOrGlyph)
        }));
    }
    let mut name = format!("{}: {title}", tag.label());
    if let Some(smiles) = smiles {
        name = format!("{name}, {smiles}");
    }
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
