//! Connected path nodes and their editable Bézier constraints.
use super::{BracketSides, Graphic, GraphicKind, GraphicStyle, PathCommand};
use crate::document::Point;
use serde::{Deserialize, Serialize};

mod smooth;
mod symmetry;

pub const MIN_OPEN_NODES: usize = 2;
pub const MIN_CLOSED_NODES: usize = 3;

/// Ordered from least to most constrained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathNodeType {
    Corner,
    Smooth,
    Symmetric,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathHandle {
    pub index: usize,
    pub point: Point,
    pub node: bool,
}

/// A connected path's handles and the node rules shared by its editors.
#[derive(Debug, Clone, PartialEq)]
pub struct PathSummary {
    pub handles: Vec<PathHandle>,
    pub closed: bool,
}
impl PathSummary {
    /// Nodes in path order, with the closing seam represented only once.
    pub fn nodes(&self) -> impl DoubleEndedIterator<Item = &PathHandle> {
        self.handles.iter().filter(|handle| handle.node)
    }
    pub fn node_count(&self) -> usize {
        self.nodes().count()
    }
    pub fn can_close(&self) -> bool {
        self.node_count() >= MIN_CLOSED_NODES
    }
    pub fn can_delete_node(&self) -> bool {
        self.node_count()
            > if self.closed {
                MIN_CLOSED_NODES
            } else {
                MIN_OPEN_NODES
            }
    }
    /// Whether a zero-based node ordinal has a following segment.
    pub fn has_following(&self, ordinal: usize) -> bool {
        self.closed || ordinal.saturating_add(1) < self.node_count()
    }
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
    node_types: Vec<PathNodeType>,
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
            node_types: vec![],
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
        if path.nodes.len() < MIN_OPEN_NODES {
            return None;
        }
        path.node_types = if graphic.node_types.len() == path.nodes.len() {
            graphic.node_types.clone()
        } else {
            (0..path.nodes.len())
                .map(|node| path.detect_node_type(node))
                .collect()
        };
        Some(path)
    }
    fn node(&self, index: usize) -> Option<usize> {
        self.nodes
            .iter()
            .position(|n| n.index == index)
            .or_else(|| (self.closing_alias == Some(index)).then_some(0))
    }
    fn owner(&self, index: usize) -> Option<usize> {
        self.node(index).or_else(|| {
            self.segments
                .iter()
                .enumerate()
                .find_map(|(node, segment)| {
                    let [outgoing, incoming] = segment.indices?;
                    if outgoing == index {
                        Some(node)
                    } else if incoming == index {
                        Some((node + 1) % self.nodes.len())
                    } else {
                        None
                    }
                })
        })
    }
    fn previous_segment(&self, node: usize) -> Option<usize> {
        if node > 0 {
            Some(node - 1)
        } else if self.closed {
            self.segments.len().checked_sub(1)
        } else {
            None
        }
    }
    /// Incoming control, node and outgoing control, when both adjacent
    /// segments are cubic.
    fn controls(&self, node: usize) -> Option<[Point; 3]> {
        let incoming = self.segments.get(self.previous_segment(node)?)?.controls?[1];
        let outgoing = self.segments.get(node)?.controls?[0];
        Some([incoming, self.nodes.get(node)?.point, outgoing])
    }
    fn set_controls(&mut self, node: usize, [incoming, outgoing]: [Point; 2]) {
        if let Some(previous) = self.previous_segment(node)
            && let Some(controls) = self
                .segments
                .get_mut(previous)
                .and_then(|segment| segment.controls.as_mut())
        {
            controls[1] = incoming;
        }
        if let Some(segment) = self.segments.get_mut(node)
            && let Some(controls) = segment.controls.as_mut()
        {
            controls[0] = outgoing;
        }
    }
    fn detect_node_type(&self, node: usize) -> PathNodeType {
        let Some([incoming, anchor, outgoing]) = self.controls(node) else {
            return PathNodeType::Corner;
        };
        let a = vector(anchor, outgoing);
        let b = vector(incoming, anchor);
        let (Some((a, out_length)), Some((b, in_length))) = (unit(a), unit(b)) else {
            return PathNodeType::Corner;
        };
        if (a.0 * b.0 + a.1 * b.1) < 1_f64.to_radians().cos() {
            PathNodeType::Corner
        } else if (out_length - in_length).abs() <= out_length.max(in_length) * 0.01 {
            PathNodeType::Symmetric
        } else {
            PathNodeType::Smooth
        }
    }
    fn align_node(&mut self, node: usize, kind: PathNodeType) {
        if kind == PathNodeType::Corner {
            return;
        }
        let Some([incoming, anchor, outgoing]) = self.controls(node) else {
            return;
        };
        let outgoing_vector = unit(vector(anchor, outgoing));
        let incoming_vector = unit(vector(incoming, anchor));
        let out_length = outgoing_vector.map_or(0., |(_, length)| length);
        let in_length = incoming_vector.map_or(0., |(_, length)| length);
        // Average the two tangent directions. Opposed or collapsed handles
        // use the outgoing direction, then the reversed incoming direction.
        let direction = outgoing_vector
            .zip(incoming_vector)
            .and_then(|((a, _), (b, _))| unit((a.0 + b.0, a.1 + b.1)))
            .map(|(direction, _)| direction)
            .or_else(|| outgoing_vector.map(|(direction, _)| direction))
            .or_else(|| incoming_vector.map(|(direction, _)| direction));
        let Some(direction) = direction else {
            return;
        };
        let (out_length, in_length) = if kind == PathNodeType::Symmetric {
            let average = (out_length + in_length) * 0.5;
            (average, average)
        } else {
            (out_length, in_length)
        };
        self.set_controls(
            node,
            [
                along(anchor, direction, -in_length),
                along(anchor, direction, out_length),
            ],
        );
    }
    fn handle_point(&self, index: usize) -> Option<Point> {
        self.node(index)
            .and_then(|node| self.nodes.get(node))
            .map(|node| node.point)
            .or_else(|| {
                self.segments.iter().find_map(|segment| {
                    let (Some([first, second]), Some([a, b])) = (segment.indices, segment.controls)
                    else {
                        return None;
                    };
                    if index == first {
                        Some(a)
                    } else if index == second {
                        Some(b)
                    } else {
                        None
                    }
                })
            })
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
    fn store(mut self, graphic: &mut Graphic) -> Result<(), String> {
        // Edits store every node's type, relaxing a constraint whose handles
        // no longer satisfy it (a split or newly curved neighbour).
        for node in 0..self.nodes.len() {
            if self.controls(node).is_some() {
                let detected = self.detect_node_type(node);
                if let Some(kind) = self.node_types.get_mut(node) {
                    *kind = (*kind).min(detected);
                }
            }
        }
        let commands = self.commands();
        if commands
            .iter()
            .flat_map(PathCommand::iter_points)
            .any(|p| !p.is_finite())
        {
            return Err("Path coordinates are out of range".into());
        }
        // Depth is expressed in this affine frame. Keep it when editing a
        // projected path, rather than assigning its old depth coefficients to
        // a new world-space frame.
        let determinant = graphic.frame_determinant();
        if determinant.is_finite() && determinant.abs() >= 0.000_001 {
            let local = |p: Point| graphic.world_to_local(p, determinant);
            let commands: Vec<_> = commands.iter().map(|c| c.map(local)).collect();
            if commands
                .iter()
                .flat_map(PathCommand::iter_points)
                .any(|p| !p.is_finite())
            {
                return Err("Path coordinates are out of range".into());
            }
            graphic.path = commands;
        } else if graphic.depth.iter().skip(1).all(|z| *z == 0.) {
            // A flat line can have a singular drawing frame; its constant
            // depth is also valid after converting its points to world units.
            graphic.path = commands;
            graphic.reset_frame();
        } else {
            return Err(
                "Rotate this edge-on projected path toward the page before editing it".into(),
            );
        }
        graphic.kind = GraphicKind::Path;
        graphic.node_types = self.node_types;
        Ok(())
    }
}

fn vector(from: Point, to: Point) -> (f64, f64) {
    (
        f64::from(to.x) - f64::from(from.x),
        f64::from(to.y) - f64::from(from.y),
    )
}
fn unit(vector: (f64, f64)) -> Option<((f64, f64), f64)> {
    let length = vector.0.hypot(vector.1);
    (length > 1e-9).then(|| ((vector.0 / length, vector.1 / length), length))
}
fn along(anchor: Point, direction: (f64, f64), length: f64) -> Point {
    Point::new(
        (f64::from(anchor.x) + direction.0 * length) as f32,
        (f64::from(anchor.y) + direction.1 * length) as f32,
    )
}
impl Graphic {
    /// Node behavior in path order, detecting legacy paths without stored types.
    pub fn path_node_types(&self) -> Option<Vec<PathNodeType>> {
        Some(Path::read(self)?.node_types)
    }
    /// Resolve a node or control handle to its owning node's point index.
    pub fn path_node_index(&self, index: usize) -> Option<usize> {
        let path = Path::read(self)?;
        path.index(path.owner(index)?).ok()
    }
    /// Set a node's behavior, aligning its two cubic controls where available.
    pub fn set_path_node_type(&mut self, index: usize, kind: PathNodeType) -> Result<(), String> {
        let mut path = Path::read(self).ok_or("Select one continuous path")?;
        let node = path.owner(index).ok_or("Select a path node or control")?;
        path.align_node(node, kind);
        if let Some(node_type) = path.node_types.get_mut(node) {
            *node_type = kind;
        }
        path.store(self)
    }
    /// Handles for a single connected path. A closed seam has only one node.
    /// Imported compound paths retain the existing generic point editor.
    pub fn path_handles(&self) -> Option<Vec<PathHandle>> {
        self.path_summary().map(|summary| summary.handles)
    }
    /// Resolve a connected path's handles and node rules together.
    pub fn path_summary(&self) -> Option<PathSummary> {
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
        Some(PathSummary {
            handles: result,
            closed: path.closed,
        })
    }
    pub fn path_closed(&self) -> bool {
        self.commands()
            .iter()
            .any(|c| matches!(c, PathCommand::Close))
    }
    /// Move a connected path handle, reporting a frame that cannot be edited.
    /// Failed edits leave the graphic unchanged.
    pub fn move_path_point(&mut self, index: usize, point: Point) -> Result<(), String> {
        let path = Path::read(self).ok_or("Select one continuous path")?;
        self.move_parsed_path_point(path, index, point, false)
    }
    /// Move a control independently and persist its owning node as a corner.
    pub fn move_path_point_free(&mut self, index: usize, point: Point) -> Result<(), String> {
        let path = Path::read(self).ok_or("Select one continuous path")?;
        self.move_parsed_path_point(path, index, point, true)
    }
    pub(super) fn edit_connected_path_point(
        &mut self,
        index: usize,
        point: Point,
    ) -> Option<Result<(), String>> {
        let path = Path::read(self)?;
        Some(self.move_parsed_path_point(path, index, point, false))
    }
    fn move_parsed_path_point(
        &mut self,
        mut path: Path,
        index: usize,
        point: Point,
        free: bool,
    ) -> Result<(), String> {
        if !point.is_finite() {
            return Ok(());
        }
        let Some(old) = path.handle_point(index) else {
            return Ok(());
        };
        let free_control = free && path.node(index).is_none();
        if old == point && !free_control {
            return Ok(());
        }
        if let Some(node) = path.node(index) {
            let Ok(old) = path.point(node) else {
                return Ok(());
            };
            let move_control = |p: Point| p.offset(point.x - old.x, point.y - old.y);
            if let Some(segment) = path.segments.get_mut(node)
                && let Some([a, _]) = segment.controls.as_mut()
            {
                *a = move_control(*a);
            }
            let previous = path.previous_segment(node);
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
            let node = path.owner(index).ok_or("Invalid path control")?;
            let opposite = path.controls(node).map(|[incoming, anchor, outgoing]| {
                let is_outgoing = path
                    .segments
                    .get(node)
                    .and_then(|segment| segment.indices)
                    .is_some_and(|[first, _]| first == index);
                (
                    is_outgoing,
                    anchor,
                    if is_outgoing { incoming } else { outgoing },
                )
            });
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
            if free_control {
                if let Some(kind) = path.node_types.get_mut(node) {
                    *kind = PathNodeType::Corner;
                }
            } else if let Some((is_outgoing, anchor, opposite)) = opposite {
                let direction = unit(vector(anchor, point));
                let opposite = match path.node_types.get(node) {
                    None | Some(PathNodeType::Corner) => None,
                    Some(PathNodeType::Smooth) => direction.map(|(direction, _)| {
                        let (dx, dy) = vector(anchor, opposite);
                        along(anchor, direction, -dx.hypot(dy))
                    }),
                    Some(PathNodeType::Symmetric) => Some(match direction {
                        Some((direction, length)) => along(anchor, direction, -length),
                        None => anchor,
                    }),
                };
                if let Some(opposite) = opposite {
                    path.set_controls(
                        node,
                        if is_outgoing {
                            [opposite, point]
                        } else {
                            [point, opposite]
                        },
                    );
                }
            }
        }
        path.store(self)
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
            let ac = a.lerp(c, 0.5);
            let cd = c.lerp(d, 0.5);
            let db = d.lerp(b, 0.5);
            let l = ac.lerp(cd, 0.5);
            let r = cd.lerp(db, 0.5);
            (l.lerp(r, 0.5), Some([ac, l]), Some([r, db]))
        } else {
            (a.lerp(b, 0.5), None, None)
        };
        path.nodes.insert(node + 1, Node { point, index: 0 });
        path.node_types.insert(
            node + 1,
            if segment.controls.is_some() {
                PathNodeType::Smooth
            } else {
                PathNodeType::Corner
            },
        );
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
        if path.nodes.len()
            <= if path.closed {
                MIN_CLOSED_NODES
            } else {
                MIN_OPEN_NODES
            }
        {
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
                        .unwrap_or_else(|| a.lerp(removed, 1. / 3.)),
                    right
                        .map(|[_, b]| b)
                        .unwrap_or_else(|| b.lerp(removed, 1. / 3.)),
                ]
            });
            path.segments
                .get_mut(previous)
                .ok_or("Invalid path segment")?
                .controls = controls;
            path.segments.remove(node);
            path.nodes.remove(node);
        }
        path.node_types.remove(node);
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
                a.lerp(b, 1. / 3.).offset(bend.x, bend.y),
                a.lerp(b, 2. / 3.).offset(bend.x, bend.y),
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
            if path.nodes.len() < MIN_CLOSED_NODES {
                return Err("A closed path needs at least three nodes".into());
            }
            if path.nodes.last().map(|n| n.point) == path.nodes.first().map(|n| n.point) {
                path.nodes.pop();
                path.node_types.pop();
                if path.nodes.len() < MIN_CLOSED_NODES {
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
        graphic.reset_frame();
        graphic.node_types = vec![PathNodeType::Corner, PathNodeType::Symmetric];
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
        if !point.is_finite() || direction.is_some_and(|p| !p.is_finite()) {
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
                    .unwrap_or_else(|| last.lerp(point, 1. / 3.)),
                Point::new(2. * point.x - direction.x, 2. * point.y - direction.y),
            ]
        });
        path.nodes.push(Node { point, index: 0 });
        path.node_types.push(if direction.is_some() {
            PathNodeType::Symmetric
        } else {
            PathNodeType::Corner
        });
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
