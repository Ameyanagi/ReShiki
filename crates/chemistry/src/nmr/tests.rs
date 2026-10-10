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
