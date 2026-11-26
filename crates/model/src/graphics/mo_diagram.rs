//! A placed molecular orbital diagram and its vector layout.
use std::ops::RangeInclusive;

use reshiki_qcdata::Spin;
use serde::{Deserialize, Serialize};

use super::{BracketSides, Graphic, GraphicKind, GraphicStyle, LinePattern, PathCommand};
use crate::{
    document::{Annotation, Document, Point, text_angle},
    models3d::{self, Model3d, surface::Figure},
    orbital_levels::Column,
    scene::Primitive,
    scientific::Part,
    style::{self, DrawingStyle},
    typography::{TextFormat, TextStyle},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoDiagram {
    /// An atom of the molecule whose model draws the pictures.
    pub molecule: Option<u64>,
    /// The result key for the energy levels.
    pub result: String,
    /// Whether the diagram follows reruns.
    pub linked: bool,
    pub columns: Vec<Column>,
    /// Levels included below the HOMO and above the LUMO, respectively.
    pub below: u8,
    pub above: u8,
    pub pictures: Vec<Figure>,
}

impl MoDiagram {
    pub fn new(molecule: u64, result: String, columns: Vec<Column>) -> Self {
        Self {
            molecule: Some(molecule),
            result,
            linked: true,
            columns,
            below: 2,
            above: 2,
            pictures: vec![],
        }
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        if !(1..=2).contains(&self.columns.len())
            || self.columns.iter().any(|c| {
                c.energies_ev.len() > 4096
                    || c.energies_ev.iter().any(|e| !e.is_finite())
                    || (!c.occupations.is_empty() && c.occupations.len() != c.energies_ev.len())
                    || c.occupations.iter().any(|n| !n.is_finite())
                    || c.occupied.is_some_and(|n| n > c.energies_ev.len())
            })
        {
            return Err("Invalid MO diagram columns".into());
        }
        if self.below > 8 || self.above > 8 || self.result.chars().count() > 64 {
            return Err("Invalid MO diagram range or result key".into());
        }
        if self.pictures.len() > 16
            || self.pictures.iter().any(|p| {
                p.orbital.result.chars().count() > 64
                    || !self.columns.iter().any(|c| {
                        c.spin == p.orbital.spin
                            && c.energies_ev.get(p.orbital.index as usize).is_some()
                    })
            })
        {
            return Err("Invalid MO diagram pictures".into());
        }
        Ok(())
    }

    /// Retain the chosen frontier levels and their appearance on a new result.
    pub fn relink(&mut self, result: String, columns: Vec<Column>) {
        self.pictures.retain_mut(|picture| {
            let index = picture.orbital.index as usize;
            let Some(old) = self
                .columns
                .iter()
                .find(|c| c.spin == picture.orbital.spin && c.energies_ev.get(index).is_some())
            else {
                return false;
            };
            let Some(new) = columns
                .iter()
                .find(|c| c.label == old.label && (c.spin == old.spin || !old.label.is_empty()))
            else {
                return false;
            };
            let index = match (old.occupied, new.occupied) {
                (Some(a), Some(b)) if index >= a => b.checked_add(index - a),
                (Some(a), Some(b)) => b.checked_sub(a - index),
                (None, None) => Some(index),
                _ => None,
            };
            let Some(index) = index.filter(|&i| i < new.energies_ev.len()) else {
                return false;
            };
            picture.orbital.result.clone_from(&result);
            picture.orbital.index = index as u32;
            picture.orbital.spin = new.spin;
            picture.snapshot = None;
            true
        });
        self.result = result;
        self.columns = columns;
    }

    pub fn natural_size(&self, doc_style: &DrawingStyle) -> (f32, f32) {
        let layout = Layout::new(self, doc_style);
        (layout.width, layout.height)
    }

    /// Hit a level bar or label using the default drawing style.
    pub fn level_at(&self, g: &Graphic, p: Point) -> Option<(Spin, usize)> {
        self.level_at_with_style(g, p, &style::DEFAULT)
    }

    /// Hit a level in a diagram laid out with the supplied drawing style.
    pub fn level_at_with_style(
        &self,
        g: &Graphic,
        p: Point,
        doc_style: &DrawingStyle,
    ) -> Option<(Spin, usize)> {
        let det = g.frame_determinant();
        if !det.is_finite() || det == 0. {
            return None;
        }
        let layout = Layout::new(self, doc_style);
        let q = g.world_to_local(p, det);
        if !(0. ..=1.).contains(&q.x) || !(0. ..=1.).contains(&q.y) {
            return None;
        }
        let q = Point::new(q.x * layout.width, q.y * layout.height);
        for c in &layout.columns {
            for group in &c.groups {
                if (group.y - q.y).abs() <= layout.font.size() * 0.55 {
                    for (index, left, right) in c.bars(group, layout.font.size()) {
                        if (left..=right).contains(&q.x) {
                            return Some((c.column.spin, index));
                        }
                    }
                }
            }
        }
        layout.labels.iter().find_map(|label| {
            let orbital = label.orbital?;
            layout.annotation(g, label).contains(p).then_some(orbital)
        })
    }

    pub(crate) fn model<'a>(&self, doc: &'a Document) -> Option<&'a Model3d> {
        models3d::model_of(doc, self.molecule?)
    }

    pub(crate) fn picture_model(&self, doc: &Document, index: usize) -> Option<Model3d> {
        let mut small = models3d::panel_model(self.model(doc)?, self.pictures.get(index)?);
        small.display = models3d::Display::ThreeD;
        small.morph = None;
        Some(small)
    }

    pub(crate) fn refresh_snapshots(&mut self, doc: &Document) {
        for index in 0..self.pictures.len() {
            if let Some(mut small) = self.picture_model(doc, index)
                && models3d::surface::draw::figure_ready(doc, &small)
            {
                models3d::refresh_surface_snapshot(doc, &mut small);
                if let Some((saved, picture)) = small.surface.zip(self.pictures.get_mut(index)) {
                    picture.snapshot = saved.snapshot;
                }
            }
        }
    }

    pub(crate) fn draw(
        &self,
        g: &Graphic,
        doc: &Document,
    ) -> (Vec<Part>, Vec<Annotation>, Vec<Primitive>) {
        let layout = Layout::new(self, &doc.drawing_style);
        let (mut parts, labels) = layout.ink(g);
        let mut pictures = vec![];
        let side = layout.picture_world_side(g);
        let angle = text_angle(g.axis_x, g.axis_y).to_radians();
        let (sin, cos) = angle.sin_cos();
        if side.is_finite() && side > 0. && self.model(doc).is_some() {
            for slot in &layout.pictures {
                let Some(model) = self.picture_model(doc, slot.index) else {
                    continue;
                };
                let centre = layout.world(g, slot.centre);
                if models3d::push_orbital_picture(doc, &model, centre, side, &mut pictures)
                    .is_none()
                {
                    continue;
                }
                let from = layout.world(g, slot.connector);
                let label_extension = slot.label_width * (layout.y_scale(g) - layout.x_scale(g));
                let start = from.offset(cos * label_extension, sin * label_extension);
                let half = side / (2. * g.axis_x.x.abs().max(g.axis_x.y.abs()));
                let left = centre.offset(-g.axis_x.x * half, -g.axis_x.y * half);
                if (left.x - start.x) * g.axis_x.x + (left.y - start.y) * g.axis_x.y > 0. {
                    parts.push(line_part(
                        g,
                        vec![PathCommand::Move(start), PathCommand::Line(left)],
                        LinePattern::Dashed,
                        0.65,
                    ));
                }
            }
        }
        (parts, labels, pictures)
    }
}

impl Graphic {
    pub fn mo_diagram(
        id: u64,
        origin: Point,
        size: Point,
        mo: MoDiagram,
        style: GraphicStyle,
    ) -> Self {
        let mut g = Self::dragged(
            id,
            GraphicKind::MoDiagram,
            origin,
            origin.offset(size.x, size.y),
            style,
            BracketSides::Both,
            false,
        );
        g.origin = origin;
        g.axis_x = Point::new(size.x, 0.);
        g.axis_y = Point::new(0., size.y);
        g.mo = Some(Box::new(mo));
        g
    }

    /// The diagram's vector strokes and labels, without embedded 3D pictures.
    pub fn mo_parts(&self, style: &DrawingStyle) -> (Vec<Part>, Vec<Annotation>) {
        self.mo
            .as_deref()
            .map_or_else(Default::default, |mo| Layout::new(mo, style).ink(self))
    }
}

struct Group {
    levels: RangeInclusive<usize>,
    energy: f64,
    y: f32,
    labels: Vec<(usize, String)>,
}

struct PlacedColumn<'a> {
    column: &'a Column,
    groups: Vec<Group>,
    left: f32,
    bar_space: f32,
    label_space: f32,
    beta: bool,
}

impl PlacedColumn<'_> {
    fn bars(&self, group: &Group, font_size: f32) -> impl Iterator<Item = (usize, f32, f32)> {
        let count = level_count(&group.levels) as f32;
        let gap = (font_size * 0.22).min(self.bar_space / (count * 3.));
        let width = (font_size * 2.5).min((self.bar_space - gap * (count - 1.)) / count);
        let left = self.left;
        group.levels.clone().enumerate().map(move |(i, index)| {
            let x = left + i as f32 * (width + gap);
            (index, x, x + width)
        })
    }
}

struct Label {
    text: String,
    left: Point,
    size: f32,
    orbital: Option<(Spin, usize)>,
}

struct PictureSlot {
    index: usize,
    centre: Point,
    connector: Point,
    label_width: f32,
}

struct Layout<'a> {
    width: f32,
    height: f32,
    font: TextStyle,
    columns: Vec<PlacedColumn<'a>>,
    labels: Vec<Label>,
    lines: Vec<Vec<Point>>,
    pictures: Vec<PictureSlot>,
    picture_side: f32,
    closed: bool,
}

impl<'a> Layout<'a> {
    fn new(mo: &'a MoDiagram, style: &DrawingStyle) -> Self {
        let font = style.text_style();
        let em = font.size();
        let tick_size = em * 0.82;
        let closed =
            mo.columns.len() == 1 && mo.columns.first().is_some_and(|c| c.label.is_empty());
        let width_of = |text: &str, size| style::styled_text_width(text, size, &font);

        let mut columns: Vec<_> = mo
            .columns
            .iter()
            .enumerate()
            .map(|(i, column)| {
                let groups = groups(column, mo.below, mo.above, closed);
                let slots = groups
                    .iter()
                    .map(|g| level_count(&g.levels))
                    .max()
                    .unwrap_or(1) as f32;
                let label_space = groups
                    .iter()
                    .map(|g| {
                        g.labels
                            .iter()
                            .map(|(_, text)| width_of(text, em) + em * 0.35)
                            .sum()
                    })
                    .fold(em, f32::max);
                PlacedColumn {
                    column,
                    groups,
                    left: 0.,
                    bar_space: em * (2.5 * slots + 0.22 * (slots - 1.)),
                    label_space,
                    beta: column.label == "β" || (column.label.is_empty() && i == 1),
                }
            })
            .collect();

        let energies = columns
            .iter()
            .flat_map(|c| c.groups.iter().map(|g| g.energy));
        let (ticks, unit) = axis_ticks(energies);
        let tick_labels: Vec<_> = ticks
            .iter()
            .map(|&tick| tick_text((tick * unit).clamp(-f64::MAX, f64::MAX)))
            .collect();
        let axis = em * 1.25
            + tick_labels
                .iter()
                .map(|label| width_of(label, tick_size))
                .fold(em, f32::max);
        let homo = frontier(&columns, Column::homo, f64::total_cmp);
        let lumo = frontier(&columns, Column::lumo, |a, b| b.total_cmp(a));
        let gap = homo
            .zip(lumo)
            .map(|((hi, he), (li, le))| (hi, li, format!("gap {} eV", signed(le - he))));
        let bracket = axis + em + gap.as_ref().map_or(0., |(_, _, text)| width_of(text, em));
        let mut width = bracket + em;
        for c in &mut columns {
            c.left = width;
            width += c.bar_space + c.label_space + em * 1.6;
        }
        let picture_side = style.bond_length_world * 2.2;
        let picture_count = mo
            .pictures
            .iter()
            .filter(|p| {
                columns.iter().any(|c| {
                    c.column.spin == p.orbital.spin
                        && c.groups
                            .iter()
                            .any(|g| g.levels.contains(&(p.orbital.index as usize)))
                })
            })
            .count();
        let picture_x = width + picture_side / 2.;
        if picture_count > 0 {
            width += picture_side + em;
        }

        let spacing = em * 1.45;
        let top = em * 2.5;
        let plot_height = columns
            .iter()
            .map(|c| c.groups.len().saturating_sub(1) as f32 * spacing)
            .fold(em * 6., f32::max);
        let pictures_height = picture_count as f32 * (picture_side + em * 0.25);
        let height = (top + plot_height + em * 1.4).max(pictures_height + em * 2.);
        let bottom = height - em * 1.4;
        let low = ticks.first().copied().unwrap_or(-1.);
        let high = ticks.last().copied().unwrap_or(1.);
        let y = |energy| top + (bottom - top) * ((high - energy) / (high - low)) as f32;
        for c in &mut columns {
            let count = c.groups.len();
            let mut previous = top - spacing;
            for (i, g) in c.groups.iter_mut().enumerate() {
                g.y = spaced(y(g.energy / unit), previous, spacing, bottom, count - 1 - i);
                previous = g.y;
            }
        }

        let mut labels = vec![];
        let mut label = |text: String, x, y, size, orbital| {
            labels.push(Label {
                text,
                left: Point::new(x, y),
                size,
                orbital,
            });
        };
        label(
            "eV".into(),
            axis - width_of("eV", tick_size) / 2.,
            em,
            tick_size,
            None,
        );
        let mut lines = vec![vec![
            Point::new(axis, top - em * 0.5),
            Point::new(axis, bottom + em * 0.5),
        ]];
        for (tick, text) in ticks.into_iter().zip(tick_labels) {
            let y = y(tick);
            lines.push(vec![Point::new(axis - em * 0.25, y), Point::new(axis, y)]);
            let x = axis - em * 0.45 - width_of(&text, tick_size);
            label(text, x, y, tick_size, None);
        }
        if let Some((h, l, text)) = gap
            && let Some((h, l)) = group_y(&columns, h).zip(group_y(&columns, l))
        {
            lines.push(vec![
                Point::new(bracket + em * 0.3, h),
                Point::new(bracket, h),
                Point::new(bracket, l),
                Point::new(bracket + em * 0.3, l),
            ]);
            label(text, axis + em * 0.55, (h + l) / 2., em, None);
        }

        let mut pictures = vec![];
        for c in &columns {
            if !closed {
                let header = if c.beta { "β" } else { "α" };
                label(header.into(), c.left, em, em, None);
            }
            for g in &c.groups {
                let right = c.bars(g, em).last().map_or(c.left, |(_, _, right)| right);
                let label_x = right + em * 0.35;
                let mut label_end = label_x;
                for (index, text) in &g.labels {
                    label(
                        text.clone(),
                        label_end,
                        g.y,
                        em,
                        Some((c.column.spin, *index)),
                    );
                    label_end += width_of(text, em) + em * 0.35;
                }
                for (index, p) in mo.pictures.iter().enumerate() {
                    if p.orbital.spin == c.column.spin
                        && g.levels.contains(&(p.orbital.index as usize))
                        && !pictures
                            .iter()
                            .any(|slot: &PictureSlot| slot.index == index)
                    {
                        pictures.push(PictureSlot {
                            index,
                            centre: Point::new(picture_x, g.y),
                            connector: Point::new(label_end + em * 0.28, g.y),
                            label_width: label_end - label_x,
                        });
                    }
                }
            }
        }
        pictures.sort_by(|a, b| a.centre.y.total_cmp(&b.centre.y));
        let count = pictures.len();
        let step = picture_side + em * 0.25;
        let mut previous = em * 0.5 + picture_side / 2. - step;
        for (i, picture) in pictures.iter_mut().enumerate() {
            picture.centre.y = spaced(
                picture.centre.y,
                previous,
                step,
                height - em * 0.5 - picture_side / 2.,
                count - 1 - i,
            );
            previous = picture.centre.y;
        }

        Self {
            width,
            height,
            font,
            columns,
            labels,
            lines,
            pictures,
            picture_side,
            closed,
        }
    }

    fn x_scale(&self, g: &Graphic) -> f32 {
        g.axis_x.x.hypot(g.axis_x.y) / self.width
    }

    fn y_scale(&self, g: &Graphic) -> f32 {
        g.axis_y.x.hypot(g.axis_y.y) / self.height
    }

    /// Fit an upright square in its local slot, including after an affine turn.
    fn picture_world_side(&self, g: &Graphic) -> f32 {
        let x = self.width * (g.axis_y.x.abs() + g.axis_y.y.abs());
        let y = self.height * (g.axis_x.x.abs() + g.axis_x.y.abs());
        self.picture_side * g.frame_determinant().abs() / x.max(y)
    }

    fn world(&self, g: &Graphic, p: Point) -> Point {
        let (x, y) = (p.x / self.width, p.y / self.height);
        g.origin.offset(
            x * g.axis_x.x + y * g.axis_y.x,
            x * g.axis_x.y + y * g.axis_y.y,
        )
    }

    fn annotation(&self, g: &Graphic, label: &Label) -> Annotation {
        let angle = text_angle(g.axis_x, g.axis_y);
        let mut a = Annotation {
            id: g.id,
            position: Point::default(),
            text: label.text.clone(),
            format: TextFormat {
                style: TextStyle {
                    size_pt: label.size * style::DEFAULT.points_per_world() * self.y_scale(g),
                    color: g.style.stroke,
                    ..self.font.clone()
                },
                ..TextFormat::default()
            },
            angle,
        };
        let (w, h) = a.size();
        let left = self.world(g, label.left);
        let centre = left.offset(w / 2., 0.).turned(left, angle);
        a.position = centre.offset(-w / 2., -h / 2.);
        a
    }

    fn ink(&self, g: &Graphic) -> (Vec<Part>, Vec<Annotation>) {
        let path = |points: &[Point]| {
            points
                .iter()
                .enumerate()
                .map(|(i, &p)| {
                    let p = self.world(g, p);
                    if i == 0 {
                        PathCommand::Move(p)
                    } else {
                        PathCommand::Line(p)
                    }
                })
                .collect::<Vec<_>>()
        };
        let em = self.font.size();
        let mut parts: Vec<_> = self
            .lines
            .iter()
            .map(|line| line_part(g, path(line), LinePattern::Solid, 1.))
            .collect();
        for c in &self.columns {
            for group in &c.groups {
                for (i, left, right) in c.bars(group, em) {
                    let y = group.y;
                    parts.push(line_part(
                        g,
                        path(&[Point::new(left, y), Point::new(right, y)]),
                        LinePattern::Solid,
                        1.,
                    ));
                    let n = c.column.occupations.get(i).copied().unwrap_or_else(|| {
                        if c.column.occupied.is_some_and(|n| i < n) {
                            if self.closed { 2. } else { 1. }
                        } else {
                            0.
                        }
                    });
                    if n >= 0.5 {
                        let pair = self.closed && n >= 1.5;
                        let centre = (left + right) / 2.;
                        let apart = (right - left).min(em) * 0.18;
                        let arrows = [
                            (if pair { centre - apart } else { centre }, c.beta),
                            (centre + apart, true),
                        ];
                        for (x, down) in arrows.into_iter().take(if pair { 2 } else { 1 }) {
                            let stem = y - em * 0.13;
                            let top = y - em * 0.75;
                            let (tail, tip) = if down { (top, stem) } else { (stem, top) };
                            let head = (right - left).min(em) * 0.14 * if down { 1. } else { -1. };
                            parts.push(line_part(
                                g,
                                path(&[
                                    Point::new(x, tail),
                                    Point::new(x, tip),
                                    Point::new(x + head, tip + (tail - tip) * 0.32),
                                ]),
                                LinePattern::Solid,
                                1.,
                            ));
                        }
                    }
                }
            }
        }
        (
            parts,
            self.labels
                .iter()
                .map(|label| self.annotation(g, label))
                .collect(),
        )
    }
}

fn line_part(g: &Graphic, commands: Vec<PathCommand>, pattern: LinePattern, weight: f32) -> Part {
    Part {
        commands,
        style: GraphicStyle {
            stroke: g.style.stroke,
            width_pt: g.style.width_pt * weight,
            pattern,
            flat_caps: g.style.flat_caps,
            ..GraphicStyle::default()
        },
        filled: false,
    }
}

fn groups(c: &Column, below: u8, above: u8, closed: bool) -> Vec<Group> {
    let Some(last) = c.energies_ev.len().checked_sub(1) else {
        return vec![];
    };
    let (first, end) = c.occupied.map_or((0, 4.min(last)), |n| {
        (
            n.saturating_sub(1 + usize::from(below)).min(last),
            n.saturating_add(usize::from(above)).min(last),
        )
    });
    let mut index = *c.group(first).start();
    let mut groups = vec![];
    while index <= end {
        let levels = c.group(index);
        let frontiers: Vec<_> = [c.homo(), c.lumo()]
            .into_iter()
            .flatten()
            .filter(|i| levels.contains(i))
            .collect();
        let label_index = frontiers.first().copied().unwrap_or(index);
        let Some(&energy) = c.energies_ev.get(label_index) else {
            break;
        };
        let mut labelled = frontiers;
        if closed && labelled.is_empty() {
            labelled.push(label_index);
        }
        let labels = labelled
            .into_iter()
            .enumerate()
            .filter_map(|(i, index)| {
                let mut text = format!(
                    "{}  {} eV",
                    c.name(index),
                    signed(*c.energies_ev.get(index)?)
                );
                let count = level_count(&levels);
                if i == 0 && count > 1 {
                    text.push_str(&format!(" · {count}-fold"));
                }
                Some((index, text))
            })
            .collect();
        index = levels.end() + 1;
        groups.push(Group {
            levels,
            energy,
            y: 0.,
            labels,
        });
    }
    groups.sort_by(|a, b| b.energy.total_cmp(&a.energy));
    groups
}

fn level_count(levels: &RangeInclusive<usize>) -> usize {
    levels.end() - levels.start() + 1
}

type Location = (usize, usize);

fn frontier(
    columns: &[PlacedColumn<'_>],
    index: impl Fn(&Column) -> Option<usize>,
    order: impl Fn(&f64, &f64) -> std::cmp::Ordering,
) -> Option<(Location, f64)> {
    columns
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let level = index(c.column)?;
            Some(((i, level), *c.column.energies_ev.get(level)?))
        })
        .max_by(|a, b| order(&a.1, &b.1))
}

fn group_y(columns: &[PlacedColumn<'_>], (column, index): Location) -> Option<f32> {
    columns
        .get(column)?
        .groups
        .iter()
        .find(|g| g.levels.contains(&index))
        .map(|g| g.y)
}

fn spaced(ideal: f32, previous: f32, spacing: f32, bottom: f32, remaining: usize) -> f32 {
    ideal
        .min(bottom - remaining as f32 * spacing)
        .max(previous + spacing)
}

fn axis_ticks(energies: impl Iterator<Item = f64>) -> (Vec<f64>, f64) {
    let (lo, hi) = energies.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), e| {
        (lo.min(e), hi.max(e))
    });
    if lo > hi {
        return (vec![-2., -1., 0.], 1.);
    }
    let largest = lo.abs().max(hi.abs());
    let unit = if largest > 1e100 { largest } else { 1. };
    let (lo, hi) = (lo / unit, hi / unit);
    let target = ((hi - lo) / 3.)
        .max(0.2)
        .max(lo.abs().max(hi.abs()) * f64::EPSILON * 4.);
    let magnitude = 10f64.powf(target.log10().floor());
    let step = [1., 2., 2.5, 4., 5., 10.]
        .into_iter()
        .map(|n| n * magnitude)
        .find(|&n| n >= target)
        .unwrap_or(10. * magnitude);
    let grid = if step == 4. * magnitude {
        step / 2.
    } else {
        step
    };
    let first = (lo / grid).floor() * grid;
    let count = (((hi - first) / step).ceil() as usize + 1).clamp(3, 5);
    ((0..count).map(|i| first + i as f64 * step).collect(), unit)
}

fn signed(value: f64) -> String {
    if value.abs() >= 1e8 {
        format!("{value:.2e}")
    } else {
        format!("{value:.2}")
    }
    .replace('-', "−")
}

fn tick_text(value: f64) -> String {
    let text = signed(value);
    if text.contains('e') {
        text
    } else {
        text.trim_end_matches('0').trim_end_matches('.').into()
    }
}

#[cfg(test)]
mod tests;
