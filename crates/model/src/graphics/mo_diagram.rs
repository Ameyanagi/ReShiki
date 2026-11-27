//! A placed molecular orbital diagram and its vector layout.
use std::ops::{Range, RangeInclusive};

use reshiki_qcdata::Spin;
use serde::{Deserialize, Serialize};

use super::{BracketSides, Graphic, GraphicKind, GraphicStyle, LinePattern, PathCommand};
use crate::{
    document::{Annotation, Document, Point, text_angle},
    models3d::{self, Model3d, surface::Figure},
    orbital_levels::{Column, DEGENERATE_EV},
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
    pub pictures: Vec<MoPicture>,
    /// The gap is the α frontier's, as g-xTB's xtb68 conventions report it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub alpha_gap: bool,
}

/// A picture belongs to a displayed column, even when both columns share rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoPicture {
    pub column: u8,
    pub figure: Figure,
}

impl MoDiagram {
    pub fn new(molecule: u64, result: String, columns: Vec<Column>) -> Self {
        Self {
            molecule: Some(molecule),
            result,
            linked: true,
            columns: complete_spin_columns(columns),
            below: 2,
            above: 2,
            pictures: vec![],
            alpha_gap: false,
        }
    }

    pub(super) fn validate(&self, g: &Graphic) -> Result<(), String> {
        let valid_labels = match self.columns.as_slice() {
            [closed] => closed.label.is_empty(),
            [alpha, beta] => alpha.label == "α" && beta.label == "β",
            _ => false,
        };
        if !valid_labels
            || self.columns.iter().any(|c| {
                c.energies_ev.len() > 4096
                    || c.energies_ev.iter().any(|e| !e.is_finite())
                    || !c.energies_ev.is_sorted()
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
                let orbital = &p.figure.orbital;
                orbital.result.chars().count() > 64
                    || !self.columns.get(usize::from(p.column)).is_some_and(|c| {
                        c.spin == orbital.spin
                            && c.energies_ev.get(orbital.index as usize).is_some()
                    })
            })
        {
            return Err("Invalid MO diagram pictures".into());
        }
        let x = g.axis_x.x.hypot(g.axis_x.y);
        let y = g.axis_y.x.hypot(g.axis_y.y);
        let det = g.frame_determinant();
        if !x.is_finite()
            || !y.is_finite()
            || x <= 1e-3
            || y <= 1e-3
            || !det.is_finite()
            || f64::from(det.abs()) <= f64::from(x) * f64::from(y) * 1e-6
            || !g.frame_corners().into_iter().all(Point::is_finite)
        {
            return Err("An MO diagram needs a finite frame with two independent axes".into());
        }
        Ok(())
    }

    /// Retain the chosen frontier levels and their appearance on a new result.
    pub fn relink(&mut self, result: String, columns: Vec<Column>) {
        let columns = complete_spin_columns(columns);
        self.pictures.retain_mut(|picture| {
            let figure = &mut picture.figure;
            let index = figure.orbital.index as usize;
            let Some(old) = self.columns.get(usize::from(picture.column)) else {
                return false;
            };
            let Some((column, new)) = columns
                .iter()
                .enumerate()
                .find(|(_, c)| c.label == old.label)
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
            picture.column = column as u8;
            figure.orbital.result.clone_from(&result);
            figure.orbital.index = index as u32;
            figure.orbital.spin = new.spin;
            figure.snapshot = None;
            true
        });
        self.result = result;
        self.columns = columns;
    }

    pub fn natural_size(&self, doc_style: &DrawingStyle) -> (f32, f32) {
        let layout = Layout::new(self, doc_style);
        (layout.width, layout.height)
    }

    /// The smallest range that also shows this level, within the stepper limits.
    pub fn range_for_picture(&self, column: u8, index: usize) -> Option<(u8, u8)> {
        let c = self.columns.get(usize::from(column))?;
        if shown_levels(c, self.below, self.above).contains(&index) {
            return Some((self.below, self.above));
        }
        if !shown_levels(c, 8, 8).contains(&index) {
            return None;
        }
        let occupied = c.occupied?;
        let group = c.group(index);
        let below = occupied.saturating_sub(1).saturating_sub(*group.end());
        let above = group.start().saturating_sub(occupied);
        Some((self.below.max(below as u8), self.above.max(above as u8)))
    }

    /// Hit a level bar or label using the default drawing style.
    pub fn level_at(&self, g: &Graphic, p: Point) -> Option<(u8, usize)> {
        self.level_at_with_style(g, p, &style::DEFAULT)
    }

    /// Hit a level in a diagram laid out with the supplied drawing style.
    pub fn level_at_with_style(
        &self,
        g: &Graphic,
        p: Point,
        doc_style: &DrawingStyle,
    ) -> Option<(u8, usize)> {
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
                            return Some((c.index, index));
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
        self.orbital_model(doc, &self.pictures.get(index)?.figure)
    }

    /// The model that draws a picture of this orbital in the diagram.
    pub fn orbital_model(&self, doc: &Document, figure: &Figure) -> Option<Model3d> {
        let mut small = models3d::panel_model(self.model(doc)?, figure);
        small.display = models3d::Display::ThreeD;
        small.morph = None;
        if !models3d::surface::draw::supports_style(small.style) {
            small.style = models3d::Style::BallStick;
        }
        Some(small)
    }

    pub(crate) fn refresh_snapshots(&mut self, doc: &Document) {
        for index in 0..self.pictures.len() {
            if let Some(mut small) = self.picture_model(doc, index)
                && models3d::surface::draw::figure_ready(doc, &small)
            {
                models3d::refresh_surface_snapshot(doc, &mut small);
                if let Some((saved, picture)) = small.surface.zip(self.pictures.get_mut(index)) {
                    picture.figure.snapshot = saved.snapshot;
                }
            }
        }
    }

    pub(crate) fn draw(
        &self,
        g: &Graphic,
        style: &DrawingStyle,
        source: &Document,
    ) -> (Vec<Part>, Vec<Annotation>, Vec<Primitive>) {
        let layout = Layout::new(self, style);
        let (mut parts, labels) = layout.ink(g);
        let mut pictures = vec![];
        let side = layout.picture_world_side(g);
        if side.is_finite() && side > 0. && self.model(source).is_some() {
            for slot in &layout.pictures {
                let Some(model) = self.picture_model(source, slot.index) else {
                    continue;
                };
                let centre = layout.world(g, slot.centre);
                if models3d::push_orbital_picture(source, &model, centre, side, &mut pictures)
                    .is_none()
                {
                    continue;
                }
                if let Some([start, left]) = layout.picture_connector(g, slot, &labels, side) {
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

/// A partial open-shell result can supply levels for just one spin.
fn complete_spin_columns(mut columns: Vec<Column>) -> Vec<Column> {
    if let [column] = columns.as_slice() {
        let (index, label, spin) = match column.label.as_str() {
            "α" => (1, "β", Spin::Beta),
            "β" => (0, "α", Spin::Alpha),
            _ => return columns,
        };
        columns.insert(
            index,
            Column {
                spin,
                label: label.into(),
                energies_ev: vec![],
                occupations: vec![],
                occupied: None,
            },
        );
    }
    columns
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

    /// The same small orbital models that the diagram uses to draw its pictures.
    pub fn mo_picture_models<'a>(
        &'a self,
        doc: &'a Document,
    ) -> impl Iterator<Item = Model3d> + 'a {
        self.mo.as_deref().into_iter().flat_map(move |mo| {
            (0..mo.pictures.len()).filter_map(move |index| mo.picture_model(doc, index))
        })
    }
}

struct Group {
    levels: RangeInclusive<usize>,
    energy: f64,
    row: usize,
    y: f32,
    labels: Vec<(usize, String)>,
}

struct PlacedColumn<'a> {
    index: u8,
    column: &'a Column,
    groups: Vec<Group>,
    left: f32,
    bar_space: f32,
    label_space: f32,
    beta: bool,
    picture_x: f32,
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
    orbital: Option<(u8, usize)>,
}

struct PictureSlot {
    index: usize,
    centre: Point,
    connector: Point,
    label_width: f32,
    label: Option<usize>,
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
        // The gap as the Summary reports it: a closed shell's own; the α
        // frontier for shared α/β orbitals (xTB open shells) and for xtb68
        // conventions; otherwise the extremes of both spins, when both have
        // occupations and one has a whole frontier.
        let whole = |c: &Column| c.homo().is_some() && c.lumo().is_some();
        let gap_columns: &[Column] = match &mo.columns[..] {
            [a, b] if a.spin == b.spin || mo.alpha_gap => std::slice::from_ref(a),
            [a, b] if a.occupied.is_none() || b.occupied.is_none() || !(whole(a) || whole(b)) => {
                &[]
            }
            columns => columns,
        };
        let homo = frontier(gap_columns, Column::homo, f64::total_cmp);
        let lumo = frontier(gap_columns, Column::lumo, |a, b| b.total_cmp(a));
        let (column_groups, energies) =
            shared_rows(mo, closed, [homo.map(|(at, _)| at), lumo.map(|(at, _)| at)]);

        let mut columns: Vec<_> = mo
            .columns
            .iter()
            .zip(column_groups)
            .enumerate()
            .map(|(i, (column, groups))| {
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
                    index: i as u8,
                    column,
                    groups,
                    left: 0.,
                    bar_space: em * (2.5 * slots + 0.22 * (slots - 1.)),
                    label_space,
                    beta: column.label == "β" || (column.label.is_empty() && i == 1),
                    picture_x: 0.,
                }
            })
            .collect();

        let (ticks, unit) = axis_ticks(energies.iter().copied());
        let tick_labels: Vec<_> = ticks
            .iter()
            .map(|&tick| tick_text((tick * unit).clamp(-f64::MAX, f64::MAX)))
            .collect();
        let axis = em * 1.25
            + tick_labels
                .iter()
                .map(|label| width_of(label, tick_size))
                .fold(em, f32::max);
        let gap = homo
            .zip(lumo)
            .map(|((hi, he), (li, le))| (hi, li, format!("gap {} eV", signed(le - he))));
        let bracket = axis + em + gap.as_ref().map_or(0., |(_, _, text)| width_of(text, em));
        let mut width = bracket + em;
        let picture_side = style.bond_length_world * 2.2;
        let mut picture_count = 0;
        for c in &mut columns {
            c.left = width;
            width += c.bar_space + c.label_space + em * 1.6;
            let count = mo
                .pictures
                .iter()
                .filter(|p| {
                    p.column == c.index
                        && c.groups
                            .iter()
                            .any(|g| g.levels.contains(&(p.figure.orbital.index as usize)))
                })
                .count();
            if count > 0 {
                // Each spin's pictures occupy the lane immediately after its
                // labels, before the next spin's bars and labels.
                c.picture_x = width + picture_side / 2.;
                width += picture_side + em;
                picture_count = picture_count.max(count);
            }
        }

        let spacing = em * 1.45;
        let top = em * 2.5;
        let gaps = energies.len().saturating_sub(1);
        let plot_height = (gaps as f32 * spacing).max(em * 6.);
        let pictures_height = picture_count as f32 * (picture_side + em * 0.25);
        let height = (top + plot_height + em * 1.4).max(pictures_height + em * 2.);
        let bottom = height - em * 1.4;
        let low = ticks.first().copied().unwrap_or(-1.);
        let high = ticks.last().copied().unwrap_or(1.);
        let y = |energy| top + (bottom - top) * ((high - energy) / (high - low)) as f32;
        let positions: Vec<_> = energies
            .into_iter()
            .enumerate()
            .scan(top - spacing, |previous, (row, energy)| {
                *previous = spaced(y(energy / unit), *previous, spacing, bottom, gaps - row);
                Some(*previous)
            })
            .collect();
        for group in columns.iter_mut().flat_map(|c| &mut c.groups) {
            if let Some(&position) = positions.get(group.row) {
                group.y = position;
            }
        }

        let mut labels = vec![];
        let mut label = |text: String, x, y, size, orbital| {
            let index = labels.len();
            labels.push(Label {
                text,
                left: Point::new(x, y),
                size,
                orbital,
            });
            index
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
        let step = picture_side + em * 0.25;
        for c in &columns {
            let first = pictures.len();
            if !closed {
                let header = if c.beta { "β" } else { "α" };
                label(header.into(), c.left, em, em, None);
            }
            for g in &c.groups {
                let right = c.bars(g, em).last().map_or(c.left, |(_, _, right)| right);
                let label_x = right + em * 0.35;
                let mut label_end = label_x;
                let mut last_label = None;
                for (index, text) in &g.labels {
                    last_label = Some(label(
                        text.clone(),
                        label_end,
                        g.y,
                        em,
                        Some((c.index, *index)),
                    ));
                    label_end += width_of(text, em) + em * 0.35;
                }
                for (index, p) in mo.pictures.iter().enumerate() {
                    if p.column == c.index && g.levels.contains(&(p.figure.orbital.index as usize))
                    {
                        pictures.push(PictureSlot {
                            index,
                            centre: Point::new(c.picture_x, g.y),
                            connector: Point::new(label_end + em * 0.28, g.y),
                            label_width: label_end - label_x,
                            label: last_label,
                        });
                    }
                }
            }
            let count = pictures.len() - first;
            let mut previous = em * 0.5 + picture_side / 2. - step;
            for (i, picture) in pictures.iter_mut().skip(first).enumerate() {
                picture.centre.y = spaced(
                    picture.centre.y,
                    previous,
                    step,
                    height - em * 0.5 - picture_side / 2.,
                    count - 1 - i,
                );
                previous = picture.centre.y;
            }
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

    fn text_scale(&self, g: &Graphic) -> f32 {
        self.x_scale(g)
            .min(g.axis_y.x.hypot(g.axis_y.y) / self.height)
    }

    /// Fit an upright square in its local slot, including after an affine turn.
    fn picture_world_side(&self, g: &Graphic) -> f32 {
        let span = |axis: Point| f64::from(axis.x.abs()) + f64::from(axis.y.abs());
        let x = f64::from(self.width) * span(g.axis_y);
        let y = f64::from(self.height) * span(g.axis_x);
        (f64::from(self.picture_side) * f64::from(g.frame_determinant().abs()) / x.max(y)) as f32
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
                    size_pt: label.size * style::DEFAULT.points_per_world() * self.text_scale(g),
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
        if !self.closed && (label.orbital.is_some() || matches!(label.text.as_str(), "α" | "β")) {
            let width = style::styled_text_width(&label.text, label.size, &self.font);
            let (lo, hi) = self.annotation_x_bounds(g, &a);
            let tolerance = self.font.size() * 1e-4;
            if lo < label.left.x - tolerance || hi > label.left.x + width + tolerance {
                // Readable text can follow axis_y after a reflection. Fit
                // the rendered box within this column's label slot.
                let fit = (width / (hi - lo)).min(1.);
                a.format.style.size_pt *= fit;
                let (w, h) = a.size();
                let centre = self.world(g, label.left.offset(width / 2., 0.));
                a.position = centre.offset(-w / 2., -h / 2.);
            }
        }
        a
    }

    fn annotation_x_bounds(&self, g: &Graphic, a: &Annotation) -> (f32, f32) {
        let det = g.frame_determinant();
        a.corners()
            .into_iter()
            .map(|p| g.world_to_local(p, det).x * self.width)
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), x| {
                (lo.min(x), hi.max(x))
            })
    }

    fn picture_connector(
        &self,
        g: &Graphic,
        slot: &PictureSlot,
        labels: &[Annotation],
        side: f32,
    ) -> Option<[Point; 2]> {
        let start = if self.closed {
            let angle = text_angle(g.axis_x, g.axis_y).to_radians();
            let (sin, cos) = angle.sin_cos();
            let extension = slot.label_width * (self.text_scale(g) - self.x_scale(g));
            self.world(g, slot.connector)
                .offset(cos * extension, sin * extension)
        } else {
            let x = slot
                .label
                .and_then(|index| labels.get(index))
                .map_or(slot.connector.x, |a| {
                    self.annotation_x_bounds(g, a).1 + self.font.size() * 0.63
                });
            self.world(g, Point::new(x, slot.connector.y))
        };
        let centre = self.world(g, slot.centre);
        let half = side / 2.;
        let span = g.axis_x.x.abs().max(g.axis_x.y.abs());
        let left = centre.offset(-g.axis_x.x / span * half, -g.axis_x.y / span * half);
        let forward = if self.closed {
            (left.x - start.x) * g.axis_x.x + (left.y - start.y) * g.axis_x.y > 0.
        } else {
            // The clear lane belongs to the local column, including when a
            // shear makes the picture's vertical displacement project onto x.
            let det = g.frame_determinant();
            g.world_to_local(left, det).x > g.world_to_local(start, det).x
        };
        forward.then_some([start, left])
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
                    let mut bar = line_part(
                        g,
                        path(&[Point::new(left, y), Point::new(right, y)]),
                        LinePattern::Solid,
                        1.,
                    );
                    bar.style.flat_caps = true;
                    parts.push(bar);
                    let n = c.column.occupations.get(i).copied().unwrap_or_else(|| {
                        if c.column.occupied.is_some_and(|n| i < n) {
                            if self.closed { 2. } else { 1. }
                        } else {
                            0.
                        }
                    });
                    if n >= 0.5 {
                        // Electrons cross the bar, centred on it: a pair as ↑ in
                        // the left half and ↓ in the right, one electron at the
                        // centre with its column's spin.
                        let pair = self.closed && n >= 1.5;
                        let width = right - left;
                        let centre = (left + right) / 2.;
                        let apart = width * 0.2;
                        let half = em * 0.38;
                        let head = (width * 0.09).min(em * 0.2);
                        let arrows = if pair {
                            vec![(centre - apart, false), (centre + apart, true)]
                        } else {
                            vec![(centre, c.beta)]
                        };
                        for (x, down) in arrows {
                            let (tail, tip) = if down {
                                (y - half, y + half)
                            } else {
                                (y + half, y - half)
                            };
                            let back = tip + head * 1.4 * if down { -1. } else { 1. };
                            for points in [
                                vec![Point::new(x, tail), Point::new(x, tip)],
                                vec![
                                    Point::new(x - head, back),
                                    Point::new(x, tip),
                                    Point::new(x + head, back),
                                ],
                            ] {
                                parts.push(line_part(g, path(&points), LinePattern::Solid, 1.));
                            }
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

fn shown_levels(c: &Column, below: u8, above: u8) -> Range<usize> {
    let Some(last) = c.energies_ev.len().checked_sub(1) else {
        return 0..0;
    };
    let (first, end) = c.occupied.map_or((0, 4.min(last)), |n| {
        (
            n.saturating_sub(1 + usize::from(below)).min(last),
            n.saturating_add(usize::from(above)).min(last),
        )
    });
    *c.group(first).start()..c.group(end).end() + 1
}

/// Find degeneracy within each column before aligning groups across columns.
fn shared_rows(
    mo: &MoDiagram,
    closed: bool,
    frontiers: [Option<Location>; 2],
) -> (Vec<Vec<Group>>, Vec<f64>) {
    let mut groups = vec![];
    for (column, c) in mo.columns.iter().enumerate() {
        let shown = shown_levels(c, mo.below, mo.above);
        let mut end = shown.end;
        while end > shown.start {
            let levels = c.group(end - 1);
            end = *levels.start();
            let frontier = frontiers
                .into_iter()
                .flatten()
                .find_map(|(i, index)| (i == column && levels.contains(&index)).then_some(index));
            if let Some(group) = group(c, levels, frontier, closed) {
                groups.push((column, group, frontier.is_some()));
            }
        }
    }
    groups.sort_by(|a, b| b.1.energy.total_cmp(&a.1.energy));
    let mut groups = groups.into_iter().peekable();
    let mut columns: Vec<Vec<Group>> = mo.columns.iter().map(|_| vec![]).collect();
    let mut energies = vec![];
    while let Some((column, mut group, frontier)) = groups.next() {
        let Some(column_groups) = columns.get_mut(column) else {
            continue;
        };
        let row = energies.len();
        let mut energy = group.energy;
        group.row = row;
        column_groups.push(group);
        // A nonzero gap needs separate rows even if its endpoints are close.
        if let Some((other, mut next, _)) = groups.next_if(|(other, next, next_frontier)| {
            *other != column
                && energy - next.energy < DEGENERATE_EV
                && (!frontier || !*next_frontier || energy == next.energy)
        }) && let Some(column_groups) = columns.get_mut(other)
        {
            next.row = row;
            energy = next.energy;
            column_groups.push(next);
        }
        energies.push(energy);
    }
    (columns, energies)
}

fn group(
    c: &Column,
    levels: RangeInclusive<usize>,
    frontier: Option<usize>,
    closed: bool,
) -> Option<Group> {
    let mut labelled: Vec<_> = [c.homo(), c.lumo()]
        .into_iter()
        .flatten()
        .filter(|i| levels.contains(i))
        .collect();
    // A group is named after its level nearest the gap.
    let nearest_gap = match c.homo() {
        Some(homo) if *levels.end() <= homo => *levels.end(),
        _ => *levels.start(),
    };
    let label_index = labelled.first().copied().unwrap_or(nearest_gap);
    let energy = *c.energies_ev.get(frontier.unwrap_or(label_index))?;
    if closed && labelled.is_empty() {
        labelled.push(label_index);
    }
    let count = level_count(&levels);
    let labels = labelled
        .into_iter()
        .enumerate()
        .filter_map(|(i, index)| {
            let mut text = format!(
                "{}  {} eV",
                c.name(index),
                signed(*c.energies_ev.get(index)?)
            );
            if i == 0 && count > 1 {
                text.push_str(&format!(" · {count}-fold"));
            }
            Some((index, text))
        })
        .collect();
    Some(Group {
        levels,
        energy,
        row: 0,
        y: 0.,
        labels,
    })
}

fn level_count(levels: &RangeInclusive<usize>) -> usize {
    levels.end() - levels.start() + 1
}

type Location = (usize, usize);

fn frontier(
    columns: &[Column],
    index: impl Fn(&Column) -> Option<usize>,
    order: impl Fn(&f64, &f64) -> std::cmp::Ordering,
) -> Option<(Location, f64)> {
    columns
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let level = index(c)?;
            Some(((i, level), *c.energies_ev.get(level)?))
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
    // No "−0.00" for a value that rounds to zero.
    let value = if value.abs() < 0.005 { 0. } else { value };
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
