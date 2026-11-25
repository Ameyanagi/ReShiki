use super::{Counts, Oracle, Paint, Visible};
use std::fmt::{Display, Write};

fn write_points<T: Display>(svg: &mut String, points: &[[T; 2]]) {
    for [x, y] in points {
        write!(svg, "{x:.3},{y:.3} ").unwrap();
    }
}

pub(super) fn svg(
    oracle: &Oracle<'_>,
    result: &Visible,
    tones: &[usize],
    counts: &Counts,
    disagreements: &[[f64; 2]],
) -> String {
    let mut bounds = oracle.bounds;
    let mut include = |[x, y]: [f64; 2]| {
        bounds[0] = bounds[0].min(x);
        bounds[1] = bounds[1].min(y);
        bounds[2] = bounds[2].max(x);
        bounds[3] = bounds[3].max(y);
    };
    let mut paint = oracle.paint.clone();
    let depth = |item: &Paint| match item {
        Paint::Ball { center, .. } => center[2],
        Paint::Stick { a, b, .. } => (a[2] + b[2]) * 0.5,
    };
    paint.sort_by(|a, b| depth(a).total_cmp(&depth(b)));
    let mut scene = String::new();
    for item in paint {
        match item {
            Paint::Ball {
                center: [x, y, _],
                radius,
                ..
            } => {
                include([x - radius, y - radius]);
                include([x + radius, y + radius]);
                writeln!(
                    scene,
                    r##"<circle cx="{x:.3}" cy="{y:.3}" r="{radius:.3}" fill="#c8ccd2" stroke="#1e2226"/>"##
                )
                .unwrap();
            }
            Paint::Stick { a, b, half, .. } => {
                let [dx, dy] = [b[0] - a[0], b[1] - a[1]];
                let length = dx.hypot(dy);
                if length < 1e-8 {
                    continue;
                }
                let corner = |p: [f64; 3], w: f64| [p[0] - w * dy / length, p[1] + w * dx / length];
                let quad = [
                    corner(a, half[0]),
                    corner(b, half[1]),
                    corner(b, -half[1]),
                    corner(a, -half[0]),
                ];
                for p in quad {
                    include(p);
                }
                scene.push_str("<polygon points=\"");
                write_points(&mut scene, &quad);
                writeln!(scene, r##"" fill="#9aa0a8"/>"##).unwrap();
            }
        }
    }
    for layer in &result.layers {
        let (dark, light) = if layer.positive {
            ([43., 79., 158.], [169., 194., 242.])
        } else {
            ([138., 143., 150.], [244., 245., 247.])
        };
        let t = layer.tone as f64 / tones[layer.depth].saturating_sub(1).max(1) as f64;
        let [r, g, b] = std::array::from_fn(|i| (dark[i] + t * (light[i] - dark[i])).round() as u8);
        write!(
            scene,
            r##"<path fill="#{r:02x}{g:02x}{b:02x}" fill-rule="evenodd" d=""##
        )
        .unwrap();
        for contour in &layer.region {
            scene.push('M');
            write_points(&mut scene, contour);
            scene.push('Z');
        }
        scene.push_str("\"/>\n");
    }
    for rim in result.rims.iter().rev().flatten() {
        scene.push_str(r##"<polyline fill="none" stroke="#1e2226" points=""##);
        write_points(&mut scene, rim);
        scene.push_str("\"/>\n");
    }

    let [left, top, right, bottom] = bounds;
    let scale = (448. / (right - left)).min(412. / (bottom - top));
    let x = 240. - scale * (left + right) * 0.5;
    let y = 222. - scale * (top + bottom) * 0.5;
    let stroke = 0.8 / scale;
    let disagree = disagreements.len();
    let agree = counts.checked - disagree;
    let skipped = counts.skipped;
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="480" height="480" viewBox="0 0 480 480">
<rect width="480" height="480" fill="#ffffff"/>
<g transform="translate({x:.6} {y:.6}) scale({scale:.6})" stroke-width="{stroke:.6}">
{scene}</g>
<rect y="444" width="480" height="36" fill="#f4f5f7"/>
<text x="240" y="467" text-anchor="middle" font-family="sans-serif" font-size="14" fill="#1e2226">ray-cast oracle: {agree} agree, {disagree} disagree, {skipped} skipped</text>
"##
    );
    for &[px, py] in disagreements {
        let cx = x + scale * px;
        let cy = y + scale * py;
        writeln!(
            svg,
            r##"<circle cx="{cx:.3}" cy="{cy:.3}" r="2" fill="#e00000"/>"##
        )
        .unwrap();
    }
    svg.push_str("</svg>\n");
    svg
}
