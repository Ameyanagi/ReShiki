//! Cases copied from pinned OPSIN CASToolsTest (MIT Daniel Lowe).
use opsin::Parser;

#[test]
fn all_upstream_cas_uninversion_examples() {
    let parser = Parser::new().unwrap();
    let cases = [
        ("Silane, chloromethyl-", "chloromethyl-Silane"),
        (
            "Acetic acid, 2-ethoxy-2-thioxo-",
            "2-ethoxy-2-thioxo-Acetic acid",
        ),
        ("Silanol, 1,1'-methylenebis-", "1,1'-methylenebis-Silanol"),
        (
            "Phosphonic acid, P,P'-(8-methylene-3,7,10,14-tetraoxo-4,6,11,13-tetraazahexadecane-1,16-diyl)-bis-, P,P,P',P'-tetramethyl ester",
            "P,P,P',P'-tetramethyl P,P'-(8-methylene-3,7,10,14-tetraoxo-4,6,11,13-tetraazahexadecane-1,16-diyl)-bis-Phosphonate",
        ),
        (
            "Benzenamine, 3,3',3''-(1-ethenyl-2-ylidene)tris[6-methyl-",
            "3,3',3''-(1-ethenyl-2-ylidene)tris[6-methyl-Benzenamine]",
        ),
        (
            "Pyridine, 3,3'-thiobis[6-chloro-",
            "3,3'-thiobis[6-chloro-Pyridine]",
        ),
        (
            "1-Butanesulfonic acid, 2,4-diamino-3-chloro- 1-ethyl ester",
            "1-ethyl 2,4-diamino-3-chloro-1-Butanesulfonate",
        ),
        (
            "Benzenecarboximidamide, N'-(1E)-1-propen-1-yl-N-(1Z)-1-propen-1-yl-",
            "N'-(1E)-1-propen-1-yl-N-(1Z)-1-propen-1-yl-Benzenecarboximidamide",
        ),
        (
            "Phosphoric acid, ethyl dimethyl ester",
            "ethyl dimethyl Phosphorate",
        ),
        ("2-Propanone, oxime", "2-Propanone oxime"),
        (
            "Disulfide, bis(2-chloroethyl)",
            "bis(2-chloroethyl) Disulfide",
        ),
        (
            "Ethanimidic acid, N-nitro-, (1Z)-",
            "(1Z)-N-nitro-Ethanimidic acid",
        ),
        (
            "2(1H)-Pyridinone, hydrazone, (2E)-",
            "(2E)-2(1H)-Pyridinone hydrazone",
        ),
        (
            "benzoic acid, 4,4'-methylenebis[2-chloro-",
            "4,4'-methylenebis[2-chloro-benzoic acid]",
        ),
        ("peroxide, ethyl methyl", "ethyl methyl peroxide"),
        (
            "Phosphonic diamide, P-phenyl- (8CI9CI)",
            "P-phenyl-Phosphonic diamide",
        ),
        (
            "piperazinium, 1,1-dimethyl-, 2,2,2-trifluoroacetate hydrochloride",
            "1,1-dimethyl-piperazinium 2,2,2-trifluoroacetate hydrochloride",
        ),
        (
            "Acetamide, ethylenebis(((ethyl)amino)-",
            "ethylenebis(((ethyl)amino)-Acetamide)",
        ),
        (
            "Benzenesulfonic acid, 4-amino-, 1-methylhydrazide",
            "4-amino-Benzenesulfonic acid 1-methylhydrazide",
        ),
        ("Acetaldehyde, O-methyloxime", "Acetaldehyde O-methyloxime"),
        (
            "Acetic acid, 2-amino-2-oxo-, 2-(phenylmethylene)hydrazide",
            "2-amino-2-oxo-Acetic acid 2-(phenylmethylene)hydrazide",
        ),
        (
            "L-Alanine, N-carboxy-, 1-ethyl ester",
            "1-ethyl N-carboxy-L-Alaninate",
        ),
        (
            "Pyridine, 3-(tetrahydro-2H-pyran-2-yl)-, (S)-",
            "(S)-3-(tetrahydro-2H-pyran-2-yl)-Pyridine",
        ),
        (
            "Pyrrolo[1,2-a]pyrimidinium, 1-[4-[(aminoiminomethyl)amino]butyl]-7-[[2-[(aminoiminomethyl)-amino]ethyl]thio]-6-(11-dodecenyl)-2,3,4,6,7,8-hexahydro-6-hydroxy-, chloride, dihydrochloride",
            "1-[4-[(aminoiminomethyl)amino]butyl]-7-[[2-[(aminoiminomethyl)-amino]ethyl]thio]-6-(11-dodecenyl)-2,3,4,6,7,8-hexahydro-6-hydroxy-Pyrrolo[1,2-a]pyrimidinium chloride dihydrochloride",
        ),
        ("acetic acid, sodium salt", "acetic acid sodium salt"),
        (
            "benzamide, trifluoroacetic acid salt",
            "benzamide trifluoroacetic acid salt",
        ),
    ];
    assert_eq!(cases.len(), 26);
    for (input, expected) in cases {
        assert_eq!(
            parser.uninvert_cas_name(input).unwrap(),
            expected,
            "{input}"
        );
    }
}

#[test]
fn upstream_non_cas_names_are_rejected() {
    let parser = Parser::new().unwrap();
    for input in [
        "hexanamine, hexylamine",
        "cyclopropane-1,2-diyldicarbonyl diisocyanate, cyclopropane-1,2-diylbis(carbonyl)bisisocyanate",
        "benzoic acid, ester",
    ] {
        assert!(parser.uninvert_cas_name(input).is_err(), "{input}");
    }
}

#[test]
fn pinned_split_and_compound_with_behavior_is_preserved() {
    let parser = Parser::new().unwrap();
    assert_eq!(
        parser
            .uninvert_cas_name("Acetic acid, sodium salt ")
            .unwrap(),
        "Acetic acid sodium salt"
    );
    assert_eq!(
        parser
            .uninvert_cas_name("Acetic acid, compound with sodium")
            .unwrap(),
        "Acetic acid compound with sodium"
    );
    assert_eq!(
        parser.uninvert_cas_name("Acetic acid (8CI)").unwrap(),
        "Aceticacid"
    );
}
