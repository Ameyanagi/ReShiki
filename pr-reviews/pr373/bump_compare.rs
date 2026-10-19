//! COSMolKit bump comparison harness (untracked; never commit).
//! Copy unchanged to crates/io/examples/bump_compare.rs and run
//! `BUMP_OUT=out.json cargo run -p reshiki-io --example bump_compare`.
//! Uses only ReShiki APIs that are identical before and after the migration:
//! reshiki_chemistry SMILES/InChI kernel/key and reshiki_geometry::solve.
use reshiki_geometry::{AtomInput, BondInput, ForceField, Operation, Request, Response, solve};
use reshiki_io::chemistry::{
    inchi::{self, kernel, key, output},
    kekulize::Direction,
    smiles,
    stereo::{Point3, perception::State},
};
use serde_json::{Value, json};
use std::{panic, time::Instant};

type P = [f64; 3];

// ---------------------------------------------------------------- InChI ----

/// (name, SMILES with optional CX suffix, optional state mutation).
fn inchi_corpus() -> Vec<(String, String, Option<&'static str>)> {
    let mut cases: Vec<(String, String, Option<&'static str>)> = Vec::new();
    let mut add = |group: &str, smiles: &str, mutation: Option<&'static str>| {
        let label = mutation.map_or(String::new(), |m| format!("#{m}"));
        cases.push((format!("{group}/{smiles}{label}"), smiles.to_owned(), mutation));
    };
    // Basic, tautomer, isotope, charge and radical cases from
    // reference/inchi_generator_reference.py.
    for s in [
        "C",
        "CCO",
        "CC(=O)C",
        "CC(O)=C",
        "NC(=O)N",
        "N=C(O)N",
        "CC(=O)CC(C)=O",
        "CC(O)=CC(C)=O",
    ] {
        add("basic-tautomer", s, None);
    }
    for s in [
        "[2H]OC([2H])([2H])C",
        "[13CH3][C@@H]([18OH])C(=O)[O-]",
        "[13CH4]",
        "[2H][C@H](F)Cl",
    ] {
        add("isotope", s, None);
    }
    for s in [
        "[NH4+].[Cl-]",
        "[O-][N+](=O)c1ccccc1",
        "C[N+](C)(C)C",
        "CC(=O)[O-].[Na+]",
        "[CH2][CH2]",
    ] {
        add("charge-radical", s, None);
    }
    // Tetrahedral stereo from reference/inchi_input_reference.py.
    for s in [
        "F[C@](Cl)(Br)I",
        "F[C@@](Cl)(Br)I",
        "N[C@@H](C)C(=O)O",
        "C[S@](=O)CC",
        "C[P@](F)Cl",
        "C1[C@H](O)CCC1",
        "O[C@H]1CC[C@@H](O)CC1",
    ] {
        add("tetrahedral", s, None);
    }
    // Double-bond stereo: specified, unspecified, CX unknown (ctu) and
    // explicit either-double / unknown-direction bonds.
    for s in [
        "F/C=C/Cl",
        "F/C=C\\Cl",
        "F/C=C1/CCCCCCC1",
        "ClC(Br)=C(F)I",
        "FC=CC=CF",
        "C/C=C/C=C/C",
        "FC=CF |ctu:1|",
        "CC=CC |ctu:1|",
    ] {
        add("double-bond", s, None);
    }
    add("double-bond", "F/C=C/Cl", Some("either-double"));
    add("double-bond", "F/C=C/Cl", Some("stereo-any"));
    add("tetrahedral", "[C@](F)(Cl)(Br)I", Some("wavy-first-bond"));
    // Aromatic heterocycles and fused systems.
    for s in [
        "c1ccncc1",
        "c1cc[nH]c1",
        "c1ccoc1",
        "c1ccsc1",
        "c1ncncn1",
        "c1ccc2[nH]ccc2c1",
        "c1ccc2ncccc2c1",
        "Cn1cnc2c1c(=O)n(C)c(=O)n2C",
        "*1ccccc1",
    ] {
        add("aromatic", s, None);
    }
    // Perchlorate-like cleanup (reference/inchi_input_reference.py), plus
    // reordered spellings that change atom and bond insertion order.
    for s in [
        "[O-][Cl+3]([O-])([O-])O",
        "[O-][Cl+3]([O-])([O-])[O-]",
        "[O-][Cl+3]([O-])([O-])[O+]",
        "[O-][Cl+3]([O-])([O-])[O-].[O-][Cl+3]([O-])([O-])O",
        "[O-][Cl+3]([O-])([O-])(O)O",
        "[O-][Cl+3]([O-])([O-])([O-])O",
        "[O-][Cl+3]([O-])([O-])O[Cl+3]([O-])([O-])[O-]",
        "O[Cl+3]([O-])([O-])[O-]",
        "[O-][Cl+3](O)([O-])[O-]",
        "OCl(=O)(=O)=O",
        "[O-]Cl(=O)(=O)=O.[Na+]",
    ] {
        add("perchlorate", s, None);
    }
    // Heavy elements Tl..Ra (fork EL_DATA_SKIP_ADDING_H patch) with neighbours.
    for s in [
        "[Tl]", "[Pb]", "[Bi]", "[Po]", "[At]", "[Rn]", "[Fr]", "[Ra]", "C[Tl](C)C", "C[Pb](C)(C)C",
        "[PbH4]", "[BiH3]", "Cl[Ra]Cl", "C[Hg]C", "[Au]",
    ] {
        add("heavy-element", s, None);
    }
    for s in ["[Tl]", "[Pb]", "[Bi]", "[Po]", "[At]", "[Fr]", "[Ra]"] {
        add("heavy-element", s, Some("implicit-h"));
    }
    // Long and invalid inputs.
    add("long", &"C".repeat(1024), None);
    add("long", &vec!["[He]"; 1023].join("."), None);
    add("long", &vec!["[He]"; 1024].join("."), None);
    for s in ["", "C(", "C1CC", "c1cccc1", "Xx", "[C+16]"] {
        add("invalid", s, None);
    }
    cases
}

fn mutate(state: &mut State, mutation: &str) -> Result<(), String> {
    match mutation {
        "either-double" | "stereo-any" => {
            let bond = state
                .graph
                .bonds
                .iter()
                .position(|b| b.order == 2)
                .ok_or("no double bond")?;
            if mutation == "either-double" {
                state.directions[bond] = Direction::EitherDouble;
            } else {
                let meta = &mut state.metadata.bonds[bond];
                meta.stereo = 1;
                meta.stereo_atoms.clear();
                meta.unknown_stereo = true;
            }
            // Clear neighbouring up/down directions so only the edit remains.
            for (i, d) in state.directions.iter_mut().enumerate() {
                if i != bond && matches!(d, Direction::Up | Direction::Down) {
                    *d = Direction::None;
                }
            }
        }
        "wavy-first-bond" => state.directions[0] = Direction::Unknown,
        "implicit-h" => state.graph.atoms[0].no_implicit = false,
        _ => return Err(format!("unknown mutation {mutation}")),
    }
    Ok(())
}

fn generated(molecule: &kernel::Molecule) -> Value {
    let started = Instant::now();
    let result = guard(|| kernel::generate(molecule));
    let elapsed = started.elapsed().as_secs_f64() * 1e3;
    match result {
        Err(_) => json!({"ok": false, "error": "panic", "elapsed_ms": elapsed}),
        Ok(Err(error)) => json!({"ok": false, "error": error, "elapsed_ms": elapsed}),
        Ok(Ok(g)) => {
            let inchikey = if g.inchi.is_empty() {
                json!(null)
            } else {
                match key::from_inchi(&g.inchi) {
                    Ok(k) => json!(k),
                    Err(e) => json!(format!("error: {e}")),
                }
            };
            let roundtrip = (!g.inchi.is_empty()).then(|| read_back(&g.inchi));
            json!({
                "ok": true, "status": g.status, "inchi": g.inchi, "inchikey": inchikey,
                "message": g.message, "log": g.log, "auxinfo": g.auxiliary,
                "diagnostics": g.diagnostics, "roundtrip": roundtrip, "elapsed_ms": elapsed,
            })
        }
    }
}

/// InChI -> ReShiki state -> InChI through the same kernel.
fn read_back(text: &str) -> Value {
    let imported = match guard(|| kernel::read(text, output::Options::default())) {
        Err(_) => return json!({"ok": false, "error": "panic"}),
        Ok(Err(e)) => return json!({"ok": false, "error": e}),
        Ok(Ok(i)) => i,
    };
    let mut value = json!({
        "ok": true, "status": imported.status, "message": imported.message,
        "log": imported.log, "diagnostics": imported.diagnostics,
        "unspecified_bonds": imported.unspecified_bonds,
    });
    if let Some(state) = imported.state {
        value["atoms"] = json!(state.graph.atoms.len());
        value["bonds"] = json!(state.graph.bonds.len());
        value["regenerated"] = match kernel::Molecule::prepare(&state, None) {
            Ok(m) => match guard(|| kernel::generate(&m)) {
                Ok(Ok(g)) => json!({"inchi": g.inchi, "status": g.status, "same": g.inchi == text}),
                Ok(Err(e)) => json!({"error": e}),
                Err(_) => json!({"error": "panic"}),
            },
            Err(e) => json!({"error": e.to_string()}),
        };
    }
    value
}

fn inchi_case(name: &str, text: &str, mutation: Option<&str>) -> Value {
    let mut out = json!({"name": name, "input": if text.len() > 200 {
        format!("{}...({} bytes)", &text[..60], text.len())
    } else {
        text.to_owned()
    }});
    let imported = match guard(|| smiles::read(text)) {
        Err(_) => {
            out["parse_error"] = json!("panic");
            return out;
        }
        Ok(Err(e)) => {
            out["parse_error"] = json!(e.to_string());
            return out;
        }
        Ok(Ok(i)) => i,
    };
    let mut state = imported.prepared.state;
    if let Some(m) = mutation
        && let Err(e) = mutate(&mut state, m)
    {
        out["mutation_error"] = json!(e);
        return out;
    }
    let positions: Option<Vec<Point3>> = imported
        .conformers
        .first()
        .map(|c| c.positions.clone())
        .filter(|p| p.len() == state.graph.atoms.len());
    out["atoms"] = json!(state.graph.atoms.len());
    out["bonds"] = json!(state.graph.bonds.len());
    out["positions"] = json!(positions.is_some());
    match guard(|| kernel::Molecule::prepare(&state, positions.as_deref())) {
        Err(_) => out["prepare_error"] = json!("panic"),
        Ok(Err(e)) => out["prepare_error"] = json!(e.to_string()),
        Ok(Ok(m)) => out["generated"] = generated(&m),
    }
    out
}

/// Direct InChI strings: reader and key paths, including invalid input.
fn inchi_read_corpus() -> Vec<&'static str> {
    vec![
        "InChI=1S/CH4/h1H4",
        "InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3",
        "InChI=1S/C2H2Cl2/c3-1-2-4/h1-2H/b2-1+",
        "InChI=1S/C2H2Cl2/c3-1-2-4/h1-2H/b2-1-",
        "InChI=1S/C3H7NO2/c1-2(4)3(5)6/h2H,4H2,1H3,(H,5,6)/t2-/m0/s1",
        "InChI=1S/Pb",
        "InChI=1S/Tl",
        "InChI=1S/Bi",
        "InChI=1S/Ra",
        "InChI=1S/H4Pb/h1H4",
        "InChI=1S/ClHO4/c2-1(3,4)5/h(H,2,3,4,5)/p-1",
        "InChI=1S/C47H51NO14/c1-25-31(60-43(56)36(52)35(28-16-10-7-11-17-28)48-41(54)29-18-12-8-13-19-29)23-47(57)40(61-42(55)30-20-14-9-15-21-30)38-45(6,32(51)22-33-46(38,24-58-33)62-27(3)50)39(53)37(59-26(2)49)34(25)44(47,4)5/h7-21,31-33,35-38,40,51-52,57H,22-24H2,1-6H3,(H,48,54)/t31-,32-,33+,35-,36+,37+,38-,40-,45+,46-,47+/m0/s1",
        "InChI=1S/C36H24/c1-2-26-4-3-25(1)27-5-7-29(8-6-27)31-13-15-33(16-14-31)35-21-23-36(24-22-35)34-19-17-32(18-20-34)30-11-9-28(26)10-12-30/h1-24H",
        "InChI=1S/é",
        "InChI=1S/",
        "InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3/x",
        "NotAnInChI",
    ]
}

// ------------------------------------------------------------- Geometry ----

const SEED: i32 = 0x52534b;
const FIELDS: [ForceField; 3] = [ForceField::MMFF94, ForceField::MMFF94s, ForceField::UFF];

fn request_from_state(state: &State, field: ForceField) -> Request {
    let atoms = state
        .graph
        .atoms
        .iter()
        .zip(&state.metadata.atoms)
        .map(|(a, m)| AtomInput {
            atomic_number: u32::from(a.atomic_number),
            isotope: u32::from(a.isotope),
            charge: i32::from(a.charge),
            explicit_h: u32::from(a.explicit_hydrogens),
            no_implicit: a.no_implicit,
            aromatic: a.aromatic,
            radical: u32::from(a.radical_electrons),
            chiral_tag: u32::from(m.chiral_tag),
        })
        .collect();
    let bonds = state
        .graph
        .bonds
        .iter()
        .zip(&state.metadata.bonds)
        .map(|(b, m)| BondInput {
            a: b.a,
            b: b.b,
            order: if b.aromatic { 12 } else { u32::from(b.order) },
            aromatic: b.aromatic,
            stereo: u32::from(m.stereo),
            stereo_atoms: match m.stereo_atoms.as_slice() {
                [a, c] => Some([*a, *c]),
                _ => None,
            },
        })
        .collect();
    Request {
        atoms,
        bonds,
        field,
        operation: Operation::Generate,
        coordinates: Vec::new(),
        fixed_atoms: Vec::new(),
        conformers: 4,
        seed: SEED,
        max_iterations: 500,
    }
}

fn fixture(path: &str) -> Request {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../native/geometry/tests/fixtures/");
    serde_json::from_str(&std::fs::read_to_string(format!("{root}{path}")).unwrap()).unwrap()
}

fn c60_reference() -> Vec<P> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../native/geometry/tests/fixtures/");
    let value: Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{root}c60-reference.json")).unwrap())
            .unwrap();
    serde_json::from_value(value["coordinates"].clone()).unwrap()
}

/// Deterministic perturbation (LCG) so Relax has work to do.
fn perturbed(points: &[P], amplitude: f64) -> Vec<P> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    points
        .iter()
        .map(|p| {
            std::array::from_fn(|axis| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let unit = (state >> 11) as f64 / (1u64 << 53) as f64;
                p[axis] + amplitude * (2.0 * unit - 1.0)
            })
        })
        .collect()
}

fn centered(points: &[P]) -> Vec<P> {
    let n = points.len() as f64;
    let c: P = std::array::from_fn(|k| points.iter().map(|p| p[k]).sum::<f64>() / n);
    points.iter().map(|p| std::array::from_fn(|k| p[k] - c[k])).collect()
}

/// Optimal-superposition RMSD (Horn quaternion; Jacobi eigen-solver).
fn kabsch_rmsd(a: &[P], b: &[P]) -> Option<f64> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let (a, b) = (centered(a), centered(b));
    let mut s = [[0.0; 3]; 3];
    let mut e0 = 0.0;
    for (p, q) in a.iter().zip(&b) {
        for i in 0..3 {
            for j in 0..3 {
                s[i][j] += p[i] * q[j];
            }
            e0 += p[i] * p[i] + q[i] * q[i];
        }
    }
    let [[sxx, sxy, sxz], [syx, syy, syz], [szx, szy, szz]] = s;
    let mut m = [
        [sxx + syy + szz, syz - szy, szx - sxz, sxy - syx],
        [syz - szy, sxx - syy - szz, sxy + syx, szx + sxz],
        [szx - sxz, sxy + syx, -sxx + syy - szz, syz + szy],
        [sxy - syx, szx + sxz, syz + szy, -sxx - syy + szz],
    ];
    for _ in 0..100 {
        let mut off = 0.0;
        for i in 0..4 {
            for j in i + 1..4 {
                off += m[i][j] * m[i][j];
            }
        }
        if off < 1e-24 {
            break;
        }
        for p in 0..4 {
            for q in p + 1..4 {
                if m[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (m[q][q] - m[p][p]) / (2.0 * m[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..4 {
                    let (mkp, mkq) = (m[k][p], m[k][q]);
                    m[k][p] = c * mkp - s * mkq;
                    m[k][q] = s * mkp + c * mkq;
                }
                for k in 0..4 {
                    let (mpk, mqk) = (m[p][k], m[q][k]);
                    m[p][k] = c * mpk - s * mqk;
                    m[q][k] = s * mpk + c * mqk;
                }
            }
        }
    }
    let lambda = (0..4).map(|i| m[i][i]).fold(f64::MIN, f64::max);
    Some(((e0 - 2.0 * lambda).max(0.0) / a.len() as f64).sqrt())
}

fn heavy(request: &Request, points: &[P]) -> Vec<P> {
    request
        .atoms
        .iter()
        .zip(points)
        .filter(|(a, _)| a.atomic_number != 1)
        .map(|(_, p)| *p)
        .collect()
}

fn round(x: f64) -> Value {
    // Keep full precision; JSON comparison applies its own tolerance.
    if x.is_finite() { json!(x) } else { json!(x.to_string()) }
}

fn grad_rms(response: &Response) -> Value {
    response.gradient.as_ref().map_or(json!(null), |g| {
        let sum: f64 = g.iter().flatten().map(|x| x * x).sum();
        round((sum / g.len().max(1) as f64).sqrt())
    })
}

fn counters(diagnostics: &[String]) -> Value {
    diagnostics
        .iter()
        .find_map(|line| {
            let start = line.find("embedding_failure_counters=")?;
            Some(json!(line[start + "embedding_failure_counters=".len()..].to_owned()))
        })
        .unwrap_or(json!(null))
}

/// ETKDG counter 11 (ExceededTimeout) means the result depends on wall time.
fn timing_sensitive(diagnostics: &[String]) -> bool {
    diagnostics.iter().any(|line| {
        line.split(|c: char| c == ' ' || c == '=' || c == ';')
            .any(|token| token.starts_with("11:"))
    })
}

fn run(request: &Request) -> (Result<Response, String>, f64) {
    let started = Instant::now();
    let result = guard(|| solve(request))
        .unwrap_or_else(|_| Err("harness: solve panicked".to_owned()));
    (result, started.elapsed().as_secs_f64() * 1e3)
}

fn summary(request: &Request, result: &Result<Response, String>, elapsed: f64) -> Value {
    match result {
        Err(e) => json!({"ok": false, "error": e, "elapsed_ms": elapsed}),
        Ok(r) => {
            let mut v = json!({
                "ok": true, "initial_energy": round(r.initial_energy), "energy": round(r.energy),
                "converged": r.converged, "atoms_out": r.coordinates.len(),
                "original_count": r.original_count, "hydrogens_added": r.hydrogen_parents.len(),
                "hydrogen_parents": r.hydrogen_parents, "gradient_rms": grad_rms(r),
                "iterations": null, "iteration_limit": request.max_iterations,
                "embedding_failure_counters": counters(&r.diagnostics),
                "timing_sensitive": timing_sensitive(&r.diagnostics),
                "diagnostics": r.diagnostics, "elapsed_ms": elapsed,
            });
            if request.operation == Operation::Generate && !request.coordinates.is_empty() {
                v["heavy_rmsd_vs_seed"] = json!(kabsch_rmsd(
                    &heavy(request, &request.coordinates),
                    &heavy(request, &r.coordinates[..request.atoms.len()])
                ));
            }
            v
        }
    }
}

/// Generate, Evaluate at the result, then Relax from a perturbed copy.
fn geometry_case(name: &str, base: &Request, fields: &[ForceField]) -> Value {
    let mut per_field = serde_json::Map::new();
    let mut common: Option<Vec<P>> = None;
    for &field in fields {
        let mut request = base.clone();
        request.field = field;
        let (generated, elapsed) = run(&request);
        let mut entry = json!({"generate": summary(&request, &generated, elapsed)});
        if let Ok(g) = &generated {
            common.get_or_insert_with(|| g.coordinates.clone());
            let mut eval = request.clone();
            eval.operation = Operation::Evaluate;
            eval.coordinates = g.coordinates.clone();
            let (evaluated, elapsed) = run(&eval);
            entry["evaluate_generated"] = summary(&eval, &evaluated, elapsed);
            let mut relax = request.clone();
            relax.operation = Operation::Relax;
            relax.coordinates = perturbed(&g.coordinates, 0.15);
            relax.max_iterations = 2000;
            let (relaxed, elapsed) = run(&relax);
            entry["relax_perturbed"] = summary(&relax, &relaxed, elapsed);
            if let Ok(r) = &relaxed {
                let n = request.atoms.len();
                entry["relax_perturbed"]["heavy_rmsd_vs_generated"] = json!(kabsch_rmsd(
                    &heavy(&request, &g.coordinates[..n]),
                    &heavy(&request, &r.coordinates[..n])
                ));
                entry["relax_perturbed"]["heavy_rmsd_vs_input"] = json!(kabsch_rmsd(
                    &heavy(&request, &relax.coordinates[..n]),
                    &heavy(&request, &r.coordinates[..n])
                ));
            }
        }
        per_field.insert(format!("{field:?}"), entry);
    }
    // Same coordinates, every field: isolates force-field energy changes from
    // embedding changes. Uses the first field's generated geometry.
    let mut common_eval = serde_json::Map::new();
    if let Some(coords) = &common {
        for &field in fields {
            let mut eval = base.clone();
            eval.field = field;
            eval.operation = Operation::Evaluate;
            eval.coordinates = coords.clone();
            let (evaluated, elapsed) = run(&eval);
            common_eval.insert(format!("{field:?}"), summary(&eval, &evaluated, elapsed));
        }
    }
    json!({"name": name, "atoms": base.atoms.len(), "bonds": base.bonds.len(),
        "conformers": base.conformers, "seed": base.seed, "fields": per_field,
        "evaluate_common_geometry": common_eval})
}

fn geometry_corpus() -> Vec<(String, Request, Vec<ForceField>)> {
    let mut cases = Vec::new();
    for s in [
        "c1ccccc1",
        "c1ccc2ccccc2c1",
        "CC(=O)NC",
        "CCO",
        "C1CCCCC1",
        "N[C@@H](C)C(=O)O",
        "C/C=C/C",
        "Cl/C=C\\Cl",
        "c1ccncc1",
        "Cn1cnc2c1c(=O)n(C)c(=O)n2C",
        "CC(=O)Oc1ccccc1C(=O)O",
        "CC(=O)[O-]",
        "C[N+](C)(C)C",
        "FS(F)(F)(F)(F)F",
        "C[Sn](C)(C)C",
    ] {
        let state = smiles::read(s).unwrap().prepared.state;
        cases.push((format!("smiles/{s}"), request_from_state(&state, ForceField::MMFF94), FIELDS.to_vec()));
    }
    // The eight-conformer fixtures reach ETKDG's wall-clock embedding timeout
    // (counter 11), so their Generate output depends on machine speed. The
    // single-conformer variants are the deterministic comparison points.
    for (name, path) in [("taxol70", "taxol70-request.json"), ("user-c36", "user-c36-request.json")] {
        let request = fixture(path);
        let mut single = request.clone();
        single.conformers = 1;
        cases.push((format!("fixture/{name}-1conf"), single, FIELDS.to_vec()));
        cases.push((format!("fixture/{name}-8conf"), request, FIELDS.to_vec()));
    }
    cases.push(("fixture/c60".into(), fixture("c60-request.json"), FIELDS.to_vec()));
    let mut seeded = fixture("c60-request.json");
    seeded.coordinates = c60_reference();
    cases.push(("fixture/c60-seeded-reference-xyz".into(), seeded, FIELDS.to_vec()));
    cases
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::var("BUMP_OUT").map_err(|_| "set BUMP_OUT to the output JSON path")?;
    let only = std::env::var("BUMP_ONLY").unwrap_or_default();
    let started = Instant::now();
    let mut root = json!({
        "harness": "bump_compare v1",
        "inchi_version": inchi::INCHI_VERSION,
        "cosmolkit_inchi_crate": kernel::CRATE_VERSION,
        "cosmolkit_geometry_crate": reshiki_geometry::COSMOLKIT_VERSION,
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
    });
    if only.is_empty() || only == "inchi" {
        let cases: Vec<_> = inchi_corpus()
            .iter()
            .map(|(name, text, m)| {
                eprintln!("inchi {}", name.chars().take(80).collect::<String>());
                inchi_case(name, text, *m)
            })
            .collect();
        root["inchi"] = json!(cases);
        root["inchi_read"] = json!(
            inchi_read_corpus()
                .into_iter()
                .map(|t| json!({"input": t, "read": read_back(t), "inchikey": match key::from_inchi(t) {
                    Ok(k) => json!(k), Err(e) => json!(format!("error: {e}")) }}))
                .collect::<Vec<_>>()
        );
    }
    if only.is_empty() || only == "geometry" {
        let cases: Vec<_> = geometry_corpus()
            .iter()
            .map(|(name, request, fields)| {
                eprintln!("geometry {name}");
                geometry_case(name, request, fields)
            })
            .collect();
        root["geometry"] = json!(cases);
    }
    root["total_elapsed_ms"] = json!(started.elapsed().as_secs_f64() * 1e3);
    std::fs::write(&out, serde_json::to_string_pretty(&root)? + "\n")?;
    eprintln!("wrote {out}");
    Ok(())
}

fn guard<T>(f: impl FnOnce() -> T) -> std::thread::Result<T> {
    panic::catch_unwind(panic::AssertUnwindSafe(f))
}
