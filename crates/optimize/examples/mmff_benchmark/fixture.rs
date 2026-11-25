//! Seeded, unminimized MMFF94 starting geometries shared with the TRIC phase profile.

use super::{ANGSTROM_PER_BOHR, Accuracy, EvalError, KCAL_PER_HARTREE, Potential};
use cosmolkit::{EmbedParams, MmffForceFieldParams, MolecularForceField, Molecule};
use std::error::Error;

pub(super) const MOLECULES: [(&str, &str); 20] = [
    ("aspirin", "CC(=O)Oc1ccccc1C(=O)O"),
    ("paracetamol", "CC(=O)Nc1ccc(O)cc1"),
    ("ibuprofen", "CC(C)Cc1ccc(C(C)C(=O)O)cc1"),
    ("naproxen", "COc1ccc2cc([C@@H](C)C(=O)O)ccc2c1"),
    ("caffeine", "Cn1c(=O)c2c(ncn2C)n(C)c1=O"),
    ("nicotine", "CN1CCC[C@H]1c1cccnc1"),
    ("lidocaine", "CCN(CC)CC(=O)Nc1c(C)cccc1C"),
    ("propranolol", "CC(C)NCC(O)COc1cccc2ccccc12"),
    ("diazepam", "CN1C(=O)CN=C(c2ccccc2)c2cc(Cl)ccc21"),
    ("fluoxetine", "CNCCC(Oc1ccc(C(F)(F)F)cc1)c1ccccc1"),
    ("sertraline", "CN[C@H]1CC[C@H](c2ccccc21)c2cc(Cl)c(Cl)cc2"),
    ("warfarin", "CC(=O)CC(c1ccccc1)c1c(O)c2ccccc2oc1=O"),
    ("omeprazole", "COc1ccc2[nH]c(S(=O)Cc3ncc(C)c(OC)c3C)nc2c1"),
    (
        "celecoxib",
        "Cc1ccc(-c2cc(C(F)(F)F)nn2-c2ccc(S(N)(=O)=O)cc2)cc1",
    ),
    (
        "sildenafil",
        "CCCc1nn(C)c2c(=O)[nH]c(-c3cc(S(=O)(=O)N4CCN(C)CC4)ccc3OCC)nc12",
    ),
    (
        "losartan",
        "CCCCc1nc(Cl)c(CO)n1Cc1ccc(-c2ccccc2-c2nn[nH]n2)cc1",
    ),
    (
        "ciprofloxacin",
        "O=C(O)c1cn(C2CC2)c2cc(N3CCNCC3)c(F)cc2c1=O",
    ),
    (
        "tamoxifen",
        r"CC/C(=C(\c1ccccc1)/c1ccc(OCCN(C)C)cc1)/c1ccccc1",
    ),
    (
        "testosterone",
        "C[C@]12CC[C@H]3[C@H]([C@@H]1CC[C@@H]2O)CCC4=CC(=O)CC[C@]34C",
    ),
    (
        "imatinib",
        "Cc1ccc(NC(=O)c2ccc(CN3CCN(C)CC3)cc2)cc1Nc1nccc(-c2cccnc2)n1",
    ),
];

pub(super) struct MmffPotential(pub(super) MolecularForceField);

impl Potential for MmffPotential {
    fn evaluate(
        &mut self,
        positions: &[f64],
        gradient: &mut [f64],
        _accuracy: Accuracy,
    ) -> Result<f64, EvalError> {
        let coordinates: Vec<_> = positions
            .as_chunks::<3>()
            .0
            .iter()
            .map(|row| row.map(|x| x * ANGSTROM_PER_BOHR))
            .collect();
        let failed = |error: cosmolkit::ForceFieldError| EvalError::Failed(error.to_string());
        self.0.set_positions_(&coordinates).map_err(failed)?;
        let energy = self.0.energy().map_err(failed)?;
        let mmff_gradient = self.0.gradient_unconstrained().map_err(failed)?;
        for (output, &value) in gradient.iter_mut().zip(mmff_gradient.as_flattened()) {
            *output = value * ANGSTROM_PER_BOHR / KCAL_PER_HARTREE;
        }
        Ok(energy / KCAL_PER_HARTREE)
    }
}

pub(super) struct Fixture {
    molecule: Molecule,
    pub(super) numbers: Vec<u8>,
    pub(super) positions: Vec<f64>,
    pub(super) heavy_atoms: usize,
}

impl Fixture {
    pub(super) fn new_field(&self) -> Result<MolecularForceField, cosmolkit::MmffForceFieldError> {
        let params = MmffForceFieldParams::new(None, "MMFF94".into(), f64::INFINITY, true);
        self.molecule.mmff_force_field_with_params(&params)
    }
}

pub(super) fn prepare(smiles: &str) -> Result<Fixture, Box<dyn Error>> {
    let molecule = Molecule::from_smiles(smiles)?
        .with_hydrogens()?
        .sanitize()?;
    let numbers: Vec<_> = molecule.atoms().iter().map(|a| a.atomic_number()).collect();
    let heavy_atoms = numbers.iter().filter(|&&z| z > 1).count();
    let charge: i32 = molecule
        .atoms()
        .iter()
        .map(|a| i32::from(a.formal_charge()))
        .sum();
    if !(10..=70).contains(&heavy_atoms) || charge != 0 {
        return Err(format!(
            "expected 10–70 heavy atoms and zero charge, got {heavy_atoms} and {charge}"
        )
        .into());
    }

    let mut embedding = EmbedParams::etkdg_v3();
    embedding.random_seed = 42;
    embedding.num_threads = 1;
    let embedded = molecule.with_3d_conformer_result_with_params(&embedding)?;
    let [conformer] = embedded.molecule.conformers_3d() else {
        return Err("ETKDG did not generate one conformer".into());
    };
    let positions: Vec<_> = conformer
        .coordinates()
        .as_flattened()
        .iter()
        .map(|x| x / ANGSTROM_PER_BOHR)
        .collect();
    Ok(Fixture {
        molecule: embedded.molecule,
        numbers,
        positions,
        heavy_atoms,
    })
}
