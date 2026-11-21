//! TLC plate Properties, with explicit lane/spot targets and one edit per Apply.
use super::{App, Message, document_tab::TabId};
use reshiki::{
    document::Point,
    graphics::{Graphic, TlcLane, TlcRfLabel},
    palette::Color,
    typography::{Script, TextFormat, TextSpan, TextStyle},
};
#[cfg(test)]
mod tests;
mod view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    tab: TabId,
    epoch: u64,
    revision: u64,
    graphic: u64,
    lane: usize,
    spot: Option<usize>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Origin,
    Front,
    Line,
    PlateColor,
    Rf,
    Width,
    Height,
    SpotColor,
}
impl Field {
    fn index(self) -> usize {
        self as usize
    }
    fn id(self) -> &'static str {
        match self {
            Self::Origin => "tlc-origin",
            Self::Front => "tlc-front",
            Self::Line => "tlc-line",
            Self::PlateColor => "tlc-plate-color",
            Self::Rf => "tlc-rf",
            Self::Width => "tlc-width",
            Self::Height => "tlc-height",
            Self::SpotColor => "tlc-spot-color",
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum Switch {
    Borders,
    Origin,
    Front,
    Filled,
    Rf,
}
#[derive(Debug, Clone, Copy)]
pub enum PaintScope {
    Plate,
    Spot,
}
#[derive(Debug, Clone)]
pub enum Action {
    Draft(Target, Field, String),
    Details(Target, bool),
    Apply(Target, Field),
    Lane(Target, usize),
    Spot(Target, usize),
    AddLane(Target),
    DeleteLane(Target),
    AddSpot(Target),
    DeleteSpot(Target),
    Toggle(Target, Switch, bool),
    Paint(Target, PaintScope, Color),
}
#[derive(Default)]
pub(super) struct State {
    target: Option<Target>,
    fields: [String; 8],
    plate_settings: bool,
}
impl State {
    fn input(&self, field: Field) -> &str {
        self.fields.get(field.index()).map_or("", String::as_str)
    }
}

fn number(value: f64) -> String {
    if value != 0. && value.abs() < 0.0001 {
        return value.to_string();
    }
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}
impl App {
    fn selected_tlc(&self) -> Option<&Graphic> {
        let mut graphics = self
            .tab
            .doc
            .graphics
            .iter()
            .filter(|g| self.tab.selected.contains(&g.id));
        let g = graphics.next()?;
        (graphics.next().is_none() && g.tlc.is_some()).then_some(g)
    }
    pub(super) fn tlc_panel_visible(&self) -> bool {
        self.tool == crate::canvas::Tool::Graphic(reshiki::graphics::GraphicKind::TlcPlate)
            || self
                .tab
                .doc
                .graphics
                .iter()
                .any(|g| self.tab.selected.contains(&g.id) && g.tlc.is_some())
    }
    pub(super) fn sync_tlc(&mut self) {
        let Some(g) = self.selected_tlc() else {
            self.tab.tlc = State::default();
            return;
        };
        let Some(t) = g.tlc.as_deref() else { return };
        let old = self.tab.tlc.target;
        let keep = old.filter(|k| {
            k.tab == self.tab.id && k.epoch == self.tab.file_epoch && k.graphic == g.id
        });
        let lane = keep
            .map_or(0, |k| k.lane)
            .min(t.lanes.len().saturating_sub(1));
        let spots = t.lanes.get(lane).map_or(0, |l| l.spots.len());
        let spot = (spots > 0).then(|| {
            keep.and_then(|k| k.spot)
                .unwrap_or(0)
                .min(spots.saturating_sub(1))
        });
        let target = Target {
            tab: self.tab.id,
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            graphic: g.id,
            lane,
            spot,
        };
        if old == Some(target) {
            return;
        }
        let s = t
            .lanes
            .get(lane)
            .and_then(|l| spot.and_then(|i| l.spots.get(i)));
        // Display compact values; unchanged Apply below keeps the exact native
        // number or semantic color rather than re-parsing the displayed draft.
        let palette = reshiki::palette::Palette::of(&self.tab.doc);
        let fields = [
            number(t.origin_fraction * 100.),
            number(t.solvent_front_fraction * 100.),
            g.style.width_pt.to_string(),
            reshiki::palette::hex(palette.rgb(g.style.stroke)),
            s.map_or_else(String::new, |s| number(s.rf)),
            s.map_or_else(String::new, |s| number(s.width_pt)),
            s.map_or_else(String::new, |s| number(s.height_pt)),
            s.map_or_else(String::new, |s| {
                reshiki::palette::hex(palette.rgb(s.style.stroke))
            }),
        ];
        let plate_settings = keep.is_some() && self.tab.tlc.plate_settings;
        self.tab.tlc = State {
            target: Some(target),
            fields,
            plate_settings,
        };
    }
    fn current_tlc_target(&self, target: Target) -> bool {
        self.tab.tlc.target == Some(target)
            && target.tab == self.tab.id
            && target.epoch == self.tab.file_epoch
            && target.revision == self.tab.revision
            && self.selected_tlc().is_some_and(|g| g.id == target.graphic)
    }
    pub(super) fn tlc_action(&mut self, action: Action) {
        let target = match &action {
            Action::Draft(k, ..)
            | Action::Details(k, ..)
            | Action::Apply(k, ..)
            | Action::Lane(k, ..)
            | Action::Spot(k, ..)
            | Action::AddLane(k)
            | Action::DeleteLane(k)
            | Action::AddSpot(k)
            | Action::DeleteSpot(k)
            | Action::Toggle(k, ..)
            | Action::Paint(k, ..) => *k,
        };
        if !self.current_tlc_target(target) {
            self.sync_tlc();
            return;
        }
        if let Action::Details(_, open) = action {
            self.tab.tlc.plate_settings = open;
            return;
        }
        if let Action::Draft(_, field, value) = action {
            if let Some(input) = self.tab.tlc.fields.get_mut(field.index()) {
                *input = value;
            }
            return;
        }
        if let Action::Lane(_, lane) | Action::Spot(_, lane) = action {
            let choose_spot = matches!(action, Action::Spot(..));
            let Some(g) = self.selected_tlc() else { return };
            let Some(t) = g.tlc.as_deref() else { return };
            let (lane, spot) = if choose_spot {
                if t.lanes
                    .get(target.lane)
                    .is_none_or(|l| lane >= l.spots.len())
                {
                    return;
                }
                (target.lane, Some(lane))
            } else {
                let Some(l) = t.lanes.get(lane) else { return };
                (lane, (!l.spots.is_empty()).then_some(0))
            };
            self.tab.tlc.target = Some(Target {
                lane,
                spot,
                ..target
            });
            // Force a refresh after changing only the editor's part selection.
            self.reload_tlc_inputs();
            return;
        }
        let Some(mut candidate) = self.selected_tlc().cloned() else {
            return;
        };
        let mut part = (target.lane, target.spot);
        let result = self
            .edit_tlc(&mut candidate, target, &action, &mut part)
            .and_then(|_| candidate.validate());
        if let Err(error) = result {
            self.status = error;
            self.error = true;
            return;
        }
        let Some(current) = self.selected_tlc() else {
            return;
        };
        if candidate == *current {
            self.error = false;
            self.status = "TLC plate unchanged".into();
            return;
        }
        let before = self.tab.doc.clone();
        if let Some(g) = self
            .tab
            .doc
            .graphics
            .iter_mut()
            .find(|g| g.id == target.graphic)
        {
            *g = candidate;
        }
        self.tab.tlc.target = Some(Target {
            lane: part.0,
            spot: part.1,
            ..target
        });
        self.error = false;
        self.changed(before);
        self.sync_tlc();
    }
    fn reload_tlc_inputs(&mut self) {
        if let Some(mut k) = self.tab.tlc.target {
            k.revision = k.revision.wrapping_sub(1);
            self.tab.tlc.target = Some(k);
        }
        self.sync_tlc();
    }
    fn edit_tlc(
        &self,
        g: &mut Graphic,
        key: Target,
        action: &Action,
        part: &mut (usize, Option<usize>),
    ) -> Result<(), String> {
        let snapshot = g.clone();
        let t = g.tlc.as_deref_mut().ok_or("Select a TLC plate")?;
        match action {
            Action::Apply(_, field) => {
                let input = self.tab.tlc.input(*field);
                let current_spot = t
                    .lanes
                    .get(key.lane)
                    .and_then(|l| key.spot.and_then(|i| l.spots.get(i)));
                let displayed = match field {
                    Field::Origin => Some(number(t.origin_fraction * 100.)),
                    Field::Front => Some(number(t.solvent_front_fraction * 100.)),
                    Field::Rf => current_spot.map(|s| number(s.rf)),
                    Field::Width => current_spot.map(|s| number(s.width_pt)),
                    Field::Height => current_spot.map(|s| number(s.height_pt)),
                    _ => None,
                };
                if displayed.as_deref() == Some(input) {
                    return Ok(());
                }
                if matches!(field, Field::PlateColor | Field::SpotColor) {
                    let rgb = super::graphics::parse_color(input)
                        .ok_or("Enter a six-digit hex color, such as #117E6C")?;
                    let current_color = if *field == Field::PlateColor {
                        g.style.stroke
                    } else {
                        spot_mut(t, key)?.style.stroke
                    };
                    if reshiki::palette::Palette::of(&self.tab.doc).rgb(current_color) == rgb {
                        return Ok(());
                    }
                    let color = Color::Custom(rgb);
                    if *field == Field::PlateColor {
                        g.style.stroke = color;
                    } else {
                        let s = spot_mut(t, key)?;
                        s.style.stroke = color;
                        if s.style.fill.is_some() {
                            s.style.fill = Some(color);
                        }
                    }
                } else {
                    let value = input
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .ok_or("Enter a finite number")?;
                    match field {
                        Field::Origin | Field::Front => {
                            if !(0. ..=100.).contains(&value) {
                                return Err("Reference line position must be 0–100%".into());
                            }
                            if *field == Field::Origin {
                                t.origin_fraction = value / 100.;
                            } else {
                                t.solvent_front_fraction = value / 100.;
                            }
                        }
                        Field::Line => {
                            if !(0.1..=12.).contains(&value) {
                                return Err("Line width must be 0.1–12 pt".into());
                            }
                            g.style.width_pt = value as f32;
                        }
                        Field::Rf => {
                            spot_mut(t, key)?.set_rf(value)?;
                        }
                        Field::Width | Field::Height => {
                            if !(1. / 65536. ..=32767.).contains(&value) {
                                return Err(
                                    "Spot dimensions must be positive and at most 32767 pt".into(),
                                );
                            }
                            let s = spot_mut(t, key)?;
                            if *field == Field::Width {
                                s.width_pt = value
                            } else {
                                s.height_pt = value
                            }
                        }
                        _ => {}
                    }
                }
            }
            Action::Toggle(_, switch, on) => match switch {
                Switch::Borders => t.show_borders = *on,
                Switch::Origin => t.show_origin = *on,
                Switch::Front => t.show_solvent_front = *on,
                Switch::Filled => {
                    let s = spot_mut(t, key)?;
                    s.style.fill = on.then_some(s.style.stroke);
                }
                Switch::Rf => {
                    let s = spot_mut(t, key)?;
                    if *on && s.label.is_none() {
                        s.label = Some(rf_label(&snapshot, s));
                    }
                    s.show_rf = *on;
                }
            },
            Action::Paint(_, scope, color) => match scope {
                PaintScope::Plate => g.style.stroke = *color,
                PaintScope::Spot => {
                    let s = spot_mut(t, key)?;
                    s.style.stroke = *color;
                    if s.style.fill.is_some() {
                        s.style.fill = Some(*color);
                    }
                }
            },
            Action::AddLane(_) => {
                if t.lanes.len() >= reshiki::graphics::MAX_TLC_LANES {
                    return Err("This plate has reached its lane limit".into());
                }
                t.lanes.push(TlcLane { spots: vec![] });
                *part = (t.lanes.len() - 1, None);
            }
            Action::DeleteLane(_) => {
                if key.lane < t.lanes.len() {
                    t.lanes.remove(key.lane);
                    *part = (key.lane.saturating_sub(1), Some(0));
                }
            }
            Action::AddSpot(_) => {
                if t.lanes.iter().map(|l| l.spots.len()).sum::<usize>()
                    >= reshiki::graphics::MAX_TLC_SPOTS
                {
                    return Err("This plate has reached its spot limit".into());
                }
                let lane = t
                    .lanes
                    .get_mut(key.lane)
                    .ok_or("Add a lane before adding a spot")?;
                let mut positions: Vec<_> =
                    lane.spots.iter().map(|s| s.rf).chain([0., 1.]).collect();
                positions.sort_by(f64::total_cmp);
                let rf = positions
                    .windows(2)
                    .filter_map(|p| match p {
                        [a, b] => Some((b - a, (a + b) / 2.)),
                        _ => None,
                    })
                    .max_by(|a, b| a.0.total_cmp(&b.0))
                    .map_or(0.5, |v| v.1);
                let mut new = lane
                    .spots
                    .last()
                    .cloned()
                    .unwrap_or_else(|| crate::canvas::tlc::spot(&snapshot.style, rf));
                new.rf = rf;
                new.show_rf = false;
                new.label = None;
                lane.spots.push(new);
                *part = (key.lane, Some(lane.spots.len() - 1));
            }
            Action::DeleteSpot(_) => {
                let lane = t.lanes.get_mut(key.lane).ok_or("Choose a lane")?;
                if let Some(i) = key.spot.filter(|i| *i < lane.spots.len()) {
                    lane.spots.remove(i);
                    *part = (
                        key.lane,
                        (!lane.spots.is_empty()).then_some(i.saturating_sub(1)),
                    );
                }
            }
            Action::Draft(..) | Action::Details(..) | Action::Lane(..) | Action::Spot(..) => {}
        }
        Ok(())
    }
}
fn spot_mut(
    t: &mut reshiki::graphics::TlcPlate,
    k: Target,
) -> Result<&mut reshiki::graphics::TlcSpot, String> {
    t.lanes
        .get_mut(k.lane)
        .and_then(|l| k.spot.and_then(|i| l.spots.get_mut(i)))
        .ok_or_else(|| "Choose a spot".into())
}
fn rf_label(g: &Graphic, s: &reshiki::graphics::TlcSpot) -> TlcRfLabel {
    let text = format!("Rf = {:.2}", s.rf);
    let mut format = TextFormat {
        style: TextStyle {
            size_pt: 7.5,
            color: s.style.stroke,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut sub = format.style.clone();
    sub.script = Script::Subscript;
    format.spans.push(TextSpan {
        start: 1,
        end: 2,
        style: sub,
    });
    let layout = reshiki::typography::layout(&text, &format);
    let baseline = layout
        .fragments
        .first()
        .map_or(0., |f| f.position.y + reshiki::style::text_ascent(&f.style));
    let rx = reshiki::style::DEFAULT.world((s.width_pt + f64::from(s.style.width_pt)) as f32) * 0.5;
    let ry =
        reshiki::style::DEFAULT.world((s.height_pt + f64::from(s.style.width_pt)) as f32) * 0.5;
    let (x, y) = g
        .tlc
        .as_deref()
        .map_or((Point::new(1., 0.), Point::new(0., 1.)), |t| t.basis(g));
    let d = Point::new(x.x * rx - y.x * ry, x.y * rx - y.y * ry - baseline);
    let det = x.x * y.y - x.y * y.x;
    TlcRfLabel {
        text,
        format,
        offset: Point::new((d.x * y.y - d.y * y.x) / det, (x.x * d.y - x.y * d.x) / det),
        angle: 0.,
    }
}
