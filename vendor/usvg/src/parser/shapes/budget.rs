//! Bound arc expansion before svgtypes eagerly buffers its cubic segments.
//! Endpoint updates mirror SimplifyingPathParser, including its rounded arc
//! endpoint, so a later relative arc is checked with the same starting point.

pub(super) struct Budget {
    pub(super) max: Option<usize>,
    pub(super) max_empty: Option<usize>,
    pub(super) exceeded: bool,
}

impl Budget {
    fn charge(&mut self, used: &mut usize, limit: usize) -> bool {
        if *used == limit {
            self.exceeded = true;
            return false;
        }
        *used += 1;
        true
    }

    pub(super) fn preflight_path(&mut self, value: &str) -> bool {
        let Some(limit) = self.max else {
            // Existing usvg callers retain the original normalization behavior.
            return true;
        };
        if self.exceeded {
            return false;
        }

        use svgtypes::PathSegment;

        let mut current = kurbo::Point::new(0., 0.);
        let mut subpath = current;
        let mut closed = false;
        let mut raw = 0;
        let mut normalized = 0;
        let mut empty = 0usize;
        for segment in svgtypes::PathParser::from(value) {
            let segment = match segment {
                Ok(segment) => segment,
                // Upstream keeps the valid prefix of malformed path data.
                Err(_) => break,
            };
            // Also bound raw work when repeated closes or empty arcs emit no
            // normalized segments.
            if !self.charge(&mut raw, limit) {
                return false;
            }

            let before = normalized;
            // svgtypes inserts a move after a close before a drawing command.
            if closed
                && !matches!(
                    segment,
                    PathSegment::MoveTo { .. } | PathSegment::ClosePath { .. }
                )
            {
                if !self.charge(&mut normalized, limit) {
                    return false;
                }
                closed = false;
            }

            match segment {
                PathSegment::EllipticalArc {
                    abs,
                    rx,
                    ry,
                    x_axis_rotation,
                    large_arc,
                    sweep,
                    x,
                    y,
                } => {
                    let to = if abs {
                        kurbo::Point::new(x, y)
                    } else {
                        kurbo::Point::new(current.x + x, current.y + y)
                    };
                    let svg_arc = kurbo::SvgArc {
                        from: current,
                        to,
                        radii: kurbo::Vec2::new(rx, ry),
                        x_rotation: x_axis_rotation.to_radians(),
                        large_arc,
                        sweep,
                    };
                    if let Some(arc) = kurbo::Arc::from_svg_arc(&svg_arc) {
                        // This is the same iterator and tolerance used by
                        // to_cubic_beziers, without allocating an output Vec.
                        let mut curves = arc.append_iter(0.1);
                        while let Some(kurbo::PathEl::CurveTo(_, _, end)) = curves.next() {
                            if !self.charge(&mut normalized, limit) {
                                return false;
                            }
                            current = end;
                        }
                    } else {
                        if !self.charge(&mut normalized, limit) {
                            return false;
                        }
                        current = to;
                    }
                }
                PathSegment::ClosePath { .. } => {
                    // Consecutive closes produce only one upstream segment.
                    if !closed && !self.charge(&mut normalized, limit) {
                        return false;
                    }
                    current = subpath;
                    closed = true;
                }
                _ => {
                    if !self.charge(&mut normalized, limit) {
                        return false;
                    }
                    current = match segment {
                        PathSegment::MoveTo { abs, x, y }
                        | PathSegment::LineTo { abs, x, y }
                        | PathSegment::CurveTo { abs, x, y, .. }
                        | PathSegment::SmoothCurveTo { abs, x, y, .. }
                        | PathSegment::Quadratic { abs, x, y, .. }
                        | PathSegment::SmoothQuadratic { abs, x, y } => {
                            if abs {
                                kurbo::Point::new(x, y)
                            } else {
                                kurbo::Point::new(current.x + x, current.y + y)
                            }
                        }
                        PathSegment::HorizontalLineTo { abs, x } => {
                            kurbo::Point::new(if abs { x } else { current.x + x }, current.y)
                        }
                        PathSegment::VerticalLineTo { abs, y } => {
                            kurbo::Point::new(current.x, if abs { y } else { current.y + y })
                        }
                        // Arcs and closes were handled above.
                        PathSegment::EllipticalArc { .. } | PathSegment::ClosePath { .. } => {
                            current
                        }
                    };
                    if matches!(segment, PathSegment::MoveTo { .. }) {
                        subpath = current;
                    }
                    closed = false;
                }
            }
            // svgtypes recursively advances past commands with no output.
            if normalized == before {
                empty = empty.saturating_add(1);
                if self.max_empty.is_some_and(|limit| empty > limit) {
                    self.exceeded = true;
                    return false;
                }
            } else {
                empty = 0;
            }
        }
        true
    }
}
