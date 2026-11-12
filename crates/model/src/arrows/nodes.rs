use super::{Arrow, ArrowHandle, unit};
use crate::{
    document::{ArrowNode, Point},
    graphics::{Graphic, GraphicStyle, PathCommand, PathNodeType},
};

/// First handle index of interior arrow nodes: node k owns 5 + 3k (node),
/// 5 + 3k + 1 (arriving control) and 5 + 3k + 2 (leaving control).
pub const FIRST_NODE_HANDLE: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodePart {
    Node,
    Arriving,
    Leaving,
}

/// The interior node and part an arrow handle index addresses.
pub const fn node_handle(index: usize) -> Option<(usize, NodePart)> {
    if index < FIRST_NODE_HANDLE {
        return None;
    }
    let index = index - FIRST_NODE_HANDLE;
    Some((
        index / 3,
        match index % 3 {
            0 => NodePart::Node,
            1 => NodePart::Arriving,
            _ => NodePart::Leaving,
        },
    ))
}

fn scaled_parameter(t: f32, segments: f32) -> f32 {
    let scaled = t * segments;
    let boundary = scaled.round();
    // A stored node parameter i/n need not multiply back to the integer i.
    if t == boundary / segments {
        boundary
    } else {
        scaled
    }
}

fn relax_node_type(node: &mut ArrowNode) {
    let direction = |a: Point, b: Point| {
        let dx = f64::from(b.x) - f64::from(a.x);
        let dy = f64::from(b.y) - f64::from(a.y);
        let length = dx.hypot(dy);
        (length > 1e-9).then_some((dx / length, dy / length, length))
    };
    let kind = match (
        direction(node.controls[0], node.point),
        direction(node.point, node.controls[1]),
    ) {
        (Some(a), Some(b)) if a.0 * b.0 + a.1 * b.1 >= 1_f64.to_radians().cos() => {
            if (a.2 - b.2).abs() <= a.2.max(b.2) * 0.01 {
                PathNodeType::Symmetric
            } else {
                PathNodeType::Smooth
            }
        }
        _ => PathNodeType::Corner,
    };
    node.kind = node.kind.min(kind);
}

#[derive(Clone, Copy)]
pub(super) struct Segment([Point; 4]);
impl Segment {
    pub(super) fn point(self, t: f32) -> Point {
        let [p, a, b, q] = self.0;
        if t == 0. {
            return p;
        }
        if t == 1. {
            return q;
        }
        let ab = a.lerp(b, t);
        p.lerp(a, t).lerp(ab, t).lerp(ab.lerp(b.lerp(q, t), t), t)
    }
    pub(super) fn velocity(self, t: f32) -> Point {
        let [p, a, b, q] = self.0;
        let u = 1. - t;
        Point::new(
            3. * (u * u * (a.x - p.x) + 2. * u * t * (b.x - a.x) + t * t * (q.x - b.x)),
            3. * (u * u * (a.y - p.y) + 2. * u * t * (b.y - a.y) + t * t * (q.y - b.y)),
        )
    }
    pub(super) fn tangent(self, t: f32) -> Point {
        let v = self.velocity(t);
        if v.distance(Point::default()) >= 0.0001 {
            return unit(Point::default(), v);
        }
        let [p, a, b, q] = self.0;
        if t == 0. {
            unit(p, if b == p { q } else { b })
        } else if t == 1. {
            unit(if a == q { p } else { a }, q)
        } else {
            unit(
                self.point((t - 0.001).max(0.)),
                self.point((t + 0.001).min(1.)),
            )
        }
    }
    pub(super) fn offset_point(self, t: f32, offset: f32) -> Point {
        let d = self.tangent(t);
        self.point(t).offset(-d.y * offset, d.x * offset)
    }
    pub(super) fn offset_velocity(self, t: f32, offset: f32) -> Point {
        let v = self.velocity(t);
        if offset == 0. {
            return v;
        }
        let speed = v.distance(Point::default());
        if speed < 0.0001 {
            let from = (t - 0.0005).max(0.);
            let to = (t + 0.0005).min(1.);
            let a = self.offset_point(from, offset);
            let b = self.offset_point(to, offset);
            return Point::new((b.x - a.x) / (to - from), (b.y - a.y) / (to - from));
        }
        let [p, a, b, q] = self.0;
        let acceleration = Point::new(
            6. * ((1. - t) * (b.x - 2. * a.x + p.x) + t * (q.x - 2. * b.x + a.x)),
            6. * ((1. - t) * (b.y - 2. * a.y + p.y) + t * (q.y - 2. * b.y + a.y)),
        );
        let projection = (v.x * acceleration.x + v.y * acceleration.y) / (speed * speed);
        v.offset(
            offset * (-acceleration.y + v.y * projection) / speed,
            offset * (acceleration.x - v.x * projection) / speed,
        )
    }
}

impl Arrow {
    fn segment(&self, index: usize) -> Segment {
        let [departure, arrival] = self.bezier_controls().unwrap_or([
            self.start.lerp(self.end, 1. / 3.),
            self.start.lerp(self.end, 2. / 3.),
        ]);
        let (start, a) = index
            .checked_sub(1)
            .and_then(|i| self.nodes.get(i))
            .map_or((self.start, departure), |node| {
                (node.point, node.controls[1])
            });
        let (b, end) = match self.nodes.get(index) {
            Some(node) => (node.controls[0], node.point),
            None => (arrival, self.end),
        };
        Segment([start, a, b, end])
    }
    pub(super) fn node_segment_at(&self, t: f32, incoming: bool) -> (Segment, f32) {
        let n = (self.nodes.len() + 1) as f32;
        let t = scaled_parameter(t.clamp(0., 1.), n);
        let index = if incoming && t > 0. && t.fract() == 0. {
            t as usize - 1
        } else {
            (t as usize).min(self.nodes.len())
        };
        (self.segment(index), t - index as f32)
    }
    pub(super) fn offset_point_side(&self, t: f32, offset: f32, incoming: bool) -> Point {
        if self.nodes.is_empty() {
            return self.offset_point(t, offset);
        }
        let (segment, t) = self.node_segment_at(t, incoming);
        segment.offset_point(t, offset)
    }
    /// The shaft parameter at a fraction of its length. Decorations on an
    /// arrow with nodes are placed by length, so adding a node leaves them in
    /// place; a single cubic keeps its parameter.
    pub(super) fn length_parameter(&self, fraction: f32) -> f32 {
        if self.nodes.is_empty() || fraction <= 0. || fraction >= 1. {
            return fraction;
        }
        let steps = 64 * (self.nodes.len() + 1);
        let points: Vec<_> = (0..=steps)
            .map(|i| self.point(i as f32 / steps as f32))
            .collect();
        let steps_between = || points.iter().zip(points.iter().skip(1));
        let total: f32 = steps_between().map(|(a, b)| a.distance(*b)).sum();
        let mut remaining = fraction * total;
        for (i, (a, b)) in steps_between().enumerate() {
            let step = a.distance(*b);
            if step >= remaining && step > 0. {
                let t = (i as f32 + remaining / step) / steps as f32;
                // A decoration landing on a node uses that node exactly.
                let n = (self.nodes.len() + 1) as f32;
                let node = (t * n).round() / n;
                return if (t - node).abs() < 1e-4 { node } else { t };
            }
            remaining -= step;
        }
        fraction
    }
    pub(super) fn node_span(&self) -> f32 {
        let mut previous = self.start;
        self.nodes
            .iter()
            .map(|node| node.point)
            .chain([self.end])
            .map(|point| {
                let length = previous.distance(point);
                previous = point;
                length
            })
            .sum()
    }
    pub(super) fn node_shaft_body(&self, from: f32, to: f32, offset: f32) -> Vec<PathCommand> {
        use PathCommand::{Cubic, Line, Move};
        let n = (self.nodes.len() + 1) as f32;
        let mut cuts = vec![from];
        cuts.extend(
            (1..=self.nodes.len())
                .map(|i| i as f32 / n)
                .filter(|t| *t > from.min(to) && *t < from.max(to)),
        );
        cuts.push(to);
        if from > to {
            cuts.sort_by(|a, b| b.total_cmp(a));
        }
        let mut commands = Vec::new();
        let mut previous = None;
        for (&from, &to) in cuts.iter().zip(cuts.iter().skip(1)) {
            let index = (((from + to) * 0.5 * n) as usize).min(self.nodes.len());
            let segment = self.segment(index);
            let from = (scaled_parameter(from, n) - index as f32).clamp(0., 1.);
            let to = (scaled_parameter(to, n) - index as f32).clamp(0., 1.);
            let start = segment.offset_point(from, offset);
            match previous {
                None => commands.push(Move(start)),
                Some(point) if offset != 0. && point != start => commands.push(Line(start)),
                _ => {}
            }
            if offset == 0. && ((from == 0. && to == 1.) || (from == 1. && to == 0.)) {
                let Segment([p, a, b, q]) = segment;
                // Preserve full segments' saved controls without derivative cancellation.
                commands.push(if from == 0. {
                    Cubic(a, b, q)
                } else {
                    Cubic(b, a, p)
                });
                previous = Some(segment.point(to));
                continue;
            }
            let steps = if offset == 0. { 1 } else { 16 };
            let dt = (to - from) / steps as f32;
            for i in 0..steps {
                let t0 = from + i as f32 * dt;
                let t1 = if i + 1 == steps {
                    to
                } else {
                    from + (i + 1) as f32 * dt
                };
                let p0 = segment.offset_point(t0, offset);
                let p1 = segment.offset_point(t1, offset);
                let v0 = segment.offset_velocity(t0, offset);
                let v1 = segment.offset_velocity(t1, offset);
                commands.push(Cubic(
                    p0.offset(v0.x * dt / 3., v0.y * dt / 3.),
                    p1.offset(-v1.x * dt / 3., -v1.y * dt / 3.),
                    p1,
                ));
            }
            previous = Some(segment.offset_point(to, offset));
        }
        commands
    }
    /// Interior node handles in pick order: (index, point, part).
    pub fn node_handles(&self) -> Vec<(usize, Point, NodePart)> {
        self.nodes
            .iter()
            .enumerate()
            .flat_map(|(i, node)| {
                let index = FIRST_NODE_HANDLE + i * 3;
                [
                    (index, node.point, NodePart::Node),
                    (index + 1, node.controls[0], NodePart::Arriving),
                    (index + 2, node.controls[1], NodePart::Leaving),
                ]
            })
            .collect()
    }
    /// Any visible handle's point, including node handles. The Bend handle
    /// (index 2) is not visible on an arrow with nodes.
    pub fn handle_point(&self, index: usize) -> Option<Point> {
        if let Some((node, part)) = node_handle(index) {
            return self.nodes.get(node).map(|node| match part {
                NodePart::Node => node.point,
                NodePart::Arriving => node.controls[0],
                NodePart::Leaving => node.controls[1],
            });
        }
        if index == ArrowHandle::Bend.index() && !self.has_bend_handle() {
            return None;
        }
        self.handles().get(index).copied()
    }
    /// Whether the Bend handle is offered (false when `nodes` is non-empty).
    pub fn has_bend_handle(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Nearest point on the shaft centerline: (global t, point, distance).
    pub fn nearest_shaft_point(&self, p: Point) -> (f32, Point, f32) {
        let mut best = (0., self.start, p.distance(self.start));
        let mut consider = |t: f32, point: Point| {
            let distance = p.distance(point);
            if distance < best.2 {
                best = (t, point, distance);
            }
        };
        if self.kind == "bent" {
            let middle = self
                .control_point()
                .unwrap_or(self.start.lerp(self.end, 0.5));
            for (i, [a, b]) in [[self.start, middle], [middle, self.end]]
                .into_iter()
                .enumerate()
            {
                let v = Point::new(b.x - a.x, b.y - a.y);
                let length = v.x * v.x + v.y * v.y;
                let t = if length > 0. {
                    ((p.x - a.x) * v.x + (p.y - a.y) * v.y) / length
                } else {
                    0.
                }
                .clamp(0., 1.);
                consider((i as f32 + t) * 0.5, a.lerp(b, t));
            }
            return best;
        }
        let n = (self.nodes.len() + 1) as f32;
        for i in 0..=self.nodes.len() {
            let segment = self.segment(i);
            let distance = |t| segment.point(t).distance(p);
            let samples: usize = 64;
            let mut nearest = 0;
            let mut minimum = distance(0.);
            for sample in 0..=samples {
                let t = sample as f32 / samples as f32;
                let d = distance(t);
                consider((i as f32 + t) / n, segment.point(t));
                if d < minimum {
                    minimum = d;
                    nearest = sample;
                }
            }
            let mut lo = nearest.saturating_sub(1) as f32 / samples as f32;
            let mut hi = (nearest + 1).min(samples) as f32 / samples as f32;
            for _ in 0..32 {
                let a = lo + (hi - lo) / 3.;
                let b = hi - (hi - lo) / 3.;
                if distance(a) <= distance(b) {
                    hi = b;
                } else {
                    lo = a;
                }
            }
            let t = (lo + hi) * 0.5;
            consider((i as f32 + t) / n, segment.point(t));
        }
        best
    }
    /// Split the shaft nearest `p`, preserving its curve and adding a Smooth
    /// node (Corner at a collapsed tangent). Returns the new node handle index.
    pub fn insert_node(&mut self, p: Point) -> Result<usize, String> {
        if self.kind == "bent" {
            return Err("Elbow arrows cannot have interior nodes".into());
        }
        if !p.is_finite() {
            return Err("Non-finite arrow position".into());
        }
        let (t, _, _) = self.nearest_shaft_point(p);
        let n = (self.nodes.len() + 1) as f32;
        let scaled = scaled_parameter(t, n);
        let index = (scaled as usize).min(self.nodes.len());
        let t = (scaled - index as f32).clamp(0., 1.);
        let Segment([start, a, b, end]) = self.segment(index);
        let ab = a.lerp(b, t);
        let left = start.lerp(a, t);
        let right = b.lerp(end, t);
        let incoming = left.lerp(ab, t);
        let outgoing = ab.lerp(right, t);
        let point = incoming.lerp(outgoing, t);
        let endpoint_controls = self.bezier_controls().unwrap_or([
            self.start.lerp(self.end, 1. / 3.),
            self.start.lerp(self.end, 2. / 3.),
        ]);
        let controls = self.cubic.get_or_insert(endpoint_controls);
        if index == 0 {
            controls[0] = left;
        } else if let Some(node) = self.nodes.get_mut(index - 1) {
            node.controls[1] = left;
            relax_node_type(node);
        }
        if index == self.nodes.len() {
            controls[1] = right;
        } else if let Some(node) = self.nodes.get_mut(index) {
            node.controls[0] = right;
            relax_node_type(node);
        }
        self.control = None;
        self.nodes.insert(
            index,
            ArrowNode {
                point,
                controls: [incoming, outgoing],
                kind: if incoming.distance(outgoing) < 1e-9 {
                    PathNodeType::Corner
                } else {
                    PathNodeType::Smooth
                },
            },
        );
        Ok(FIRST_NODE_HANDLE + index * 3)
    }
    /// Remove the node addressed by any of its handles, keeping outer controls.
    /// Removing the last node leaves an ordinary single-cubic arrow.
    pub fn delete_node(&mut self, index: usize) -> Result<(), String> {
        let (node, _) = node_handle(index)
            .filter(|(node, _)| *node < self.nodes.len())
            .ok_or("Select an interior arrow node or control")?;
        self.nodes.remove(node);
        Ok(())
    }
    pub fn node_type(&self, index: usize) -> Option<PathNodeType> {
        self.nodes.get(node_handle(index)?.0).map(|node| node.kind)
    }
    /// Set a node's type, aligning its controls like a pen path.
    pub fn set_node_type(&mut self, index: usize, kind: PathNodeType) -> Result<(), String> {
        let (node, _) = node_handle(index)
            .filter(|(node, _)| *node < self.nodes.len())
            .ok_or("Select an interior arrow node or control")?;
        let mut graphic = self.node_graphic();
        graphic.set_path_node_type((node + 1) * 3, kind)?;
        self.read_node_graphic(&graphic);
        Ok(())
    }
    /// Move a node control alone and make its node a Corner (Option/Alt-drag).
    pub fn edit_handle_free(&mut self, index: usize, p: Point) {
        if node_handle(index).is_some() {
            self.edit_node_handle(index, p, true);
        } else {
            self.edit_handle(index, p);
        }
    }
    pub(super) fn edit_node_handle(&mut self, index: usize, p: Point, free: bool) {
        let Some((node, part)) = node_handle(index).filter(|(node, _)| *node < self.nodes.len())
        else {
            return;
        };
        let index = (node + 1) * 3;
        let index = match part {
            NodePart::Node => index,
            NodePart::Arriving => index - 1,
            NodePart::Leaving => index + 1,
        };
        let mut graphic = self.node_graphic();
        let result = if free {
            graphic.move_path_point_free(index, p)
        } else {
            graphic.move_path_point(index, p)
        };
        if result.is_ok() {
            self.read_node_graphic(&graphic);
        }
    }
    fn node_graphic(&self) -> Graphic {
        let mut graphic = Graphic::pen_curve(
            self.id,
            self.start,
            self.end,
            GraphicStyle {
                width_pt: self.appearance().width_pt,
                ..GraphicStyle::default()
            },
        );
        graphic.reset_frame();
        graphic.path = vec![PathCommand::Move(self.start)];
        for i in 0..=self.nodes.len() {
            let Segment([_, a, b, end]) = self.segment(i);
            graphic.path.push(PathCommand::Cubic(a, b, end));
        }
        graphic.node_types = std::iter::once(PathNodeType::Corner)
            .chain(self.nodes.iter().map(|node| node.kind))
            .chain([PathNodeType::Corner])
            .collect();
        graphic
    }
    fn read_node_graphic(&mut self, graphic: &Graphic) {
        let commands = graphic.commands();
        let mut segments = commands.iter().filter_map(|command| match command {
            PathCommand::Cubic(a, b, p) => Some([*a, *b, *p]),
            _ => None,
        });
        let Some([departure, mut arrival, mut point]) = segments.next() else {
            return;
        };
        self.nodes.clear();
        for ([a, b, end], &kind) in segments.zip(graphic.node_types.iter().skip(1)) {
            self.nodes.push(ArrowNode {
                point,
                controls: [arrival, a],
                kind,
            });
            arrival = b;
            point = end;
        }
        self.cubic = Some([departure, arrival]);
    }
    pub(super) fn smooth_nodes(&mut self) -> Result<bool, String> {
        let mut graphic = self.node_graphic();
        if !graphic.smooth_path()? {
            return Ok(false);
        }
        self.read_node_graphic(&graphic);
        Ok(true)
    }
    pub(super) fn symmetrise_nodes(&mut self) -> Result<bool, String> {
        let start = self.start;
        let end = self.end;
        let dx = f64::from(end.x) - f64::from(start.x);
        let dy = f64::from(end.y) - f64::from(start.y);
        let length = dx.hypot(dy);
        if length <= 1e-9 {
            return Ok(false);
        }
        let (ux, uy) = (dx / length, dy / length);
        let middle = (
            (f64::from(start.x) + f64::from(end.x)) * 0.5,
            (f64::from(start.y) + f64::from(end.y)) * 0.5,
        );
        let reflect = |p: Point| {
            let projection = (f64::from(p.x) - middle.0) * ux + (f64::from(p.y) - middle.1) * uy;
            Point::new(
                (f64::from(p.x) - 2. * projection * ux) as f32,
                (f64::from(p.y) - 2. * projection * uy) as f32,
            )
        };
        let Some([departure, arrival]) = self.cubic else {
            return Ok(false);
        };
        let mut mismatch = departure.distance(reflect(arrival));
        for (a, b) in self.nodes.iter().zip(self.nodes.iter().rev()) {
            for (a, b) in [
                (a.point, b.point),
                (a.controls[0], b.controls[1]),
                (a.controls[1], b.controls[0]),
            ] {
                mismatch = mismatch.max(a.distance(reflect(b)));
            }
        }
        if f64::from(mismatch) > length * 0.2 {
            return Err("Not symmetric enough: arrow nodes are too asymmetric".into());
        }
        if f64::from(mismatch) <= length * 1e-6 {
            return Ok(false);
        }
        let average = |a: Point, b: Point| a.lerp(reflect(b), 0.5);
        self.cubic = Some([average(departure, arrival), average(arrival, departure)]);
        let old = self.nodes.clone();
        for (node, opposite) in self.nodes.iter_mut().zip(old.iter().rev()) {
            node.point = average(node.point, opposite.point);
            node.controls = [
                average(node.controls[0], opposite.controls[1]),
                average(node.controls[1], opposite.controls[0]),
            ];
            relax_node_type(node);
        }
        Ok(true)
    }
}
