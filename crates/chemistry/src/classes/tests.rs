use super::*;

fn terms(smiles: &str) -> Result<Vec<&'static str>, String> {
    let prepared = crate::smiles::prepare(smiles).map_err(|error| error.to_string())?;
    Ok(classify(&prepared.state.graph)?
        .into_iter()
        .map(|class| class.term)
        .collect())
}

#[test]
fn canonical_class_examples() -> Result<(), String> {
    for (smiles, term) in [
        ("CN", "primary amine"),
        ("CNC", "secondary amine"),
        ("CN(C)C", "tertiary amine"),
        ("C[N+](C)(C)C", "quaternary ammonium"),
        ("CC(=O)N", "amide"),
        ("O=C1CCCN1", "lactam"),
        ("O=C1CCCO1", "lactone"),
        ("CC(=O)OC", "ester"),
        ("CC(=O)SC", "thioester"),
        ("COC(=O)N", "carbamate"),
        ("NC(=O)N", "urea"),
        ("CC(=O)Cl", "acyl halide"),
        ("CC(=O)OC(C)=O", "carboxylic acid anhydride"),
        ("CC(=O)O", "carboxylic acid"),
        ("CC=O", "aldehyde"),
        ("CC(=O)C", "ketone"),
        ("CCO", "alcohol"),
        ("Oc1ccccc1", "phenol"),
        ("COC", "ether"),
        ("CS", "thiol"),
        ("CC#N", "nitrile"),
        ("C[N+]#[C-]", "isocyanide"),
        ("CC=NC", "imine"),
        ("CC=NO", "oxime"),
        ("C[N+]([O-])=CC", "nitrone"),
        ("C[N+](C)(C)[O-]", "amine oxide"),
        ("CC(O)N", "hemiaminal"),
        ("COCOC", "acetal"),
        ("COCO", "hemiacetal"),
        ("C=CN(C)C", "enamine"),
        ("C=CO", "enol"),
        ("C[N+](=O)[O-]", "nitro compound"),
        ("CN=[N+]=[N-]", "azide"),
        ("CCCl", "alkyl halide"),
        ("Clc1ccccc1", "aryl halide"),
        ("CS(=O)(=O)N", "sulfonamide"),
        ("CS(=O)(=O)C", "sulfone"),
        ("CS(=O)C", "sulfoxide"),
        ("C1CO1", "epoxide"),
    ] {
        assert!(terms(smiles)?.contains(&term), "{smiles}: missing {term}");
    }
    Ok(())
}

#[test]
fn methyl_pyroglutamate_has_only_the_three_expected_classes() -> Result<(), String> {
    assert_eq!(
        terms("COC(=O)[C@@H]1CCC(=O)N1")?,
        ["lactam", "amide", "ester"]
    );
    Ok(())
}

#[test]
fn amines_require_an_amine_nitrogen() -> Result<(), String> {
    for smiles in [
        "CC(=O)N",
        "O=C1CCCN1",
        "O=c1cccc[nH]1",
        "COC(=O)N",
        "NC(=O)N",
        "CS(=O)(=O)N",
        "CC(=S)N",
        "CC(=N)N",
        "NC(=N)N",
        "n1ccccc1",
        "c1cc[nH]c1",
        "C[N+](C)(C)[O-]",
        "C[N+]([O-])=CC",
        "C[NH3+]",
        "CN=O",
        "CNO",
        "C[NH]",
    ] {
        let found = terms(smiles)?;
        assert!(
            found.iter().all(|term| !term.ends_with(" amine")),
            "{smiles}: {found:?}"
        );
    }
    assert_eq!(terms("Nc1ccccc1")?, ["primary amine"]);
    assert_eq!(terms("CN(C)C")?, ["tertiary amine"]);
    assert!(!terms("C[NH2+]C")?.contains(&"quaternary ammonium"));
    assert!(!terms("C=[N+](C)C")?.contains(&"quaternary ammonium"));
    Ok(())
}

#[test]
fn cyclic_subclasses_require_the_functional_bond_in_a_ring() -> Result<(), String> {
    assert_eq!(terms("O=c1cccc[nH]1")?, ["lactam", "amide"]);
    assert_eq!(terms("O=C1CCCN1")?, ["lactam", "amide"]);
    assert_eq!(terms("O=C1CCCO1")?, ["lactone", "ester"]);
    assert_eq!(terms("O=c1cccco1")?, ["lactone", "ester"]);
    assert_eq!(terms("O=c1ccc2ccccc2o1")?, ["lactone", "ester"]);
    assert_eq!(terms("CC(=O)NC1CCCC1")?, ["amide"]);
    assert_eq!(terms("CC(=O)OC1CCCC1")?, ["ester"]);
    assert!(!terms("C1COC1")?.contains(&"epoxide"));
    assert_eq!(terms("C1CO1")?, ["ether", "epoxide"]);
    Ok(())
}

#[test]
fn carbonyl_classes_overlap_only_when_the_definitions_do() -> Result<(), String> {
    assert_eq!(terms("COC(=O)N")?, ["amide", "ester", "carbamate"]);
    assert_eq!(terms("NC(=O)N")?, ["amide", "urea"]);
    // The glossary carbamate covers salts as well as esters.
    assert!(terms("NC(=O)[O-].[Na+]")?.contains(&"carbamate"));
    assert_eq!(terms("CC(=O)SC")?, ["thioester"]);
    assert_eq!(terms("CC(=O)OC(C)=O")?, ["carboxylic acid anhydride"]);
    assert_eq!(terms("CC(=O)O")?, ["carboxylic acid"]);
    assert_eq!(terms("CC(=O)Cl")?, ["acyl halide"]);
    assert_eq!(terms("CC=O")?, ["aldehyde"]);
    assert_eq!(terms("C=O")?, ["aldehyde"]);
    assert_eq!(terms("CC(=O)C")?, ["ketone"]);
    assert!(terms("CC(=O)[O-]")?.is_empty());
    assert!(!terms("CS(=O)(=O)Cl")?.contains(&"acyl halide"));
    Ok(())
}

#[test]
fn acetals_are_diethers_but_ordinary_ethers_are_not_acetals() -> Result<(), String> {
    assert_eq!(terms("COC")?, ["ether"]);
    assert_eq!(terms("COCOC")?, ["ether", "acetal"]);
    assert_eq!(terms("CC(OC)(OC)C")?, ["ether", "acetal"]);
    assert_eq!(terms("C1COCO1")?, ["ether", "acetal"]);
    assert_eq!(terms("COCO")?, ["alcohol", "ether", "hemiacetal"]);
    assert_eq!(terms("CC(O)N")?, ["primary amine", "alcohol", "hemiaminal"]);
    assert_eq!(terms("CC(=O)NCO")?, ["amide", "alcohol", "hemiaminal"]);
    for smiles in ["COC", "CC(=O)OC", "COC(OC)OC", "CC(=O)OC(OC(C)=O)C"] {
        assert!(!terms(smiles)?.contains(&"acetal"), "{smiles}");
    }
    Ok(())
}

#[test]
fn hydroxy_and_unsaturated_nitrogen_classes_are_distinct() -> Result<(), String> {
    assert_eq!(terms("CCO")?, ["alcohol"]);
    assert_eq!(terms("Oc1ccccc1")?, ["phenol"]);
    assert_eq!(terms("C=CO")?, ["enol"]);
    assert_eq!(terms("CS")?, ["thiol"]);
    assert_eq!(terms("CC#N")?, ["nitrile"]);
    assert_eq!(terms("C[N+]#[C-]")?, ["isocyanide"]);
    assert_eq!(terms("CC=NC")?, ["imine"]);
    assert_eq!(terms("CC=NO")?, ["oxime"]);
    assert_eq!(terms("C[N+]([O-])=CC")?, ["nitrone"]);
    assert_eq!(terms("C[N+](C)(C)[O-]")?, ["amine oxide"]);
    assert_eq!(terms("C=CN(C)C")?, ["tertiary amine", "enamine"]);
    assert!(!terms("CC#N")?.contains(&"imine"));
    assert!(!terms("C[N+]#[C-]")?.contains(&"nitrile"));
    assert!(!terms("CO[N+](=O)[O-]")?.contains(&"nitro compound"));
    assert!(!terms("C[N+](=O)[O-]")?.contains(&"amine oxide"));
    assert!(!terms("CC=NOC")?.contains(&"oxime"));
    Ok(())
}

#[test]
fn halide_and_sulfur_predicates_respect_their_neighbors() -> Result<(), String> {
    for smiles in ["CCF", "CCCl", "CCBr", "CCI"] {
        assert_eq!(terms(smiles)?, ["alkyl halide"]);
    }
    assert_eq!(terms("Clc1ccccc1")?, ["aryl halide"]);
    assert!(terms("C=CCl")?.is_empty());
    assert!(terms("[Cl-]")?.is_empty());
    assert_eq!(terms("CS(=O)(=O)N")?, ["sulfonamide"]);
    assert_eq!(terms("CS(=O)(=O)C")?, ["sulfone"]);
    assert_eq!(terms("CS(=O)C")?, ["sulfoxide"]);
    assert!(!terms("CS(=O)(=O)O")?.contains(&"sulfone"));
    assert!(!terms("CS(=O)O")?.contains(&"sulfoxide"));
    assert!(terms("CN=NN")?.iter().all(|term| *term != "azide"));
    Ok(())
}

#[test]
fn explicit_hydrogens_and_fragment_order_do_not_change_class_membership() -> Result<(), String> {
    assert_eq!(terms("[H]N([H])C")?, terms("CN")?);
    assert_eq!(terms("[H]OC")?, terms("CO")?);
    assert_eq!(terms("[2H]N([2H])C")?, terms("CN")?);
    assert_eq!(terms("[2H]OC")?, terms("CO")?);
    assert_eq!(terms("CCO.CN")?, terms("CN.CCO")?);
    assert!(terms("")?.is_empty());
    Ok(())
}

#[test]
fn prepared_resonance_forms_and_extended_oxide_classes() -> Result<(), String> {
    assert_eq!(terms("CN(=O)=O")?, ["nitro compound"]);
    assert_eq!(terms("CN=[N+]=[N-]")?, ["azide"]);
    assert_eq!(terms("C[N-][N+]#N")?, ["azide"]);
    assert_eq!(terms("C[S+](C)[O-]")?, ["sulfoxide"]);
    assert_eq!(terms("C[S+](C)(=O)[O-]")?, ["sulfone"]);
    assert_eq!(terms("C[NH2+][O-]")?, ["amine oxide"]);
    assert_eq!(terms("C[NH+](C)[O-]")?, ["amine oxide"]);
    assert_eq!(terms("CC=[NH+][O-]")?, ["nitrone"]);
    assert!(!terms("[O-][n+]1ccccc1")?.contains(&"amine oxide"));
    Ok(())
}

#[test]
fn classification_is_read_only_and_rejects_invalid_graphs() -> Result<(), String> {
    let prepared = crate::smiles::prepare("CC(O)N").map_err(|error| error.to_string())?;
    let graph = prepared.state.graph;
    let before = graph.clone();
    assert!(!classify(&graph)?.is_empty());
    assert_eq!(graph, before);
    let mut invalid = graph;
    invalid.bonds[0].b = invalid.atoms.len();
    assert!(classify(&invalid).is_err());
    Ok(())
}
