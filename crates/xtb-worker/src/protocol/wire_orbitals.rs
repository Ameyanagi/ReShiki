//! Orbital metadata on the wire. Coefficients travel as chunks.

use reshiki_qcdata::{BasisSet, Beta, OrbitalSet, Orbitals};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

#[derive(Serialize, Deserialize)]
#[serde(remote = "Orbitals", deny_unknown_fields)]
struct Metadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    basis: Option<BasisSet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    core_electrons: Vec<u32>,
    #[serde(with = "Levels")]
    alpha: OrbitalSet,
    #[serde(with = "BetaLevels")]
    beta: Beta,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "OrbitalSet", deny_unknown_fields)]
struct Levels {
    energies_hartree: Vec<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    occupations: Vec<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    symmetries: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rows: Vec<u32>,
    #[serde(
        default,
        serialize_with = "omit_coefficients",
        deserialize_with = "empty_coefficients"
    )]
    coefficients: Vec<f64>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "Beta", rename_all = "snake_case", deny_unknown_fields)]
enum BetaLevels {
    Restricted,
    SameOrbitals {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        occupations: Vec<f64>,
    },
    Unrestricted(#[serde(with = "Levels")] OrbitalSet),
}

fn omit_coefficients<S: Serializer>(_: &[f64], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str("")
}

fn empty_coefficients<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<f64>, D::Error> {
    if String::deserialize(deserializer)?.is_empty() {
        Ok(vec![])
    } else {
        Err(de::Error::custom(
            "orbital coefficients must arrive as chunks",
        ))
    }
}

pub(super) fn serialize<S: Serializer>(
    orbitals: &Option<Orbitals>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct Wire<'a>(#[serde(with = "Metadata")] &'a Orbitals);
    orbitals.as_ref().map(Wire).serialize(serializer)
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Orbitals>, D::Error> {
    #[derive(Deserialize)]
    struct Wire(#[serde(with = "Metadata")] Orbitals);
    Ok(Option::<Wire>::deserialize(deserializer)?.map(|wire| wire.0))
}
