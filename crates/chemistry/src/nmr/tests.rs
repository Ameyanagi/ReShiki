use super::*;
fn environment(smiles: &str) -> Environment {
    let imported = crate::smiles::read(smiles).unwrap();
    Environment::new(&imported.prepared.state.graph).unwrap()
}
#[test]
fn canonical_spheres_preserve_rings_and_ignore_atom_order() {
    for (a, b) in [
        ("CCO", "OCC"),
        ("CC(=O)O", "OC(C)=O"),
        ("Cc1ccccc1", "c1ccc(C)cc1"),
    ] {
        let ea = environment(a);
        let eb = environment(b);
        for nucleus in [Nucleus::H1, Nucleus::C13] {
            for radius in MIN_RADIUS..=MAX_RADIUS {
                let mut ca: Vec<_> = ea
                    .sites(nucleus)
                    .unwrap()
                    .iter()
                    .filter(|s| !s.exchangeable)
                    .map(|s| {
                        ea.code(s, nucleus, radius, Environment::deadline())
                            .unwrap()
                    })
                    .collect();
                let mut cb: Vec<_> = eb
                    .sites(nucleus)
                    .unwrap()
                    .iter()
                    .filter(|s| !s.exchangeable)
                    .map(|s| {
                        eb.code(s, nucleus, radius, Environment::deadline())
                            .unwrap()
                    })
                    .collect();
                ca.sort();
                cb.sort();
                assert_eq!(ca, cb, "{a} {b} {nucleus:?} {radius}");
            }
        }
    }
    let chain = environment("CCCCCC");
    let ring = environment("C1CCCCC1");
    assert_ne!(
        chain
            .code(
                &chain.sites(Nucleus::C13).unwrap()[2],
                Nucleus::C13,
                4,
                Environment::deadline()
            )
            .unwrap(),
        ring.code(
            &ring.sites(Nucleus::C13).unwrap()[0],
            Nucleus::C13,
            4,
            Environment::deadline()
        )
        .unwrap()
    );
}
#[test]
fn implicit_and_explicit_h_share_sites() {
    let a = environment("CO");
    let b = environment("[H]C([H])([H])O[H]");
    let sa = a.sites(Nucleus::H1).unwrap();
    let sb = b.sites(Nucleus::H1).unwrap();
    assert_eq!(sa.len(), 2);
    assert_eq!(sa[0].count, 3);
    assert!(sa[1].exchangeable);
    assert_eq!(sb[0].count, 3);
    for radius in MIN_RADIUS..=MAX_RADIUS {
        assert_eq!(
            a.code(&sa[0], Nucleus::H1, radius, Environment::deadline())
                .unwrap(),
            b.code(&sb[0], Nucleus::H1, radius, Environment::deadline())
                .unwrap()
        );
    }
}
#[test]
fn longest_supported_sphere_and_unsupported_are_explicit() {
    let env = environment("CCO");
    let site = env.sites(Nucleus::H1).unwrap().remove(0);
    let mut index = Index::default();
    for (radius, values) in [(2, vec![1., 3., 9.]), (3, vec![2., 4.]), (4, vec![17.])] {
        index.entries.insert(
            (
                Nucleus::H1,
                radius,
                env.code(&site, Nucleus::H1, radius, Environment::deadline())
                    .unwrap(),
            ),
            Statistics::from_values(values).unwrap(),
        );
    }
    let rows = index
        .predict(&env, Nucleus::H1, Environment::deadline())
        .unwrap();
    assert_eq!(rows[0].radius, Some(3));
    assert_eq!(rows[0].statistics.as_ref().unwrap().median, 3.);
    assert!(rows[0].limitation.is_none());
    assert!(rows[1].statistics.is_none());
    assert!(
        rows[2]
            .limitation
            .as_ref()
            .unwrap()
            .contains("Exchangeable")
    );
    assert!(
        env.code(
            &site,
            Nucleus::H1,
            4,
            Instant::now() - Duration::from_secs(1)
        )
        .is_err()
    );
}
#[test]
fn broad_match_notes_use_nucleus_spread_cutoffs_only_at_the_shortest_sphere() {
    let env = environment("CC");
    for (nucleus, sd, broad) in [
        (Nucleus::H1, 0.499, false),
        (Nucleus::H1, 0.5, true),
        (Nucleus::H1, 0.501, true),
        (Nucleus::C13, 0.5, false),
        (Nucleus::C13, 4.999, false),
        (Nucleus::C13, 5., true),
        (Nucleus::C13, 5.001, true),
    ] {
        let site = env.sites(nucleus).unwrap().remove(0);
        for radius in MIN_RADIUS..=MAX_RADIUS {
            let statistics = Statistics {
                support: MIN_SUPPORT,
                median: 1.5,
                standard_deviation: sd,
                minimum: 0.,
                maximum: 10.,
            };
            let mut index = Index::default();
            index.entries.insert(
                (
                    nucleus,
                    radius,
                    env.code(&site, nucleus, radius, Environment::deadline())
                        .unwrap(),
                ),
                statistics.clone(),
            );
            let rows = index
                .predict(&env, nucleus, Environment::deadline())
                .unwrap();
            let row = &rows[0];
            assert_eq!(row.radius, Some(radius));
            assert_eq!(
                serde_json::to_value(&row.statistics).unwrap(),
                serde_json::to_value(&statistics).unwrap()
            );
            let expected = (broad && radius == MIN_RADIUS)
                .then(|| format!("Broad match · Shell {radius} · SD {sd:.3} ppm"));
            assert_eq!(row.limitation, expected, "{nucleus:?} {radius} {sd}");
        }
    }
}
#[test]
fn unsupported_chemistry_never_receives_guessed_numbers() {
    for input in ["[Na+].[Cl-]", "[CH2]", "[2H]C"] {
        let parsed = crate::smiles::read(input).unwrap();
        assert!(
            Environment::new(&parsed.prepared.state.graph).is_err(),
            "{input}"
        );
    }
}
#[test]
fn graph_hydrogen_atoms_are_retained_as_linked_proton_sites() {
    let graph = Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                no_implicit: true,
                ..Default::default()
            },
            Atom {
                atomic_number: 8,
                no_implicit: true,
                ..Default::default()
            },
            Atom {
                atomic_number: 1,
                no_implicit: true,
                ..Default::default()
            },
            Atom {
                atomic_number: 1,
                no_implicit: true,
                ..Default::default()
            },
            Atom {
                atomic_number: 1,
                no_implicit: true,
                ..Default::default()
            },
            Atom {
                atomic_number: 1,
                no_implicit: true,
                ..Default::default()
            },
        ],
        bonds: vec![
            Bond {
                a: 0,
                b: 1,
                order: 1,
                aromatic: false,
            },
            Bond {
                a: 0,
                b: 2,
                order: 1,
                aromatic: false,
            },
            Bond {
                a: 0,
                b: 3,
                order: 1,
                aromatic: false,
            },
            Bond {
                a: 0,
                b: 4,
                order: 1,
                aromatic: false,
            },
            Bond {
                a: 1,
                b: 5,
                order: 1,
                aromatic: false,
            },
        ],
    };
    let explicit = Environment::new(&graph).unwrap();
    let implicit = environment("CO");
    let sites = explicit.sites(Nucleus::H1).unwrap();
    assert_eq!(sites[0].explicit_hydrogens, vec![2, 3, 4]);
    assert_eq!(sites[0].count, 3);
    assert_eq!(sites[1].explicit_hydrogens, vec![5]);
    for radius in MIN_RADIUS..=MAX_RADIUS {
        assert_eq!(
            explicit
                .code(&sites[0], Nucleus::H1, radius, Environment::deadline())
                .unwrap(),
            implicit
                .code(
                    &implicit.sites(Nucleus::H1).unwrap()[0],
                    Nucleus::H1,
                    radius,
                    Environment::deadline()
                )
                .unwrap()
        );
    }
}
#[test]
fn symmetric_substituted_ring_codes_survive_atom_and_bond_permutations() {
    // Canonical ranking tie-breaking is tested against independently permuted
    // input indices, edge order and edge endpoint direction, not only SMILES.
    for smiles in [
        "c1ccccc1",
        "Cc1ccc(C)cc1",
        "Cc1cc(C)cc(C)c1",
        "Clc1cc(Cl)ccc1Br",
        "CC(C)c1ccc(C(C)C)cc1",
        "CC1CCC(C)CC1",
        "C1CC2CCC1C2",
        "C1CC1",
    ] {
        let original = crate::smiles::read(smiles).unwrap().prepared.state.graph;
        let signature = |graph: &Graph| {
            let env = Environment::new(graph).unwrap();
            let mut codes = Vec::new();
            for nucleus in [Nucleus::H1, Nucleus::C13] {
                for radius in MIN_RADIUS..=MAX_RADIUS {
                    for site in env
                        .sites(nucleus)
                        .unwrap()
                        .iter()
                        .filter(|s| !s.exchangeable)
                    {
                        codes.push((
                            nucleus,
                            radius,
                            env.code(site, nucleus, radius, Environment::deadline())
                                .unwrap(),
                        ));
                    }
                }
            }
            codes.sort();
            codes
        };
        let expected = signature(&original);
        let n = original.atoms.len();
        let mut state = 0x486f7365u64;
        for permutation in 0..32 {
            let mut old: Vec<_> = (0..n).collect();
            for i in (1..n).rev() {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                old.swap(i, (state as usize) % (i + 1));
            }
            let mut mapping = vec![0; n];
            for (new, &previous) in old.iter().enumerate() {
                mapping[previous] = new;
            }
            let mut bonds: Vec<_> = original
                .bonds
                .iter()
                .map(|b| Bond {
                    a: mapping[b.a],
                    b: mapping[b.b],
                    ..b.clone()
                })
                .collect();
            bonds.reverse();
            for (i, b) in bonds.iter_mut().enumerate() {
                if (i + permutation) % 2 == 0 {
                    std::mem::swap(&mut b.a, &mut b.b);
                }
            }
            let permuted = Graph {
                atoms: old.iter().map(|&i| original.atoms[i].clone()).collect(),
                bonds,
            };
            assert_eq!(
                expected,
                signature(&permuted),
                "{smiles}, permutation {permutation}"
            );
        }
    }
}

#[test]
fn reshiki_hose_v1_digests_match_fixed_goldens() {
    // Captured once from the current encoder. These keys address the bundled
    // NMR index; changing them requires an encoder version and index review.
    // The site index selects a proton group or carbon in SMILES input order;
    // the three digests are for radii 2, 3 and 4.
    const DIGESTS: [(&str, Nucleus, usize, [&str; 3]); 10] = [
        (
            "[H]C([H])([H])O[H]",
            Nucleus::H1,
            0,
            [
                "41bff7ef829030894ce181ddc745842af7f4c09c23d5cbfc1554dd35d17e1b42",
                "a5bc8a896374303a086534db88955fe11cadf90324e2979db6cb3caaa18c4686",
                "ea9a631498034028e060fdc855ba183ea1f352c05eaae5e9ce112f494750a637",
            ],
        ),
        (
            "[H]C([H])([H])O[H]",
            Nucleus::C13,
            0,
            [
                "1f334e1e79244bfd6ca4004c39ffa237749bd97dce2157ba6c63f6558a8a8ddc",
                "088fa6485bd093ad1d9660f7c3fac72e8dfc656fd0f83f4d36cfada2da9a4fb5",
                "213ff2cc7f97e00a1c85e82ee0238305aba29b11839eac852db8cb4472bfa54e",
            ],
        ),
        (
            "CCO",
            Nucleus::H1,
            1,
            [
                "732e5f39ed2b1b819c5c504dfdd9c9b488bfae0831b7e53f2e3f1104e082a629",
                "fb2bf7dd16aeded7f693101715b96ba3c64b84220382ecee68fb5a1db9f6dd5d",
                "46163e0107ba3d844f6231ac8979fb50ce37041badd4e498d92a313c16209493",
            ],
        ),
        (
            "CCO",
            Nucleus::C13,
            1,
            [
                "31dc682bd6d9270e0160fd4736638eddc037e4734c5181268ebee00a5c5143e1",
                "f21b5488e7bb8cf58bd539845c7a2b735f51feb3410e6d59cb41d975f5303e05",
                "82ed02241868676fc9c0c2a17ad842b8f718cb09db1a227ef08ec78c3f2b9a19",
            ],
        ),
        (
            "Cc1ccccc1",
            Nucleus::H1,
            1,
            [
                "3927b6fd94c007cf7ec9f0f91850881cbf312d0132b3721b6903fddaa0898e57",
                "3b891d35460903c3751b6f6fbe1819ae8465601c3d76de64c6afdf565277ad14",
                "3b2fab22f6e117abf5375c28fbe4820b4d560fa94a5fcb6ee7fa797eba13d113",
            ],
        ),
        (
            "Cc1ccccc1",
            Nucleus::C13,
            1,
            [
                "153c747af1cbd12d04b473a123c4526bc07ead2216cd91fd566369a2bcd06804",
                "7ddaf3449cb81a7bc2a992b24ba56f0c1d35a3bd7bc35d8b6f3120321e6931a4",
                "684e29f5f084384aa679fcf65424d15d30b68cad467010d5f68488a6453eeea3",
            ],
        ),
        (
            "C1CCCCC1",
            Nucleus::H1,
            0,
            [
                "b67fcd39edd4b74fd6a5e030439be6e8f6554238e2da712b8d8f8d1c195fa311",
                "4281a07028245db85eb0e7bbb49910a4bd2ad4ed3f53b7f324cd6ba22e9fec36",
                "a20094b0c9f1e9e03191d3959c1ceeacc072e789d922c273ac5acc84136ce201",
            ],
        ),
        (
            "C1CCCCC1",
            Nucleus::C13,
            0,
            [
                "9d61da9d8fcbb13fc66765fe016a3716fb039f4c43f4dcf3d02d5917843276c8",
                "fb7f8d9a9d3a88bae61fe6f13452bebec8da76fe8cbc9abead452e4f733987f2",
                "7e8b0f1faf0b124ec938bf6495868211f9c896f9230c3b1b5596ddb08aedd7a2",
            ],
        ),
        (
            "CCOC(C)=O",
            Nucleus::H1,
            0,
            [
                "868cfd6592e0d9c3c1e0f79de5e47d2de6e3324a9a3aaabc319091ad2dc37f9e",
                "44f2e78309f2147181cc22cd6c61e93af2acb1cf84e6c3346a65e6484673435a",
                "117db168fc45f41196686e4a0b0dfed5c1534c2f17d64901262596c04886d612",
            ],
        ),
        (
            "CCOC(C)=O",
            Nucleus::C13,
            2,
            [
                "a0d4358346d97c7482f830507ed724bd313c333bd6b3a1d2423247d97c030f20",
                "0ea90cfd39d9b60fa6d6d45c2ac4a9e7ba344ec5f37a0927365dd4f08be03d7b",
                "d6477c6bda067980755d7ff3f8bb54ebfc3a4f4cc4de55f99c59bae0e51e4a86",
            ],
        ),
    ];
    assert_eq!(ENCODER, "reshiki-hose-v1");
    assert_eq!((MIN_RADIUS, MAX_RADIUS), (2, 4));
    for (smiles, nucleus, site_index, expected) in DIGESTS {
        let env = environment(smiles);
        let sites = env.sites(nucleus).unwrap();
        let site = sites.get(site_index).unwrap();
        assert!(!site.exchangeable, "{smiles} {nucleus:?} {site_index}");
        for (radius, expected) in (MIN_RADIUS..=MAX_RADIUS).zip(expected) {
            let digest = env
                .code(site, nucleus, radius, Environment::deadline())
                .unwrap();
            assert_eq!(digest.len(), 64, "{smiles} {nucleus:?} {radius}");
            assert!(
                digest
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')),
                "Digest must be lowercase hexadecimal: {smiles} {nucleus:?} {radius}"
            );
            assert_eq!(
                digest, expected,
                "{smiles} {nucleus:?} {site_index} {radius}"
            );
        }
    }
}

#[test]
fn reshiki_hose_v1_ranking_options_pin_inherited_defaults() {
    // These options mirror Environment::code and define reshiki-hose-v1 keys.
    // Any option change must be treated as invalidating the bundled NMR index.
    // The golden digests above also catch ranking drift through encoder behavior.
    // Exhaustive destructuring also makes newly added ranking fields explicit.
    let ranking::Options {
        break_ties,
        include_chirality,
        include_isotopes,
        include_maps,
        include_chiral_presence,
        include_stereo_groups,
        use_non_stereo_ranks,
        include_ring_stereo,
        fragment,
    } = ranking::Options {
        include_chirality: false,
        ..Default::default()
    };
    assert!(break_ties, "break_ties");
    assert!(!include_chirality, "include_chirality");
    assert!(include_isotopes, "include_isotopes");
    assert!(include_maps, "include_maps");
    assert!(!include_chiral_presence, "include_chiral_presence");
    assert!(include_stereo_groups, "include_stereo_groups");
    assert!(!use_non_stereo_ranks, "use_non_stereo_ranks");
    assert!(include_ring_stereo, "include_ring_stereo");
    assert!(!fragment, "fragment");
}
