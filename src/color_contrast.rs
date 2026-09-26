//! Perceptual palette generation, followed by contrast checks on final sRGB bytes.
//!
//! Oklab matrices: Björn Ottosson, 2021-01-25, public-domain implementation:
//! https://bottosson.github.io/posts/oklab/ . Palette/solver code is ReShiki's.
//! WCAG relative luminance is the engineering baseline, not a guarantee about
//! antialiasing, tiny projected text, or an unknown transparent-paste background.
pub type Rgb = [u8; 3];
pub const TEXT_MIN: f64 = 4.5;
pub const TEXT_TARGET: f64 = 5.;
pub const OUTLINE_TARGET: f64 = 3.2;

fn decode(v: u8) -> f64 {
    let v = f64::from(v) / 255.;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn encode(v: f64) -> u8 {
    let v = if v <= 0.0031308 {
        12.92 * v
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    };
    (v * 255.).round().clamp(0., 255.) as u8
}
pub fn luminance(rgb: Rgb) -> f64 {
    let [r, g, b] = rgb.map(decode);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
pub fn meets(ink: Rgb, backgrounds: &[Rgb], ratio: f64) -> bool {
    backgrounds.iter().all(|&bg| contrast(ink, bg) >= ratio)
}

#[derive(Clone, Copy, Debug)]
pub struct Oklch {
    pub l: f64,
    pub c: f64,
    /// Hue in radians. Near-neutral source colors have zero chroma.
    pub h: f64,
}
impl Oklch {
    pub fn from_rgb(rgb: Rgb) -> Self {
        let [r, g, b] = rgb.map(decode);
        let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
        let a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
        let b = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
        let c = a.hypot(b);
        Self {
            l: 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            c: if c < 1e-7 { 0. } else { c },
            h: if c < 1e-7 { 0. } else { b.atan2(a) },
        }
    }
    fn linear(self) -> [f64; 3] {
        let a = self.c * self.h.cos();
        let b = self.c * self.h.sin();
        let l = (self.l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
        let m = (self.l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
        let s = (self.l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
        [
            4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
            -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
            -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
        ]
    }
    /// Reduce chroma at constant hue/lightness to fit sRGB; never clip saturated
    /// channels independently. Contrast must be checked after this quantization.
    pub fn to_rgb(self) -> Rgb {
        let mut color = Self {
            l: self.l.clamp(0., 1.),
            c: self.c.max(0.),
            ..self
        };
        let fits = |v: [f64; 3]| v.iter().all(|&v| (-1e-7..=1.0000001).contains(&v));
        if !fits(color.linear()) {
            let (mut low, mut high) = (0., color.c);
            for _ in 0..22 {
                let mid = (low + high) / 2.;
                if fits(Self { c: mid, ..color }.linear()) {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            color.c = low;
        }
        color.linear().map(encode)
    }
}

/// Find nearby ink at the source hue, checking every background after gamut
/// mapping and 8-bit rounding. None means no passing candidate was found; callers
/// must report the conflict, not silently overwrite an explicit user color.
pub fn ensure_contrast(seed: Rgb, backgrounds: &[Rgb], ratio: f64) -> Option<Rgb> {
    if !ratio.is_finite() || !(1. ..=21.).contains(&ratio) {
        return None;
    }
    if meets(seed, backgrounds, ratio) {
        return Some(seed);
    }
    // Intersect all allowed foreground-luminance intervals. This also handles
    // opposing backgrounds, for which neither pure black nor white may work.
    let mut ranges = vec![(0_f64, 1_f64)];
    for &bg in backgrounds {
        let y = luminance(bg);
        let dark = (y + 0.05) / ratio - 0.05;
        let light = ratio * (y + 0.05) - 0.05;
        ranges = ranges
            .into_iter()
            .flat_map(|(lo, hi)| {
                [(lo, hi.min(dark)), (lo.max(light), hi)]
                    .into_iter()
                    .filter(|&(lo, hi)| lo <= hi)
            })
            .collect();
    }
    let source = Oklch::from_rgb(seed);
    let source_y = luminance(seed);
    let mut best: Option<(f64, Rgb)> = None;
    for (lo, hi) in ranges {
        // Move slightly inside the admissible interval, then check the bytes.
        // The midpoint and quartiles handle tight ranges near byte boundaries.
        let inset = ((hi - lo) * 0.05).min(0.003);
        for target in [
            source_y.clamp(lo + inset, hi - inset),
            (lo + hi) / 2.,
            lo + (hi - lo) * 0.25,
            lo + (hi - lo) * 0.75,
            lo,
            hi,
        ] {
            let (mut low, mut high) = (0., 1.);
            for _ in 0..24 {
                let mid = (low + high) / 2.;
                let rgb = Oklch { l: mid, ..source }.to_rgb();
                if luminance(rgb) < target {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            for l in [low, high] {
                let rgb = Oklch { l, ..source }.to_rgb();
                if meets(rgb, backgrounds, ratio) {
                    let distance = (l - source.l).abs();
                    if best.is_none_or(|(d, _)| distance < d) {
                        best = Some((distance, rgb));
                    }
                }
            }
        }
    }
    best.map(|(_, rgb)| rgb)
}

/// Separate tonal roles for colored element tiles. Text remains neutral and
/// regular weight; selection is additionally identified by a contrasting outline.
pub fn tile(seed: Rgb, dark: bool, selected: bool, hovered: bool) -> Rgb {
    let source = Oklch::from_rgb(seed);
    let (l, scale) = match (dark, selected, hovered) {
        (false, true, _) => (0.87, 0.46),
        (false, false, true) => (0.91, 0.35),
        (false, false, false) => (0.95, 0.25),
        (true, true, _) => (0.39, 0.46),
        (true, false, true) => (0.34, 0.35),
        (true, false, false) => (0.29, 0.25),
    };
    Oklch {
        l,
        c: source.c * scale,
        ..source
    }
    .to_rgb()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_colors_round_trip_and_gamut_mapping_preserves_hue() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let rgb = [r, g, b];
                    assert_eq!(Oklch::from_rgb(rgb).to_rgb(), rgb);
                }
            }
        }
        let red = Oklch::from_rgb([255, 0, 0]);
        assert!((red.l - 0.62795536).abs() < 1e-7);
        assert!((red.c - 0.2576833).abs() < 1e-7);
        let mapped = Oklch::from_rgb(
            Oklch {
                l: 0.8,
                c: 0.4,
                ..red
            }
            .to_rgb(),
        );
        assert!((mapped.l - 0.8).abs() < 0.003);
        assert!((mapped.h - red.h).abs() < 0.025);
        assert!(mapped.c < 0.4);
        assert_eq!(contrast([0; 3], [255; 3]), 21.);
        assert!(
            contrast([119; 3], [255; 3]) < TEXT_MIN,
            "never round 4.478 to a pass"
        );
    }
    #[test]
    fn solves_multiple_backgrounds_and_reports_conflicts() {
        for seed in [[255, 255, 0], [10, 20, 230], [128; 3], [255, 0, 0]] {
            for backgrounds in [
                vec![[255; 3]],
                vec![[0; 3]],
                vec![[255; 3], [230, 245, 235]],
            ] {
                let ink = ensure_contrast(seed, &backgrounds, TEXT_TARGET).unwrap();
                assert!(meets(ink, &backgrounds, TEXT_TARGET));
                assert_eq!(ensure_contrast(ink, &backgrounds, TEXT_TARGET), Some(ink));
            }
        }
        let opposing = [[0; 3], [255; 3]];
        let gray = ensure_contrast([128; 3], &opposing, TEXT_MIN).unwrap();
        assert!(meets(gray, &opposing, TEXT_MIN));
        assert_eq!(ensure_contrast([128; 3], &opposing, TEXT_TARGET), None);
        assert_eq!(
            ensure_contrast([0; 3], &[[0; 3], [120; 3], [255; 3]], TEXT_MIN),
            None
        );
    }
}
