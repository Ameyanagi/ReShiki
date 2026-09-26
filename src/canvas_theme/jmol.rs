//! Numerical element colors published by Jmol.
//! Source: https://jmol.sourceforge.net/jscolors/ (jmol_constants.js).
//! The source table specifies H–Mt; later elements retain neutral colors.
use super::CanvasTheme;

const JMOL: [u32; 109] = [
    0xFFFFFF, // H
    0xD9FFFF, // He
    0xCC80FF, // Li
    0xC2FF00, // Be
    0xFFB5B5, // B
    0x909090, // C
    0x3050F8, // N
    0xFF0D0D, // O
    0x90E050, // F
    0xB3E3F5, // Ne
    0xAB5CF2, // Na
    0x8AFF00, // Mg
    0xBFA6A6, // Al
    0xF0C8A0, // Si
    0xFF8000, // P
    0xFFFF30, // S
    0x1FF01F, // Cl
    0x80D1E3, // Ar
    0x8F40D4, // K
    0x3DFF00, // Ca
    0xE6E6E6, // Sc
    0xBFC2C7, // Ti
    0xA6A6AB, // V
    0x8A99C7, // Cr
    0x9C7AC7, // Mn
    0xE06633, // Fe
    0xF090A0, // Co
    0x50D050, // Ni
    0xC88033, // Cu
    0x7D80B0, // Zn
    0xC28F8F, // Ga
    0x668F8F, // Ge
    0xBD80E3, // As
    0xFFA100, // Se
    0xA62929, // Br
    0x5CB8D1, // Kr
    0x702EB0, // Rb
    0x00FF00, // Sr
    0x94FFFF, // Y
    0x94E0E0, // Zr
    0x73C2C9, // Nb
    0x54B5B5, // Mo
    0x3B9E9E, // Tc
    0x248F8F, // Ru
    0x0A7D8C, // Rh
    0x006985, // Pd
    0xC0C0C0, // Ag
    0xFFD98F, // Cd
    0xA67573, // In
    0x668080, // Sn
    0x9E63B5, // Sb
    0xD47A00, // Te
    0x940094, // I
    0x429EB0, // Xe
    0x57178F, // Cs
    0x00C900, // Ba
    0x70D4FF, // La
    0xFFFFC7, // Ce
    0xD9FFC7, // Pr
    0xC7FFC7, // Nd
    0xA3FFC7, // Pm
    0x8FFFC7, // Sm
    0x61FFC7, // Eu
    0x45FFC7, // Gd
    0x30FFC7, // Tb
    0x1FFFC7, // Dy
    0x00FF9C, // Ho
    0x00E675, // Er
    0x00D452, // Tm
    0x00BF38, // Yb
    0x00AB24, // Lu
    0x4DC2FF, // Hf
    0x4DA6FF, // Ta
    0x2194D6, // W
    0x267DAB, // Re
    0x266696, // Os
    0x175487, // Ir
    0xD0D0E0, // Pt
    0xFFD123, // Au
    0xB8B8D0, // Hg
    0xA6544D, // Tl
    0x575961, // Pb
    0x9E4FB5, // Bi
    0xAB5C00, // Po
    0x754F45, // At
    0x428296, // Rn
    0x420066, // Fr
    0x007D00, // Ra
    0x70ABFA, // Ac
    0x00BAFF, // Th
    0x00A1FF, // Pa
    0x008FFF, // U
    0x0080FF, // Np
    0x006BFF, // Pu
    0x545CF2, // Am
    0x785CE3, // Cm
    0x8A4FE3, // Bk
    0xA136D4, // Cf
    0xB31FD4, // Es
    0xB31FBA, // Fm
    0xB30DA6, // Md
    0xBD0D87, // No
    0xC70066, // Lr
    0xCC0059, // Rf
    0xD1004F, // Db
    0xD90045, // Sg
    0xE00038, // Bh
    0xE6002E, // Hs
    0xEB0026, // Mt
];

pub(super) fn swatch(element: &str) -> Option<[u8; 3]> {
    let index = crate::editing::ELEMENTS
        .iter()
        .position(|&name| name == element)?;
    let color = *JMOL.get(index)?;
    Some([(color >> 16) as u8, (color >> 8) as u8, color as u8])
}

/// Preserve Jmol's hue and relative lightness while reducing HSL saturation.
/// Dark variants lift toward light tints rather than mixing colors with black.
pub(super) fn soften(rgb: [u8; 3], canvas: CanvasTheme, pastel: bool) -> [u8; 3] {
    let values = rgb.map(|v| f64::from(v) / 255.);
    let low = values.into_iter().fold(1., f64::min);
    let high = values.into_iter().fold(0., f64::max);
    let chroma = high - low;
    let lightness = (high + low) / 2.;
    // Saturation scale, midpoint lightness, retained source-lightness range.
    let (saturation_scale, midpoint, range) = match (pastel, canvas) {
        (false, CanvasTheme::Light) => (0.50, 0.48, 0.32),
        (false, CanvasTheme::Dark) => (0.60, 0.70, 0.20),
        (true, CanvasTheme::Light) => (0.32, 0.55, 0.22),
        (true, CanvasTheme::Dark) => (0.50, 0.80, 0.15),
    };
    let toned_lightness = midpoint + (lightness - 0.5) * range;
    if chroma == 0. {
        return [(toned_lightness * 255.).round() as u8; 3];
    }
    let saturation = chroma / (1. - (2. * lightness - 1.).abs());
    let toned_chroma = (1. - (2. * toned_lightness - 1.).abs()) * saturation * saturation_scale;
    values.map(|v| {
        let value = toned_lightness + ((v - low) / chroma - 0.5) * toned_chroma;
        (value * 255.).round().clamp(0., 255.) as u8
    })
}

/// Keep the published hue while adjusting luminance only when necessary for
/// small 2D labels. The 3.2:1 target leaves room for 8-bit rounding above 3:1.
pub(super) fn label_ink(rgb: [u8; 3], canvas: CanvasTheme) -> [u8; 3] {
    let linear = rgb.map(|value| {
        let value = f64::from(value) / 255.;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });
    let luminance = linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722;
    let target = if canvas.is_dark() {
        0.05 * 3.2 - 0.05
    } else {
        1.05 / 3.2 - 0.05
    };
    if (canvas.is_dark() && luminance >= target) || (canvas.is_light() && luminance <= target) {
        return rgb;
    }
    linear.map(|value| {
        let adjusted = if canvas.is_dark() {
            value + (1. - value) * (target - luminance) / (1. - luminance)
        } else {
            value * target / luminance
        };
        let encoded = if adjusted <= 0.0031308 {
            adjusted * 12.92
        } else {
            1.055 * adjusted.powf(1. / 2.4) - 0.055
        };
        (encoded * 255.).round().clamp(0., 255.) as u8
    })
}
