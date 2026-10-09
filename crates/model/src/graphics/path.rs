//! Connected path nodes and their independent Bézier directions.
use super::{BracketSides, Graphic, GraphicKind, GraphicStyle, PathCommand};
use crate::document::Point;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathHandle {
    pub index: usize,
    pub point: Point,
    pub node: bool,
}

#[derive(Clone)]
struct Node {
    point: Point,
    index: usize,
}
#[derive(Clone)]
struct Segment {
    controls: Option<[Point; 2]>,
    indices: Option<[usize; 2]>,
}
struct Path {
    nodes: Vec<Node>,
    segments: Vec<Segment>,
    closed: bool,
    closing_alias: Option<usize>,
}
fn lerp(a: Point, b: Point, t: f32) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}
fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}
impl Path {
    fn read(graphic: &Graphic) -> Option<Self> {
        if !matches!(graphic.kind, GraphicKind::Curve | GraphicKind::Path) {
            return None;
        }
        let commands = graphic.commands();
        let PathCommand::Move(first) = commands.first()? else {
            return None;
        };
        let mut path = Self {
            nodes: vec![Node {
                point: *first,
                index: 0,
            }],
            segments: vec![],
            closed: false,
            closing_alias: None,
        };
        let mut index = 1;
        for command in commands.iter().skip(1) {
            if path.closed {
                return None;
            }
            match *command {
                PathCommand::Line(point) => {
                    path.nodes.push(Node { point, index });
                    path.segments.push(Segment {
                        controls: None,
                        indices: None,
                    });
                    index += 1;
                }
                PathCommand::Cubic(a, b, point) => {
                    path.nodes.push(Node {
                        point,
                        index: index + 2,
                    });
                    path.segments.push(Segment {
                        controls: Some([a, b]),
                        indices: Some([index, index + 1]),
                    });
                    index += 3;
                }
                PathCommand::Close => path.closed = true,
                PathCommand::Move(_) => return None,
            }
        }
        if path.closed {
            if path.nodes.last()?.point == *first && path.nodes.len() > 1 {
                path.closing_alias = path.nodes.pop().map(|n| n.index);
            } else {
                path.segments.push(Segment {
                    controls: None,
                    indices: None,
                });
            }
        }
        (path.nodes.len() >= 2).then_some(path)
    }
    fn node(&self, index: usize) -> Option<usize> {
        self.nodes
            .iter()
            .position(|n| n.index == index)
            .or_else(|| (self.closing_alias == Some(index)).then_some(0))
    }
    fn reindex(&mut self) {
        let Some(first) = self.nodes.first_mut() else {
            return;
        };
        first.index = 0;
        self.closing_alias = None;
        let mut index = 1;
        for (i, segment) in self.segments.iter_mut().enumerate() {
            if segment.controls.is_some() {
                segment.indices = Some([index, index + 1]);
                index += 2;
            } else {
                segment.indices = None;
            }
            let next = (i + 1) % self.nodes.len();
            if next == 0 {
                self.closing_alias = Some(index);
            } else if let Some(node) = self.nodes.get_mut(next) {
                node.index = index;
            }
            index += 1;
        }
    }
    fn commands(&self) -> Vec<PathCommand> {
        let Some(first) = self.nodes.first() else {
            return vec![];
        };
        let mut result = vec![PathCommand::Move(first.point)];
        let endpoints = self.nodes.iter().skip(1).chain(self.nodes.first());
        for (segment, node) in self.segments.iter().zip(endpoints) {
            let end = node.point;
            result.push(match segment.controls {
                Some([a, b]) => PathCommand::Cubic(a, b, end),
                None => PathCommand::Line(end),
            });
        }
        if self.closed {
            result.push(PathCommand::Close);
        }
        result
    }
    fn point(&self, node: usize) -> Result<Point, String> {
        self.nodes
            .get(node)
            .map(|n| n.point)
            .ok_or_else(|| "Invalid path node".into())
    }
    fn index(&self, node: usize) -> Result<usize, String> {
        self.nodes
            .get(node)
            .map(|n| n.index)
            .ok_or_else(|| "Invalid path node".into())
    }
    fn store(self, graphic: &mut Graphic) -> Result<(), String> {
        let commands = self.commands();
        if commands
            .iter()
            .flat_map(PathCommand::iter_points)
            .any(|p| !finite(p))
        {
            return Err("Path coordinates are out of range".into());
        }
        // Depth is expressed in this affine frame. Keep it when editing a
        // projected path, rather than assigning its old depth coefficients to
        // a new world-space frame.
        let determinant = graphic.axis_x.x * graphic.axis_y.y - graphic.axis_x.y * graphic.axis_y.x;
        if determinant.is_finite() && determinant.abs() >= 0.000_001 {
            let local = |p: Point| {
                let delta = Point::new(p.x - graphic.origin.x, p.y - graphic.origin.y);
                Point::new(
                    (delta.x * graphic.axis_y.y - delta.y * graphic.axis_y.x) / determinant,
                    (graphic.axis_x.x * delta.y - graphic.axis_x.y * delta.x) / determinant,
                )
            };
            let commands: Vec<_> = commands.iter().map(|c| c.map(local)).collect();
            if commands
                .iter()
                .flat_map(PathCommand::iter_points)
                .any(|p| !finite(p))
            {
                return Err("Path coordinates are out of range".into());
            }
            graphic.path = commands;
        } else if graphic.depth.iter().skip(1).all(|z| *z == 0.) {
            // A flat line can have a singular drawing frame; its constant
            // depth is also valid after converting its points to world units.
            graphic.path = commands;
            graphic.origin = Point::default();
            graphic.axis_x = Point::new(1., 0.);
            graphic.axis_y = Point::new(0., 1.);
        } else {
            return Err(
                "Rotate this edge-on projected path toward the page before editing it".into(),
            );
        }
        graphic.kind = GraphicKind::Path;
        Ok(())
    }
}
impl Graphic {
    /// Handles for a single connected path. A closed seam has only one node.
    /// Imported compound paths retain the existing generic point editor.
    pub fn path_handles(&self) -> Option<Vec<PathHandle>> {
        let path = Path::read(self)?;
        let mut result: Vec<_> = path
            .nodes
            .iter()
            .map(|n| PathHandle {
                index: n.index,
                point: n.point,
                node: true,
            })
            .collect();
        for segment in &path.segments {
            if let (Some(points), Some(indices)) = (segment.controls, segment.indices) {
                result.extend(
                    points
                        .into_iter()
                        .zip(indices)
                        .map(|(point, index)| PathHandle {
                            index,
                            point,
                            node: false,
                        }),
                );
            }
        }
        result.sort_by_key(|h| h.index);
        Some(result)
    }
    pub fn path_closed(&self) -> bool {
        self.commands()
            .iter()
            .any(|c| matches!(c, PathCommand::Close))
    }
    pub(super) fn edit_path_point(&mut self, index: usize, point: Point) -> bool {
        let Some(mut path) = Path::read(self) else {
            return false;
        };
        if !finite(point) {
            return true;
        }
        if !self.path_handles().is_some_and(|handles| {
            handles.iter().any(|handle| {
                (handle.index == index || path.closing_alias == Some(index) && handle.index == 0)
                    && handle.point != point
            })
        }) {
            return true;
        }
        if let Some(node) = path.node(index) {
            let Ok(old) = path.point(node) else {
                return true;
            };
            let move_control = |p: Point| p.offset(point.x - old.x, point.y - old.y);
            if let Some(segment) = path.segments.get_mut(node)
                && let Some([a, _]) = segment.controls.as_mut()
            {
                *a = move_control(*a);
            }
            let previous = if node > 0 {
                Some(node - 1)
            } else if path.closed {
                path.segments.len().checked_sub(1)
            } else {
                None
            };
            if let Some(previous) = previous
                && let Some(segment) = path.segments.get_mut(previous)
                && let Some([_, b]) = segment.controls.as_mut()
            {
                *b = move_control(*b);
            }
            if let Some(node) = path.nodes.get_mut(node) {
                node.point = point;
            }
        } else {
            for segment in &mut path.segments {
                if let (Some([first, second]), Some([a, b])) =
                    (segment.indices, segment.controls.as_mut())
                {
                    if first == index {
                        *a = point;
                    }
                    if second == index {
                        *b = point;
                    }
                }
            }
        }
        let _ = path.store(self);
        true
    }
    pub fn insert_path_node(&mut self, index: usize) -> Result<usize, String> {
        let mut path = Path::read(self).ok_or("Select one continuous path")?;
        let node = path.node(index).ok_or("Select a round node")?;
        let segment = path
            .segments
            .get(node)
            .ok_or("The last open node has no following segment")?
            .clone();
        let a = path.point(node)?;
        let b = path.point((node + 1) % path.nodes.len())?;
        let (point, left, right) = if let Some([c, d]) = segment.controls {
            let ac = lerp(a, c, 0.5);
            let cd = lerp(c, d, 0.5);
            let db = lerp(d, b, 0.5);
            let l = lerp(ac, cd, 0.5);
            let r = lerp(cd, db, 0.5);
            (lerp(l, r, 0.5), Some([ac, l]), Some([r, db]))
        } else {
            (lerp(a, b, 0.5), None, None)
        };
        path.nodes.insert(node + 1, Node { point, index: 0 });
        path.segments
            .get_mut(node)
            .ok_or("Invalid path segment")?
            .controls = left;
        path.segments.insert(
            node + 1,
            Segment {
                controls: right,
                indices: None,
            },
        );
        path.reindex();
        let index = path.index(node + 1)?;
        path.store(self)?;
        Ok(index)
    }
    pub fn delete_path_node(&mut self, index: usize) -> Result<usize, String> {
        let mut path = Path::read(self).ok_or("Select one continuous path")?;
        let node = path.node(index).ok_or("Select a round node")?;
        if path.nodes.len() <= if path.closed { 3 } else { 2 } {
            return Err("Keep at least two open nodes or three closed nodes".into());
        }
        if !path.closed && node == 0 {
            path.nodes.remove(0);
            path.segments.remove(0);
        } else if !path.closed && node + 1 == path.nodes.len() {
            path.nodes.pop();
            path.segments.pop();
        } else {
            let previous = (node + path.nodes.len() - 1) % path.nodes.len();
            let next = (node + 1) % path.nodes.len();
            let a = path.point(previous)?;
            let b = path.point(next)?;
            let removed = path.point(node)?;
            let left = path
                .segments
                .get(previous)
                .ok_or("Invalid path segment")?
                .controls;
            let right = path
                .segments
                .get(node)
                .ok_or("Invalid path segment")?
                .controls;
            let controls = (left.is_some() || right.is_some()).then(|| {
                [
                    left.map(|[a, _]| a)
                        .unwrap_or_else(|| lerp(a, removed, 1. / 3.)),
                    right
                        .map(|[_, b]| b)
                        .unwrap_or_else(|| lerp(b, removed, 1. / 3.)),
                ]
            });
            path.segments
                .get_mut(previous)
                .ok_or("Invalid path segment")?
                .controls = controls;
            path.segments.remove(node);
            path.nodes.remove(node);
        }
        path.reindex();
        let index = path.index(node.min(path.nodes.len() - 1))?;
        path.store(self)?;
        Ok(index)
    }
    pub fn set_path_segment_curved(&mut self, index: usize, curved: bool) -> Result<usize, String> {
        let mut path = Path::read(self).ok_or("Select one continuous path")?;
        let node = path.node(index).ok_or("Select a round node")?;
        let a = path.point(node)?;
        let b = path.point((node + 1) % path.nodes.len())?;
        let segment = path
            .segments
            .get_mut(node)
            .ok_or("The last open node has no following segment")?;
        if curved == segment.controls.is_some() {
            return path.index(node);
        }
        if curved {
            let bend = Point::new(-(b.y - a.y) * 0.25, (b.x - a.x) * 0.25);
            segment.controls = Some([
                lerp(a, b, 1. / 3.).offset(bend.x, bend.y),
                lerp(a, b, 2. / 3.).offset(bend.x, bend.y),
            ]);
        } else {
            segment.controls = None;
        }
        path.reindex();
        let index = path.index(node)?;
        path.store(self)?;
        Ok(index)
    }
    pub fn set_path_closed(&mut self, closed: bool) -> Result<(), String> {
        let mut path = Path::read(self).ok_or("Select one continuous path")?;
        if path.closed == closed {
            return Ok(());
        }
        if closed {
            if path.nodes.len() < 3 {
                return Err("A closed path needs at least three nodes".into());
            }
            if path.nodes.last().map(|n| n.point) == path.nodes.first().map(|n| n.point) {
                path.nodes.pop();
                if path.nodes.len() < 3 {
                    return Err("A closed path needs at least three distinct nodes".into());
                }
            } else {
                path.segments.push(Segment {
                    controls: None,
                    indices: None,
                });
            }
        } else {
            path.segments.pop();
        }
        path.closed = closed;
        path.store(self)
    }
    pub fn pen_curve(id: u64, start: Point, end: Point, style: GraphicStyle) -> Self {
        let mut graphic = Self::dragged(
            id,
            GraphicKind::Curve,
            start,
            end,
            style,
            BracketSides::Both,
            false,
        );
        graphic.path = graphic.commands();
        graphic.kind = GraphicKind::Path;
        graphic.origin = Point::default();
        graphic.axis_x = Point::new(1., 0.);
        graphic.axis_y = Point::new(0., 1.);
        graphic
    }
    /// A click appends a line. Dragging a new node establishes its outgoing
    /// direction; the incoming direction is its reflection about the node.
    pub fn append_pen_node(
        &mut self,
        point: Point,
        direction: Option<Point>,
    ) -> Result<usize, String> {
        let mut path = Path::read(self).ok_or("Select one continuous path")?;
        if path.closed {
            return Err("Open this path before adding nodes".into());
        }
        if !finite(point) || direction.is_some_and(|p| !finite(p)) {
            return Err("Non-finite path point".into());
        }
        let last_node = path.nodes.last().ok_or("Invalid path node")?;
        let last = last_node.point;
        if point.distance(last) < 0.0001 {
            return Ok(last_node.index);
        }
        let controls = direction.map(|direction| {
            [
                path.segments
                    .last()
                    .and_then(|s| s.controls)
                    .map(|[_, incoming]| {
                        Point::new(2. * last.x - incoming.x, 2. * last.y - incoming.y)
                    })
                    .unwrap_or_else(|| lerp(last, point, 1. / 3.)),
                Point::new(2. * point.x - direction.x, 2. * point.y - direction.y),
            ]
        });
        path.nodes.push(Node { point, index: 0 });
        path.segments.push(Segment {
            controls,
            indices: None,
        });
        path.reindex();
        let index = path.nodes.last().ok_or("Invalid path node")?.index;
        path.store(self)?;
        Ok(index)
    }
}
