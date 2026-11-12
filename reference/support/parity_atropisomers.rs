//! #1117: RDKit-parity runs (#1037) lay RDKit's `AtropManyChirals.cxsmi` and
//! its expected outputs out again and wedge them by RDKit's plain rules, which
//! loses their atropisomer axis. RDKit 2026.09.1's own relayout is just as
//! layout-sensitive. `engine_migration` counts these cases;
//! `atropisomer_normal_mode` checks that the normal import keeps each axis.

/// The original worker's refusal of RDKit's atropisomer bond stereo.
pub const FAILURE: &str = "This bond stereochemistry is not supported yet.";

/// Each file under the RDKit source's `Code/GraphMol/SmilesParse/test_data`
/// with its first line, the input `engine_migration` reads from it.
pub const CASES: [(&str, &str); 4] = [
    (
        "AtropManyChirals.cxsmi",
        "O=C1C=CNC(=O)N1C(=C)[C@]([C@H](C)Br)([C@@H](F)C)[C@H](Cl)C |(1.6038,0.5917,;1.2842,-0.2899,;1.9435,-0.5613,;2.0181,-1.3829,;1.344,-1.8584,;0.5951,-1.5123,;0.0595,-2.2018,;0.5204,-0.6907,;-0.3117,-0.3431,;-0.861,-1.0085,;-0.5013,0.5405,;-0.4918,1.4387,;-0.0783,2.2018,;-1.0961,2.0398,;0.3347,0.736,;0.6313,-0.0436,;1.118,1.0786,;-1.3746,0.4021,;-2.0181,0.9732,;-1.9327,-0.2608,),wU:7.7,11.13,10.14,17.18wD:14.15|",
    ),
    (
        "AtropManyChirals.cxsmi.expected.cxsmi",
        "O=c1cc[nH]c(=O)n1C(=C)[C@]([C@H](C)Br)([C@@H](F)C)[C@H](Cl)C |(1.6038,0.5917,;1.2842,-0.2899,;1.9435,-0.5613,;2.0181,-1.3829,;1.344,-1.8584,;0.5951,-1.5123,;0.0595,-2.2018,;0.5204,-0.6907,;-0.3117,-0.3431,;-0.861,-1.0085,;-0.5013,0.5405,;-0.4918,1.4387,;-0.0783,2.2018,;-1.0961,2.0398,;0.3347,0.736,;0.6313,-0.0436,;1.118,1.0786,;-1.3746,0.4021,;-2.0181,0.9732,;-1.9327,-0.2608,),wD:14.15,wU:7.7,11.13,10.14,17.18|",
    ),
    (
        "AtropManyChirals.cxsmi.expected2.cxsmi",
        "O=c1cc[nH]c(=O)n1C(=C)[C@]([C@H](C)Br)([C@@H](F)C)[C@H](Cl)C |(1.6038,0.5917,;1.2842,-0.2899,;1.9435,-0.5613,;2.0181,-1.3829,;1.344,-1.8584,;0.5951,-1.5123,;0.0595,-2.2018,;0.5204,-0.6907,;-0.3117,-0.3431,;-0.861,-1.0085,;-0.5013,0.5405,;-0.4918,1.4387,;-0.0783,2.2018,;-1.0961,2.0398,;0.3347,0.736,;0.6313,-0.0436,;1.118,1.0786,;-1.3746,0.4021,;-2.0181,0.9732,;-1.9327,-0.2608,),wD:10.10,14.16,wU:7.7,11.12,17.19|",
    ),
    (
        "AtropManyChirals.cxsmi.expected3.cxsmi",
        "C=C(n1c(=O)cc[nH]c1=O)[C@@]([C@H](C)F)([C@H](C)Br)[C@@H](C)Cl |(-0.861,-1.0085,;-0.3117,-0.3431,;0.5204,-0.6907,;1.2842,-0.2899,;1.6038,0.5917,;1.9435,-0.5613,;2.0181,-1.3829,;1.344,-1.8584,;0.5951,-1.5123,;0.0595,-2.2018,;-0.5013,0.5405,;0.3347,0.736,;1.118,1.0786,;0.6313,-0.0436,;-0.4918,1.4387,;-0.0783,2.2018,;-1.0961,2.0398,;-1.3746,0.4021,;-1.9327,-0.2608,;-2.0181,0.9732,),wD:10.10,11.12,wU:2.2,14.15,17.18|",
    ),
];

/// Whether an `engine_migration` SMILES case is one of these inputs.
#[allow(dead_code)]
pub fn parity_loses_axis(name: &str, failure: Option<&str>) -> bool {
    failure == Some(FAILURE)
        && CASES
            .iter()
            .any(|(file, _)| name == format!("source-cx/{file}/0"))
}
