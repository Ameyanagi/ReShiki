use super::Look;
use reshiki_qcdata::Spin;
use serde::{Deserialize, Deserializer, Serialize};

/// An orbital placed on a 3D model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Figure {
    pub orbital: OrbitalRef,
    /// Isovalue in bohr⁻³⁄².
    #[serde(deserialize_with = "isovalue")]
    pub iso: f32,
    #[serde(flatten)]
    pub look: Look,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Snapshot>,
}

/// A zero-based orbital index in a stored result's spin spectrum.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrbitalRef {
    pub result: String,
    pub spin: Spin,
    pub index: u32,
}

/// The saved vector surface for one view of the model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub view: String,
    pub reach: f32,
    pub shapes: String,
}

fn isovalue<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    let iso = f32::deserialize(d)?;
    if (0.005..=0.2).contains(&iso) {
        Ok(iso)
    } else {
        Err(serde::de::Error::custom(
            "Orbital isovalue must be between 0.005 and 0.2",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_value, json, to_value};

    #[test]
    fn round_trip_with_flattened_look() {
        let value = json!({
            "orbital": {"result": "0123456789abcdef", "spin": "beta", "index": 3},
            "iso": 0.05,
            "fill": "shaded",
            "mesh": true,
            "opacity": 70,
            "snapshot": {"view": "1234", "reach": 2.0, "shapes": "test"}
        });
        let figure = from_value::<Figure>(value.clone()).unwrap();
        assert_eq!(figure.orbital.spin, Spin::Beta);
        assert_eq!(figure.look.opacity, 70);
        let serialized = to_value(&figure).unwrap();
        assert_eq!(serialized["fill"], "shaded");
        assert!(serialized.get("look").is_none());
        assert_eq!(from_value::<Figure>(serialized).unwrap(), figure);
        let mut extra = value;
        extra["unexpected"] = json!(1);
        assert!(from_value::<Figure>(extra).is_err());
    }

    #[test]
    fn isovalue_is_finite_and_bounded() {
        let mut value = json!({
            "orbital": {"result": "0123456789abcdef", "spin": "alpha", "index": 0},
            "iso": 0.05
        });
        for iso in [0.005, 0.2] {
            value["iso"] = json!(iso);
            assert!(from_value::<Figure>(value.clone()).is_ok());
        }
        for iso in [0., 0.0049, 0.201, f64::NAN, f64::INFINITY] {
            value["iso"] = json!(iso);
            assert!(from_value::<Figure>(value.clone()).is_err(), "{iso}");
        }
    }
}
