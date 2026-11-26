//! Stored parameters for a placed, editable infrared spectrum figure.
use crate::style::DrawingStyle;
pub use reshiki_qcdata::vibrational::BandShape;
use serde::{Deserialize, Serialize};

const MAX_NAME_CHARS: usize = 200;
const MAX_METHOD_CHARS: usize = 100;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum YAxis {
    #[default]
    Transmittance,
    Absorbance,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frame {
    #[default]
    Box,
    Axes,
    Baseline,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Journal {
    #[default]
    Acs,
    Rsc,
    Wiley,
}

impl Journal {
    pub fn for_style(style: &DrawingStyle) -> Self {
        let name = style.name.to_ascii_uppercase();
        if name.contains("RSC") {
            Self::Rsc
        } else if name.contains("WILEY") {
            Self::Wiley
        } else {
            Self::Acs
        }
    }

    /// Font family, label size, tick size and line width, in points.
    pub const fn metrics(self) -> (&'static str, f32, f32, f32) {
        match self {
            Self::Acs => ("Arial", 8., 7., 0.75),
            Self::Rsc => ("Arial", 7., 6.5, 0.6),
            Self::Wiley => ("Times New Roman", 8., 7., 0.75),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Columns {
    #[default]
    One,
    Two,
}

impl Columns {
    pub const fn size_pt(self) -> (f32, f32) {
        match self {
            Self::One => (234., 150.),
            Self::Two => (504., 230.),
        }
    }
}

/// The original bands and presentation choices needed to rebuild an IR figure.
/// Validation takes place when reading stored parameters, not while rendering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "IrFigureFields")]
pub struct IrFigure {
    pub name: String,
    pub method: String,
    /// Unscaled wavenumbers in cm⁻¹ and intensities in km/mol.
    pub bands: Vec<[f64; 2]>,
    pub y: YAxis,
    pub from: f64,
    pub to: f64,
    pub minor_ticks: bool,
    pub frame: Frame,
    pub shape: BandShape,
    pub fwhm: f64,
    pub scale: f64,
    pub labels: u8,
    pub vertical_labels: bool,
    /// Unscaled, rounded wavenumbers of hidden labels in cm⁻¹.
    pub hidden_labels: Vec<u32>,
    pub method_line: bool,
    pub journal: Journal,
    pub columns: Columns,
}

impl Default for IrFigure {
    fn default() -> Self {
        Self::with_defaults(String::new(), String::new(), Vec::new(), Journal::default())
    }
}

impl IrFigure {
    pub const FWHM_RANGE: std::ops::RangeInclusive<f64> = 1. ..=200.;
    pub const SCALE_RANGE: std::ops::RangeInclusive<f64> = 0.5..=1.5;
    pub const MAX_BANDS: usize = 3000;
    pub const MAX_LABELS: u8 = 10;
    pub const MAX_HIDDEN_LABELS: usize = 64;

    /// Construct parameters from bands already checked by the calculation boundary.
    pub fn with_defaults(
        mut name: String,
        mut method: String,
        bands: Vec<[f64; 2]>,
        journal: Journal,
    ) -> Self {
        for (text, limit) in [(&mut name, MAX_NAME_CHARS), (&mut method, MAX_METHOD_CHARS)] {
            if let Some((end, _)) = text.char_indices().nth(limit) {
                text.truncate(end);
            }
        }
        Self {
            name,
            method,
            bands,
            y: YAxis::default(),
            from: 4000.,
            to: 400.,
            minor_ticks: true,
            frame: Frame::default(),
            shape: BandShape::default(),
            fwhm: 20.,
            scale: 1.,
            labels: 8,
            vertical_labels: false,
            hidden_labels: Vec::new(),
            method_line: false,
            journal,
            columns: Columns::default(),
        }
    }

    /// A descending IR axis spanning at least 200 cm⁻¹ within 0–10,000 cm⁻¹.
    pub fn valid_range(from: f64, to: f64) -> bool {
        from.is_finite()
            && to.is_finite()
            && (0. ..=10_000.).contains(&from)
            && (0. ..=10_000.).contains(&to)
            && from - to >= 200.
    }

    pub fn fwhm_text(&self) -> String {
        if self.fwhm.fract() == 0. {
            format!("{:.0}", self.fwhm)
        } else {
            format!("{:.1}", self.fwhm)
        }
    }

    pub fn caption(&self) -> String {
        let name = self.name.trim();
        let name = if name.is_empty() {
            "the molecule"
        } else {
            name
        };
        let shape = match self.shape {
            BandShape::Lorentzian => "Lorentzian",
            BandShape::Gaussian => "Gaussian",
        };
        let scaling = if self.scale == 1. {
            "unscaled frequencies".into()
        } else {
            format!("frequencies scaled by {:.3}", self.scale)
        };
        let relative = match self.y {
            YAxis::Transmittance => " Transmittance is relative.",
            YAxis::Absorbance => "",
        };
        format!(
            "Calculated IR spectrum of {name} ({}, {shape} broadening, FWHM {} cm⁻¹, {scaling}).{relative}",
            self.method,
            self.fwhm_text(),
        )
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct IrFigureFields {
    name: String,
    method: String,
    bands: Vec<[f64; 2]>,
    y: YAxis,
    from: f64,
    to: f64,
    minor_ticks: bool,
    frame: Frame,
    shape: BandShape,
    fwhm: f64,
    scale: f64,
    labels: u8,
    vertical_labels: bool,
    hidden_labels: Vec<u32>,
    method_line: bool,
    journal: Journal,
    columns: Columns,
}

impl Default for IrFigureFields {
    fn default() -> Self {
        let IrFigure {
            name,
            method,
            bands,
            y,
            from,
            to,
            minor_ticks,
            frame,
            shape,
            fwhm,
            scale,
            labels,
            vertical_labels,
            hidden_labels,
            method_line,
            journal,
            columns,
        } = IrFigure::default();
        Self {
            name,
            method,
            bands,
            y,
            from,
            to,
            minor_ticks,
            frame,
            shape,
            fwhm,
            scale,
            labels,
            vertical_labels,
            hidden_labels,
            method_line,
            journal,
            columns,
        }
    }
}

impl TryFrom<IrFigureFields> for IrFigure {
    type Error = String;

    fn try_from(fields: IrFigureFields) -> Result<Self, Self::Error> {
        if fields.bands.len() > Self::MAX_BANDS {
            return Err(format!(
                "IR figures may contain at most {} bands",
                Self::MAX_BANDS
            ));
        }
        if fields.hidden_labels.len() > Self::MAX_HIDDEN_LABELS {
            return Err("IR figures may contain at most 64 hidden labels".into());
        }
        if fields.name.chars().count() > MAX_NAME_CHARS {
            return Err(format!(
                "IR figure names may contain at most {MAX_NAME_CHARS} characters"
            ));
        }
        if fields.method.chars().count() > MAX_METHOD_CHARS {
            return Err(format!(
                "IR figure methods may contain at most {MAX_METHOD_CHARS} characters"
            ));
        }
        if fields
            .bands
            .iter()
            .flatten()
            .any(|value| !value.is_finite() || *value < 0.)
        {
            return Err(
                "IR bands must contain finite, nonnegative frequencies and intensities".into(),
            );
        }
        if !Self::valid_range(fields.from, fields.to) {
            return Err("IR range must descend by at least 200 cm⁻¹ within 0–10000 cm⁻¹".into());
        }
        if !Self::FWHM_RANGE.contains(&fields.fwhm) {
            return Err("IR FWHM must be between 1 and 200 cm⁻¹".into());
        }
        if !Self::SCALE_RANGE.contains(&fields.scale) {
            return Err("IR frequency scale must be between 0.5 and 1.5".into());
        }
        if fields.labels > Self::MAX_LABELS {
            return Err("IR label count must be between 0 and 10".into());
        }
        Ok(Self {
            name: fields.name,
            method: fields.method,
            bands: fields.bands,
            y: fields.y,
            from: fields.from,
            to: fields.to,
            minor_ticks: fields.minor_ticks,
            frame: fields.frame,
            shape: fields.shape,
            fwhm: fields.fwhm,
            scale: fields.scale,
            labels: fields.labels,
            vertical_labels: fields.vertical_labels,
            hidden_labels: fields.hidden_labels,
            method_line: fields.method_line,
            journal: fields.journal,
            columns: fields.columns,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_value, json, to_value};

    #[test]
    fn stored_parameters_round_trip_and_missing_fields_use_defaults() {
        let value = json!({
            "name": "acetic acid", "method": "GFN2-xTB", "bands": [[1700., 120.], [2900., 4.]],
            "y": "absorbance", "from": 3800., "to": 600., "minor_ticks": false,
            "frame": "axes", "shape": "gaussian", "fwhm": 12.5, "scale": 0.96,
            "labels": 3, "vertical_labels": true, "hidden_labels": [1, 7],
            "method_line": true, "journal": "rsc", "columns": "two"
        });
        let figure = from_value::<IrFigure>(value.clone()).unwrap();
        assert_eq!(to_value(&figure).unwrap(), value);
        assert_eq!(
            from_value::<IrFigure>(to_value(&figure).unwrap()).unwrap(),
            figure
        );
        assert_eq!(
            from_value::<IrFigure>(json!({})).unwrap(),
            IrFigure::default()
        );
        assert_eq!(
            from_value::<IrFigure>(json!({"name":"water", "journal":"wiley"})).unwrap(),
            IrFigure::with_defaults("water".into(), String::new(), vec![], Journal::Wiley)
        );
        let defaults = to_value(IrFigure::default()).unwrap();
        assert_eq!(defaults["y"], "transmittance");
        assert_eq!(defaults["frame"], "box");
        assert_eq!(defaults["shape"], "lorentzian");
        assert_eq!(defaults["journal"], "acs");
        assert_eq!(defaults["columns"], "one");
        assert_eq!(defaults["labels"], 8);
        assert_eq!(to_value(Frame::Baseline).unwrap(), "baseline");
    }

    #[test]
    fn invalid_stored_parameters_are_rejected() {
        for value in [
            json!({"bands":[[-1., 2.]]}),
            json!({"bands":[[1000., -1.]]}),
            json!({"from":10001.}),
            json!({"to":-1.}),
            json!({"from":400., "to":4000.}),
            json!({"from":599., "to":400.}),
            json!({"fwhm":0.99}),
            json!({"fwhm":200.01}),
            json!({"scale":0.49}),
            json!({"scale":1.51}),
            json!({"labels":11}),
        ] {
            assert!(from_value::<IrFigure>(value.clone()).is_err(), "{value}");
        }
        for value in [
            json!({"bands":[[0., 0.]], "from":200., "to":0., "fwhm":1., "scale":0.5, "labels":0}),
            json!({"from":10000., "to":9800., "fwhm":200., "scale":1.5, "labels":10}),
        ] {
            from_value::<IrFigure>(value).unwrap();
        }
    }

    #[test]
    fn stored_metadata_sizes_are_bounded_and_text_limits_count_unicode_characters() {
        let value = json!({
            "name": "名".repeat(200), "method": "法".repeat(100),
            "bands": vec![[1000., 1.]; 3000], "hidden_labels": vec![0_u32; 64]
        });
        from_value::<IrFigure>(value.clone()).unwrap();
        for (field, oversized) in [
            ("name", json!("名".repeat(201))),
            ("method", json!("法".repeat(101))),
            ("bands", json!(vec![[1000., 1.]; 3001])),
            ("hidden_labels", json!(vec![0_u32; 65])),
        ] {
            let mut invalid = value.clone();
            invalid[field] = oversized;
            assert!(from_value::<IrFigure>(invalid).is_err(), "{field}");
        }
    }

    #[test]
    fn actual_nonfinite_serde_numbers_are_rejected() {
        use serde::de::value::{Error, MapDeserializer};
        for key in ["from", "to", "fwhm", "scale"] {
            for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let deserializer = MapDeserializer::<_, Error>::new([(key, value)].into_iter());
                assert!(
                    IrFigure::deserialize(deserializer).is_err(),
                    "{key}: {value}"
                );
            }
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for band in [[value, 1.], [1000., value]] {
                let fields = IrFigureFields {
                    bands: vec![band],
                    ..Default::default()
                };
                assert!(IrFigure::try_from(fields).is_err());
            }
        }
    }

    #[test]
    fn captions_describe_default_scaled_and_absorbance_figures() {
        let mut figure = IrFigure::with_defaults(
            "acetic acid".into(),
            "GFN2-xTB".into(),
            vec![[1700., 120.]],
            Journal::Acs,
        );
        assert_eq!(
            figure.caption(),
            "Calculated IR spectrum of acetic acid (GFN2-xTB, Lorentzian broadening, FWHM 20 cm⁻¹, unscaled frequencies). Transmittance is relative."
        );
        figure.name.clear();
        figure.shape = BandShape::Gaussian;
        figure.fwhm = 12.5;
        figure.scale = 0.96;
        assert_eq!(
            figure.caption(),
            "Calculated IR spectrum of the molecule (GFN2-xTB, Gaussian broadening, FWHM 12.5 cm⁻¹, frequencies scaled by 0.960). Transmittance is relative."
        );
        figure.y = YAxis::Absorbance;
        assert_eq!(
            figure.caption(),
            "Calculated IR spectrum of the molecule (GFN2-xTB, Gaussian broadening, FWHM 12.5 cm⁻¹, frequencies scaled by 0.960)."
        );
    }

    #[test]
    fn journal_presets_follow_the_drawing_style() {
        let mut style = DrawingStyle::default();
        for (name, expected) in [
            ("JACS / ACS", Journal::Acs),
            ("RSC", Journal::Rsc),
            ("wiley", Journal::Wiley),
            ("Custom", Journal::Acs),
        ] {
            style.name = name.into();
            assert_eq!(Journal::for_style(&style), expected);
        }
        assert_eq!(Journal::Acs.metrics(), ("Arial", 8., 7., 0.75));
        assert_eq!(Journal::Rsc.metrics(), ("Arial", 7., 6.5, 0.6));
        assert_eq!(Journal::Wiley.metrics(), ("Times New Roman", 8., 7., 0.75));
        assert_eq!(Columns::One.size_pt(), (234., 150.));
        assert_eq!(Columns::Two.size_pt(), (504., 230.));
    }
}
