use std::{collections::HashSet, fmt, mem::size_of_val};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::{Error, MAX_FRAME_ATOMS, MAX_FRAMES, MAX_TRAJECTORY_ATOMS};

/// One exact molecular geometry and its electronic state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// Atomic numbers, from 1 to 118.
    pub numbers: Vec<u8>,
    /// Cartesian positions in ångströms, one xyz triplet per atom.
    pub positions_angstrom: Vec<[f64; 3]>,
    /// Mass numbers per atom, with zero for natural abundance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub isotopes: Vec<u16>,
    /// Explicit masses per atom in unified atomic mass units.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masses_amu: Vec<f64>,
    /// Total charge in elementary charge units.
    pub charge: i32,
    /// Spin multiplicity, 2S + 1.
    pub multiplicity: u32,
    /// Drawing atom IDs at creation, if recorded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawing_atoms: Vec<Option<u64>>,
}

/// SHA-256 geometry identity, serialized as 64 lowercase hexadecimal digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameHash(
    /// The 32 bytes of the SHA-256 digest.
    pub [u8; 32],
);

impl fmt::Display for FrameHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl Serialize for FrameHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for FrameHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let hex = String::deserialize(deserializer)?;
        if hex.len() != 64
            || !hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(serde::de::Error::custom(
                "expected 64 lowercase hexadecimal digits",
            ));
        }
        let mut bytes = [0; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte =
                u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(serde::de::Error::custom)?;
        }
        Ok(Self(bytes))
    }
}

impl Frame {
    /// Hash the exact scientific frame, ignoring drawing IDs.
    pub fn hash(&self) -> FrameHash {
        let mut digest = Sha256::new();
        digest.update(b"reshiki-qcdata frame 1\0");
        digest.update((self.numbers.len() as u64).to_le_bytes());
        digest.update(&self.numbers);
        for i in 0..self.numbers.len() {
            digest.update(self.isotopes.get(i).copied().unwrap_or(0).to_le_bytes());
        }
        digest.update([u8::from(!self.masses_amu.is_empty())]);
        for mass in &self.masses_amu {
            digest.update(mass.to_le_bytes());
        }
        for coordinate in self.positions_angstrom.iter().flatten() {
            let coordinate = if *coordinate == 0.0 { 0.0 } else { *coordinate };
            digest.update(coordinate.to_le_bytes());
        }
        digest.update(self.charge.to_le_bytes());
        digest.update(self.multiplicity.to_le_bytes());
        FrameHash(digest.finalize().into())
    }

    /// Return the number of electrons, which can be negative on an invalid frame.
    pub fn electrons(&self) -> i64 {
        electrons(&self.numbers, self.charge)
    }

    /// Check atom and spin metadata, finite positions, drawing IDs, and size limits.
    pub fn validate(&self) -> Result<(), Error> {
        validate_atoms(
            &self.numbers,
            &self.isotopes,
            &self.masses_amu,
            self.charge,
            self.multiplicity,
        )?;
        if self.positions_angstrom.len() != self.numbers.len()
            || self
                .positions_angstrom
                .iter()
                .flatten()
                .any(|x| !x.is_finite())
        {
            return Err(Error::Invalid("invalid frame positions".into()));
        }
        if !self.drawing_atoms.is_empty() && self.drawing_atoms.len() != self.numbers.len() {
            return Err(Error::Invalid(
                "drawing atom count differs from frame".into(),
            ));
        }
        let mut ids = HashSet::new();
        if self
            .drawing_atoms
            .iter()
            .flatten()
            .any(|id| !ids.insert(*id))
        {
            return Err(Error::Invalid("duplicate drawing atom ID".into()));
        }
        Ok(())
    }

    /// Estimate the bytes owned by the frame's arrays.
    pub fn heap_bytes(&self) -> usize {
        size_of_val(self.numbers.as_slice())
            + size_of_val(self.positions_angstrom.as_slice())
            + size_of_val(self.isotopes.as_slice())
            + size_of_val(self.masses_amu.as_slice())
            + size_of_val(self.drawing_atoms.as_slice())
    }
}

fn electrons(numbers: &[u8], charge: i32) -> i64 {
    numbers.iter().map(|&z| i64::from(z)).sum::<i64>() - i64::from(charge)
}

fn validate_atoms(
    numbers: &[u8],
    isotopes: &[u16],
    masses_amu: &[f64],
    charge: i32,
    multiplicity: u32,
) -> Result<(), Error> {
    let n = numbers.len();
    if n > MAX_FRAME_ATOMS {
        return Err(Error::TooLarge {
            what: "frame atoms",
            count: n,
            max: MAX_FRAME_ATOMS,
        });
    }
    if numbers.iter().any(|z| !(1..=118).contains(z)) {
        return Err(Error::Invalid("atomic number outside 1–118".into()));
    }
    if !isotopes.is_empty() && isotopes.len() != n {
        return Err(Error::Invalid(
            "isotope count differs from atom count".into(),
        ));
    }
    if (!masses_amu.is_empty() && masses_amu.len() != n)
        || masses_amu.iter().any(|m| !m.is_finite() || *m <= 0.0)
    {
        return Err(Error::Invalid("invalid explicit atomic masses".into()));
    }
    let n_electrons = electrons(numbers, charge);
    let unpaired = i64::from(multiplicity) - 1;
    if unpaired < 0 || n_electrons < unpaired || (n_electrons - unpaired) % 2 != 0 {
        return Err(Error::Invalid(
            "electron count and spin multiplicity disagree".into(),
        ));
    }
    Ok(())
}

/// A path of geometries with common atoms and electronic state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trajectory {
    /// Atomic numbers, from 1 to 118.
    pub numbers: Vec<u8>,
    /// Mass numbers per atom, with zero for natural abundance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub isotopes: Vec<u16>,
    /// Explicit masses per atom in unified atomic mass units.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masses_amu: Vec<f64>,
    /// Total charge in elementary charge units.
    pub charge: i32,
    /// Spin multiplicity, 2S + 1.
    pub multiplicity: u32,
    /// Step-major Cartesian positions in ångströms, with xyz triplets in atom order.
    #[serde(
        with = "crate::compact::f32",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub positions_angstrom: Vec<f64>,
    /// Energies in hartree, if recorded for each step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub energies_hartree: Vec<f64>,
    /// Labels, if recorded for each step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// Starting minimizer energies in hartree, if recorded for each conformer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minimizer_energies_hartree: Vec<f64>,
    /// Number of starts in each conformer's cluster, if recorded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cluster_sizes: Vec<u32>,
}

impl Trajectory {
    /// Return the step count, using the other step arrays when positions were omitted.
    pub fn steps(&self) -> usize {
        if self.positions_angstrom.is_empty() {
            self.energies_hartree
                .len()
                .max(self.labels.len())
                .max(self.minimizer_energies_hartree.len())
                .max(self.cluster_sizes.len())
        } else {
            self.positions_angstrom
                .len()
                .checked_div(self.numbers.len() * 3)
                .unwrap_or(0)
        }
    }

    /// Reconstruct the frame at the zero-based step index if its positions are available.
    pub fn frame(&self, i: usize) -> Option<Frame> {
        let width = self.numbers.len() * 3;
        if self.positions_angstrom.is_empty() || i >= self.steps() {
            return None;
        }
        let start = i * width;
        let positions_angstrom = self.positions_angstrom[start..start + width]
            .as_chunks::<3>()
            .0
            .to_vec();
        Some(Frame {
            numbers: self.numbers.clone(),
            positions_angstrom,
            isotopes: self.isotopes.clone(),
            masses_amu: self.masses_amu.clone(),
            charge: self.charge,
            multiplicity: self.multiplicity,
            drawing_atoms: Vec::new(),
        })
    }

    /// Check shared atom metadata, step arrays, and trajectory size limits.
    pub fn validate(&self) -> Result<(), Error> {
        validate_atoms(
            &self.numbers,
            &self.isotopes,
            &self.masses_amu,
            self.charge,
            self.multiplicity,
        )?;
        let width = self.numbers.len() * 3;
        if !self.positions_angstrom.is_empty()
            && (width == 0 || !self.positions_angstrom.len().is_multiple_of(width))
        {
            return Err(Error::Invalid("incomplete trajectory position step".into()));
        }
        let steps = self.steps();
        if steps > MAX_FRAMES {
            return Err(Error::TooLarge {
                what: "trajectory steps",
                count: steps,
                max: MAX_FRAMES,
            });
        }
        let atoms = self.numbers.len() * steps;
        if atoms > MAX_TRAJECTORY_ATOMS {
            return Err(Error::TooLarge {
                what: "atoms across trajectory steps",
                count: atoms,
                max: MAX_TRAJECTORY_ATOMS,
            });
        }
        if self
            .positions_angstrom
            .iter()
            .any(|&x| !(x as f32).is_finite())
            || (!self.energies_hartree.is_empty() && self.energies_hartree.len() != steps)
            || self.energies_hartree.iter().any(|x| !x.is_finite())
            || (!self.labels.is_empty() && self.labels.len() != steps)
        {
            return Err(Error::Invalid(
                "invalid trajectory positions, energies, or labels".into(),
            ));
        }
        if (!self.minimizer_energies_hartree.is_empty()
            && self.minimizer_energies_hartree.len() != steps)
            || self
                .minimizer_energies_hartree
                .iter()
                .any(|x| !x.is_finite())
        {
            return Err(Error::Invalid(
                "invalid trajectory minimizer energies".into(),
            ));
        }
        if (!self.cluster_sizes.is_empty() && self.cluster_sizes.len() != steps)
            || self.cluster_sizes.contains(&0)
        {
            return Err(Error::Invalid("invalid trajectory cluster sizes".into()));
        }
        Ok(())
    }

    /// Estimate the bytes owned by trajectory arrays and labels.
    pub fn heap_bytes(&self) -> usize {
        size_of_val(self.numbers.as_slice())
            + size_of_val(self.isotopes.as_slice())
            + size_of_val(self.masses_amu.as_slice())
            + size_of_val(self.positions_angstrom.as_slice())
            + size_of_val(self.energies_hartree.as_slice())
            + size_of_val(self.labels.as_slice())
            + self.labels.iter().map(String::len).sum::<usize>()
            + size_of_val(self.minimizer_energies_hartree.as_slice())
            + size_of_val(self.cluster_sizes.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn water() -> Frame {
        Frame {
            numbers: vec![8, 1, 1],
            positions_angstrom: vec![
                [0.0, 0.0, 0.0],
                [0.9572, 0.0, 0.0],
                [-0.2399872, 0.9266272, 0.0],
            ],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        }
    }

    #[test]
    fn water_and_oh_have_valid_electron_counts() {
        let mut frame = water();
        frame.validate().unwrap();
        assert_eq!(frame.electrons(), 10);
        frame.numbers.pop();
        frame.positions_angstrom.pop();
        frame.multiplicity = 2;
        frame.validate().unwrap();
        assert_eq!(frame.electrons(), 9);
    }

    #[test]
    fn invalid_frame_arrays_and_drawing_ids() {
        let mut frame = water();
        frame.positions_angstrom.pop();
        assert!(frame.validate().is_err());
        frame = water();
        frame.isotopes = vec![16, 1];
        assert!(frame.validate().is_err());
        frame.isotopes.clear();
        frame.masses_amu = vec![16.0, f64::INFINITY, 1.0];
        assert!(frame.validate().is_err());
        frame.masses_amu[1] = 0.0;
        assert!(frame.validate().is_err());
        frame.masses_amu.clear();
        frame.positions_angstrom[1][0] = f64::NAN;
        assert!(frame.validate().is_err());
        frame = water();
        frame.drawing_atoms = vec![Some(3), Some(3), None];
        assert!(frame.validate().is_err());
        frame.drawing_atoms = vec![Some(3), None, None];
        frame.validate().unwrap();
    }

    #[test]
    fn invalid_atomic_numbers_and_spin_states() {
        for z in [0, 119] {
            let mut frame = water();
            frame.numbers[0] = z;
            assert!(frame.validate().is_err());
        }
        let mut frame = water();
        frame.multiplicity = 2;
        assert!(frame.validate().is_err());
        frame.multiplicity = 0;
        assert!(frame.validate().is_err());
        frame = water();
        frame.charge = 11;
        assert!(frame.validate().is_err());
        frame.numbers = vec![1];
        frame.positions_angstrom.truncate(1);
        frame.charge = 0;
        frame.multiplicity = 4;
        assert!(frame.validate().is_err());
    }

    #[test]
    fn frame_atom_limit() {
        let frame = Frame {
            numbers: vec![2; MAX_FRAME_ATOMS + 1],
            positions_angstrom: vec![[0.0; 3]; MAX_FRAME_ATOMS + 1],
            ..water()
        };
        assert!(matches!(frame.validate(), Err(Error::TooLarge { .. })));
    }

    #[test]
    fn frame_hash_golden_and_hex_serde() {
        let hash = water().hash();
        let hex = "b2309becad0c41f063deb724f828b3eb45ecaaaf18da32f90aa0ca0aa2276725";
        assert_eq!(hash.to_string(), hex);
        let json = serde_json::to_string(&hash).unwrap();
        assert_eq!(json, format!("\"{hex}\""));
        assert_eq!(serde_json::from_str::<FrameHash>(&json).unwrap(), hash);
        assert!(serde_json::from_str::<FrameHash>(&json.to_ascii_uppercase()).is_err());
        assert!(serde_json::from_str::<FrameHash>("\"00\"").is_err());
    }

    #[test]
    fn hash_ignores_drawing_ids_negative_zero_and_explicit_natural_isotopes() {
        let frame = water();
        assert_eq!(frame.hash(), frame.clone().hash());
        let mut copy = frame.clone();
        copy.drawing_atoms = vec![Some(17), None, Some(9)];
        copy.positions_angstrom[0][0] = -0.0;
        copy.isotopes = vec![0; 3];
        assert_eq!(frame.hash(), copy.hash());
    }

    #[test]
    fn hash_changes_for_exact_coordinates_and_atom_metadata() {
        let frame = water();
        let hash = frame.hash();
        let mut changed = frame.clone();
        changed.positions_angstrom[1][0] = changed.positions_angstrom[1][0].next_up();
        assert_ne!(hash, changed.hash());
        changed = frame.clone();
        changed.charge = 2;
        assert_ne!(hash, changed.hash());
        changed = frame.clone();
        changed.multiplicity = 3;
        assert_ne!(hash, changed.hash());
        changed = frame.clone();
        changed.isotopes = vec![16, 2, 1];
        assert_ne!(hash, changed.hash());
        changed = frame;
        changed.masses_amu = vec![16.0, 1.0, 1.0];
        let with_masses = changed.hash();
        assert_ne!(hash, with_masses);
        changed.masses_amu[0] = 17.0;
        assert_ne!(with_masses, changed.hash());
    }

    fn trajectory() -> Trajectory {
        Trajectory {
            numbers: vec![8, 1, 1],
            isotopes: vec![16, 2, 1],
            masses_amu: vec![16.0, 2.0, 1.0],
            charge: 0,
            multiplicity: 1,
            positions_angstrom: (0..18).map(f64::from).collect(),
            energies_hartree: vec![-10.0, -11.0],
            labels: vec!["first".into(), "second".into()],
            minimizer_energies_hartree: vec![],
            cluster_sizes: vec![],
        }
    }

    #[test]
    fn trajectory_frames_restore_atom_metadata() {
        let path = trajectory();
        path.validate().unwrap();
        assert_eq!(path.steps(), 2);
        let frame = path.frame(1).unwrap();
        assert_eq!(frame.isotopes, path.isotopes);
        assert_eq!(frame.masses_amu, path.masses_amu);
        assert_eq!(frame.positions_angstrom[0], [9.0, 10.0, 11.0]);
        frame.validate().unwrap();
        assert!(path.frame(2).is_none());
    }

    #[test]
    fn trajectory_light_steps_and_mismatched_arrays() {
        let mut path = trajectory();
        path.energies_hartree.pop();
        assert!(path.validate().is_err());
        path = trajectory();
        path.labels.pop();
        assert!(path.validate().is_err());
        path = trajectory();
        path.positions_angstrom.pop();
        assert!(path.validate().is_err());
        path.positions_angstrom.clear();
        path.validate().unwrap();
        assert_eq!(path.steps(), 2);
        assert!(path.frame(0).is_none());
        path.energies_hartree.clear();
        assert_eq!(path.steps(), 2);
        path.energies_hartree.push(0.0);
        assert!(path.validate().is_err());
    }

    #[test]
    fn trajectory_conformer_metadata_counts_steps_and_validates() {
        let mut path = trajectory();
        path.minimizer_energies_hartree = vec![-0.5];
        assert!(path.validate().is_err());
        path.minimizer_energies_hartree.push(-0.25);
        path.cluster_sizes = vec![3];
        assert!(path.validate().is_err());
        path.cluster_sizes.push(1);
        path.validate().unwrap();

        path.positions_angstrom.clear();
        path.energies_hartree.clear();
        path.labels.clear();
        path.minimizer_energies_hartree[1] = f64::INFINITY;
        assert!(path.validate().is_err());
        path.minimizer_energies_hartree.clear();
        assert_eq!(path.steps(), 2);
        assert!(path.frame(0).is_none());
        path.validate().unwrap();
        path.cluster_sizes[1] = 0;
        assert!(path.validate().is_err());
        path.cluster_sizes.clear();
        path.minimizer_energies_hartree = vec![-0.5, -0.25];
        assert_eq!(path.steps(), 2);
        path.validate().unwrap();
    }

    #[test]
    fn trajectory_conformer_metadata_uses_plain_json_and_counts_heap_bytes() {
        let mut path = trajectory();
        path.positions_angstrom.clear();
        let legacy = serde_json::to_value(&path).unwrap();
        assert!(legacy.get("minimizer_energies_hartree").is_none());
        assert!(legacy.get("cluster_sizes").is_none());
        assert_eq!(serde_json::from_value::<Trajectory>(legacy).unwrap(), path);

        let original_bytes = path.heap_bytes();
        path.minimizer_energies_hartree = vec![-0.123456789, -0.25];
        path.cluster_sizes = vec![3, 1];
        assert_eq!(path.heap_bytes() - original_bytes, 2 * (8 + 4));
        let json = serde_json::to_value(&path).unwrap();
        assert_eq!(
            json["minimizer_energies_hartree"],
            serde_json::json!([-0.123456789, -0.25])
        );
        assert_eq!(json["cluster_sizes"], serde_json::json!([3, 1]));
        assert_eq!(serde_json::from_value::<Trajectory>(json).unwrap(), path);
    }

    #[test]
    fn trajectory_step_and_total_atom_limits() {
        let mut path = trajectory();
        path.positions_angstrom.clear();
        path.labels.clear();
        path.energies_hartree = vec![0.0; MAX_FRAMES + 1];
        assert!(matches!(
            path.validate(),
            Err(Error::TooLarge {
                max: MAX_FRAMES,
                ..
            })
        ));
        path.energies_hartree.pop();
        path.numbers = vec![2; MAX_TRAJECTORY_ATOMS / MAX_FRAMES + 1];
        path.isotopes.clear();
        path.masses_amu.clear();
        assert!(matches!(
            path.validate(),
            Err(Error::TooLarge {
                max: MAX_TRAJECTORY_ATOMS,
                ..
            })
        ));
    }
}
