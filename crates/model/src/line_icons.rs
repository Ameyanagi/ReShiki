//! Line icons placed with the Icons tool. Each icon keeps its own drawing,
//! so a file shows it the same without the icon library.
use crate::{
    document::Point,
    graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, PathCommand},
    scientific::Part,
};
use serde::{Deserialize, Serialize};

/// The icon grid: 24 units a side, y down, as Tabler draws it.
pub const GRID: f32 = 24.;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Icon {
    /// The library's name for the icon, as "flask-2".
    pub name: String,
    pub paths: Vec<IconPath>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IconPath {
    /// On the grid.
    pub commands: Vec<PathCommand>,
    /// A dot or wedge painted in the line color.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub filled: bool,
    /// Whether the path also takes the icon's line width. Older saved icons
    /// stroked every path, including filled details.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub stroked: bool,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

impl Icon {
    /// Reads the library's form: SVG path data joined by `|`, a leading `*`
    /// marking a filled path, followed by `!` when the path has no stroke.
    pub fn parse(name: &str, data: &str) -> Result<Self, String> {
        use crate::equipment::geometry::{Seg, parse_path};
        let point = |[x, y]: [f64; 2]| Point::new(x as f32, y as f32);
        let paths = data
            .split('|')
            .map(|d| {
                let (filled, d) = d.strip_prefix('*').map_or((false, d), |d| (true, d));
                let (stroked, d) = d.strip_prefix('!').map_or((true, d), |d| (false, d));
                let commands = parse_path(d)
                    .map_err(|e| format!("Icon {name}: {e}"))?
                    .into_iter()
                    .map(|s| match s {
                        Seg::Move(p) => PathCommand::Move(point(p)),
                        Seg::Line(p) => PathCommand::Line(point(p)),
                        Seg::Cubic(a, b, c) => PathCommand::Cubic(point(a), point(b), point(c)),
                        Seg::Close => PathCommand::Close,
                    })
                    .collect();
                Ok(IconPath {
                    commands,
                    filled,
                    stroked,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            name: name.to_owned(),
            paths,
        })
    }

    /// Bounds an icon read from a file: the library's largest has 21 paths
    /// and a few hundred commands.
    pub fn validate(&self) -> Result<(), String> {
        let on_grid = |p: Point| p.is_finite() && p.x.abs().max(p.y.abs()) <= 2. * GRID;
        let commands: usize = self.paths.iter().map(|p| p.commands.len()).sum();
        if self.name.len() > 100 || self.paths.len() > 64 || commands > 2000 {
            return Err(format!("Icon {} is larger than an icon can be", self.name));
        }
        if self.paths.is_empty()
            || self.paths.iter().any(|path| {
                !matches!(path.commands.first(), Some(PathCommand::Move(_)))
                    || path
                        .commands
                        .iter()
                        .flat_map(PathCommand::iter_points)
                        .any(|p| !on_grid(p))
            })
        {
            return Err(format!("Icon {} has no drawing on its grid", self.name));
        }
        Ok(())
    }
}

/// The icon in its unit frame: a disc in the fill, the paper when placed,
/// so an icon on an arrow covers the shaft; then its lines.
pub fn parts(icon: &Icon, style: &GraphicStyle) -> Vec<Part> {
    let unit = |p: Point| Point::new(p.x / GRID - 0.5, p.y / GRID - 0.5);
    let mut out: Vec<Part> = crate::scientific::disc(style, 0.55).into_iter().collect();
    out.extend(icon.paths.iter().map(|path| Part {
        commands: path.commands.iter().map(|c| c.map(unit)).collect(),
        style: GraphicStyle {
            fill: path.filled.then_some(style.stroke),
            width_pt: if path.stroked { style.width_pt } else { 0. },
            ..style.clone()
        },
        filled: path.filled,
    }));
    out
}

impl Graphic {
    /// An icon centred on `at`, its grid `size` across, in front of the
    /// structures and on a paper disc.
    pub fn line_icon(id: u64, icon: Icon, at: Point, size: f32, style: GraphicStyle) -> Self {
        let mut g = Self::dragged(
            id,
            GraphicKind::Icon,
            at,
            at,
            style,
            BracketSides::Both,
            false,
        );
        g.axis_x = Point::new(size, 0.);
        g.axis_y = Point::new(0., size);
        g.icon = Some(Box::new(icon));
        g
    }
}

#[cfg(test)]
mod tests;
