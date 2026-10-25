use super::*;
use crate::document::{DISTANCE_BONDS, MODELS_3D, Point, VERSION};
use crate::models3d::{Display, Model3d};

const NACL: &str = include_str!("../../../../tests/fixtures/crystal/nacl.cif");
const CO2: &str = include_str!("../../../../tests/fixtures/crystal/co2.cif");
const UREA: &str = include_str!("../../../../tests/fixtures/crystal/urea.pdb");
const RUTILE: &str = include_str!("../../../../tests/fixtures/crystal/rutile.vasp");
const WATER: &str = include_str!("../../../../tests/fixtures/crystal/water.xyz");
const SILICON: &str = include_str!("../../../../tests/fixtures/crystal/silicon.extxyz");

fn open(format: &str, text: &str) -> (Document, Vec<String>) {
    read(format, text).unwrap()
}

fn model(doc: &Document) -> &Model3d {
    assert_eq!(doc.models.len(), 1);
    &doc.models[0]
}

fn count(doc: &Document, element: &str) -> usize {
    doc.atoms.iter().filter(|a| a.element == element).count()
}

fn xyz(model: &Model3d, id: u64) -> [f32; 3] {
    model.atoms.iter().find(|a| a.id == id).unwrap().xyz
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>().sqrt()
}

/// Fractional coordinates of a model atom in its cell.
fn fractional(model: &Model3d, id: u64) -> [f64; 3] {
    let cell = model.cell.as_ref().unwrap();
    let lattice = build::Lattice::new(cell.vectors.map(|v| v.map(f64::from))).unwrap();
    lattice.fractional(xyz(model, id).map(f64::from))
}

/// Bonds of an atom: inside the drawing and leaving the cell.
fn bonds_of(doc: &Document, id: u64) -> usize {
    let inside = doc.bonds.iter().filter(|b| b.a == id || b.b == id).count();
    let leaving = model(doc).cell.as_ref().map_or(0, |c| {
        c.crossing.iter().filter(|x| x.a == id).count()
            + c.crossing.iter().filter(|x| x.b == id).count()
    });
    inside + leaving
}

#[test]
fn nacl_keeps_each_atom_once_with_bonds_leaving_the_cell() {
    let (doc, warnings) = open("cif", NACL);
    assert!(warnings.is_empty(), "{warnings:?}");
    let m = model(&doc);
    assert_eq!((count(&doc, "Na"), count(&doc, "Cl")), (4, 4));
    assert_eq!(m.display, Display::TwoD);
    assert_eq!(doc.version, DISTANCE_BONDS);
    for atom in &doc.atoms {
        assert!(
            fractional(m, atom.id)
                .iter()
                .all(|f| (-1e-3..1.).contains(f))
        );
        // Rock salt: every ion has six neighbours, inside the cell or not.
        assert_eq!(bonds_of(&doc, atom.id), 6, "{}", atom.element);
        assert_eq!(atom.charge, if atom.element == "Na" { 1 } else { -1 });
    }
    let cell = m.cell.as_ref().unwrap();
    assert_eq!(doc.bonds.len() + cell.crossing.len(), 24);
    assert!(!cell.crossing.is_empty());
    assert!((cell.vectors[0][0] - 5.6402).abs() < 1e-4);
    let labels: Vec<_> = m.sites.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels.iter().filter(|l| **l == "Na1").count(), 4);
    assert!(
        m.sites
            .iter()
            .all(|s| matches!(s.adp, Some(Adp::Iso(u)) if u > 0.01))
    );
}

#[test]
fn co2_keeps_whole_molecules_with_centres_in_the_cell() {
    let (doc, _) = open("cif", CO2);
    let m = model(&doc);
    assert_eq!((count(&doc, "C"), count(&doc, "O")), (4, 8));
    assert_eq!(doc.bonds.len(), 8);
    // O=C=O.
    assert!(doc.bonds.iter().all(|b| b.order == 2));
    assert!(m.cell.as_ref().unwrap().crossing.is_empty());
    for carbon in doc.atoms.iter().filter(|a| a.element == "C") {
        let oxygens: Vec<_> = doc
            .bonds
            .iter()
            .filter_map(|b| match (b.a == carbon.id, b.b == carbon.id) {
                (true, _) => Some(b.b),
                (_, true) => Some(b.a),
                _ => None,
            })
            .collect();
        assert_eq!(oxygens.len(), 2);
        for o in oxygens {
            // Unwrapped: the bond is 1.15 Å, not across the cell.
            let d = distance(xyz(m, carbon.id), xyz(m, o));
            assert!((d - 1.154).abs() < 0.01, "{d}");
        }
        assert!(
            fractional(m, carbon.id)
                .iter()
                .all(|f| (-1e-3..1.).contains(f))
        );
    }
    // Oxygen sticks out of the cell where its molecule sits on a corner.
    assert!(
        doc.atoms
            .iter()
            .any(|a| fractional(m, a.id).iter().any(|f| *f < -0.05))
    );
    let oxygen_adps: Vec<_> = m
        .sites
        .iter()
        .filter(|s| s.label == "O1")
        .map(|s| s.adp.unwrap())
        .collect();
    assert_eq!(oxygen_adps.len(), 8);
    for adp in oxygen_adps {
        let Adp::Aniso(u) = adp else {
            panic!("{adp:?}")
        };
        assert_eq!(&u[..3], &[0.045; 3]);
        // A mirror or inversion through an axis flips off-diagonal signs.
        assert!(u[3..].iter().all(|v| (v.abs() - 0.010).abs() < 1e-6));
    }
    assert!(m.sites.iter().any(|s| s.adp == Some(Adp::Iso(0.030))));
}

#[test]
fn pdb_reads_cryst1_and_a_whole_molecule() {
    let (doc, _) = open("pdb", UREA);
    let m = model(&doc);
    assert_eq!(doc.atoms.len(), 8);
    assert_eq!(doc.bonds.len(), 7);
    // C=O is double; C–N and N–H stay single.
    let double: Vec<_> = doc.bonds.iter().filter(|b| b.order == 2).collect();
    assert_eq!(double.len(), 1);
    let ends = [double[0].a, double[0].b].map(|id| doc.atom(id).unwrap().element.clone());
    assert!(ends.contains(&"C".into()) && ends.contains(&"O".into()));
    let cell = m.cell.as_ref().unwrap();
    assert!((cell.vectors[2][2] - 4.684).abs() < 1e-4);
    assert!(cell.crossing.is_empty());
    assert_eq!(m.sites.first().unwrap().label, "C1");
    let n = m.atoms.len() as f64;
    let centre = (0..3)
        .map(|k| {
            doc.atoms
                .iter()
                .map(|a| fractional(m, a.id)[k])
                .sum::<f64>()
                / n
        })
        .collect::<Vec<_>>();
    assert!(centre.iter().all(|f| (-1e-3..1.).contains(f)), "{centre:?}");
}

#[test]
fn poscar_reads_direct_positions_and_keeps_metals_apart_in_oxides() {
    let (doc, _) = open("poscar", RUTILE);
    let m = model(&doc);
    assert_eq!((count(&doc, "Ti"), count(&doc, "O")), (2, 4));
    for atom in &doc.atoms {
        assert_eq!(
            bonds_of(&doc, atom.id),
            if atom.element == "Ti" { 6 } else { 3 }
        );
    }
    let titanium: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| a.element == "Ti")
        .map(|a| a.id)
        .collect();
    assert!(
        doc.bonds
            .iter()
            .all(|b| !(titanium.contains(&b.a) && titanium.contains(&b.b)))
    );
    assert!(m.sites.is_empty());
}

#[test]
fn poscar_reads_cartesian_scaled_positions_and_selective_dynamics() {
    let text = "scaled\n2.0\n1 0 0\n0 1 0\n0 0 1\nNa_pv Cl\n1 1\nSelective dynamics\nCartesian\n0 0 0 T T T\n0.5 0.5 0.5 T T T\n";
    let (doc, _) = open("poscar", text);
    let m = model(&doc);
    assert!((m.cell.as_ref().unwrap().vectors[0][0] - 2.).abs() < 1e-6);
    assert!((distance(m.atoms[0].xyz, m.atoms[1].xyz) - 3f32.sqrt()).abs() < 1e-4);
    assert_eq!(doc.atoms[0].element, "Na");
}

#[test]
fn plain_xyz_is_a_molecule_without_a_cell() {
    let (doc, _) = open("xyz", WATER);
    let m = model(&doc);
    assert_eq!(doc.bonds.len(), 2);
    assert!(m.cell.is_none());
    assert_eq!(doc.version, MODELS_3D);
    assert_eq!(m.display, Display::ThreeD);
    // The 2D layout comes from the 3D view, with depth kept.
    assert!(doc.atoms.iter().all(|a| a.position.is_finite()));
    assert!(doc.atoms.iter().any(|a| a.depth != 0.));
}

#[test]
fn extended_xyz_reads_the_lattice_and_properties() {
    let (doc, _) = open("xyz", SILICON);
    let m = model(&doc);
    let cell = m.cell.as_ref().unwrap();
    assert_eq!(doc.atoms.len(), 2);
    // Diamond: four neighbours each, three of them in other cells.
    assert_eq!(doc.bonds.len() + cell.crossing.len(), 4);
    for atom in &doc.atoms {
        assert_eq!(bonds_of(&doc, atom.id), 4);
    }
}

#[test]
fn saved_drawings_keep_the_structure_and_drop_the_rest_of_the_file() {
    for (format, text) in [("cif", NACL), ("cif", CO2), ("xyz", SILICON), ("pdb", UREA)] {
        let (doc, _) = open(format, text);
        let bytes = doc.file_json().unwrap();
        let saved = String::from_utf8(bytes.clone()).unwrap();
        for dropped in [
            "DROPPED", "journal", "refln", "energy", "forces", "REMARK", "symop",
        ] {
            assert!(!saved.contains(dropped), "{format}: {dropped}");
        }
        let read = Document::from_json(&bytes).unwrap();
        assert_eq!(read.version, VERSION);
        assert_eq!(read.models, doc.models);
        assert_eq!(read.atoms, doc.atoms);
    }
}

#[test]
fn symmetry_operators_parse_in_their_usual_spellings() {
    let (r, t) = cif::parse_operator("-x+1/2, y, -z").unwrap();
    assert_eq!(r, [[-1., 0., 0.], [0., 1., 0.], [0., 0., -1.]]);
    assert_eq!(t, [0.5, 0., 0.]);
    let (r, t) = cif::parse_operator("1/2+X,Y-X,0.25+z").unwrap();
    assert_eq!(r, [[1., 0., 0.], [-1., 1., 0.], [0., 0., 1.]]);
    assert_eq!(t, [0.5, 0., 0.25]);
    for bad in ["x,y", "x,y,z,x", "x,y,a", "2x,y,z", "x,x,z"] {
        assert!(cif::parse_operator(bad).is_none(), "{bad}");
    }
}

const CELL: &str = "data_t\n_cell_length_a 4\n_cell_length_b 4\n_cell_length_c 4\n_cell_angle_alpha 90\n_cell_angle_beta 90\n_cell_angle_gamma 90\n";

#[test]
fn disorder_keeps_the_first_alternative() {
    let text = format!(
        "{CELL}_space_group_name_H-M_alt 'P 1'\nloop_\n_atom_site_label\n_atom_site_fract_x\n_atom_site_fract_y\n_atom_site_fract_z\n_atom_site_disorder_group\nC1 0.1 0.1 0.1 .\nO1A 0.4 0.1 0.1 1\nO1B 0.1 0.4 0.1 2\nN1 0.6 0.6 0.6 2\n"
    );
    let (doc, warnings) = open("cif", &text);
    let labels: Vec<_> = model(&doc).sites.iter().map(|s| s.label.clone()).collect();
    assert_eq!(labels, ["C1", "O1A"]);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}

#[test]
fn atoms_on_the_cell_boundary_are_kept_once() {
    let text = format!(
        "{CELL}loop_\n_atom_site_label\n_atom_site_fract_x\n_atom_site_fract_y\n_atom_site_fract_z\nAr1 0 0 0\nAr2 1.0 0 0\nAr3 0.99999 1 1\n"
    );
    let (doc, warnings) = open("cif", &text);
    assert_eq!(doc.atoms.len(), 1);
    assert!(
        warnings.iter().any(|w| w.contains("no symmetry operators")),
        "{warnings:?}"
    );
}

#[test]
fn b_values_are_converted_to_u() {
    let text = format!(
        "{CELL}loop_\n_atom_site_label\n_atom_site_fract_x\n_atom_site_fract_y\n_atom_site_fract_z\n_atom_site_B_iso_or_equiv\nFe1 0 0 0 7.8957\nFe2 0.5 0.5 0.5 ?\nloop_\n_atom_site_aniso_label\n_atom_site_aniso_B_11\n_atom_site_aniso_B_22\n_atom_site_aniso_B_33\n_atom_site_aniso_B_12\n_atom_site_aniso_B_13\n_atom_site_aniso_B_23\nFe2 7.8957 7.8957 7.8957 0 0 0\n"
    );
    let (doc, _) = open("cif", &text);
    let sites = &model(&doc).sites;
    let Some(Adp::Iso(u)) = sites[0].adp else {
        panic!()
    };
    assert!((u - 0.1).abs() < 1e-4);
    let Some(Adp::Aniso(u)) = sites[1].adp else {
        panic!()
    };
    assert!((u[0] - 0.1).abs() < 1e-4);
}

#[test]
fn invalid_files_are_refused_with_a_reason() {
    let atoms =
        "loop_\n_atom_site_label\n_atom_site_fract_x\n_atom_site_fract_y\n_atom_site_fract_z\n";
    for (format, text, message) in [
        (
            "cif",
            "data_x\n_cell_length_b 3\n".to_owned(),
            "no unit cell",
        ),
        (
            "cif",
            format!("{CELL}{atoms}C1 0 0\n"),
            "not a whole number of rows",
        ),
        ("cif", format!("{CELL};\nopen text\n"), "not closed"),
        (
            "cif",
            format!("{CELL}{atoms}Q1 0 0 0\n"),
            "element of atom 'Q1'",
        ),
        (
            "cif",
            format!("{CELL}{atoms}C1 0 ? 0\n"),
            "no fractional coordinates",
        ),
        (
            "cif",
            CELL.replace("_cell_angle_gamma 90", "_cell_angle_gamma 190") + atoms,
            "between 0° and 180°",
        ),
        (
            "cif",
            format!("{CELL}loop_\n_symmetry_equiv_pos_as_xyz\n'x,y,q'\n{atoms}C1 0 0 0\n"),
            "symmetry operator",
        ),
        ("cif", CELL.to_owned(), "no atom sites"),
        (
            "pdb",
            "CRYST1    5.000    5.000    5.000  90.00  90.00  90.00 P 1\n".to_owned(),
            "no ATOM",
        ),
        (
            "poscar",
            "x\n1\n1 0 0\n0 1 0\n0 0 1\n1 1\nDirect\n0 0 0\n".to_owned(),
            "VASP 4",
        ),
        (
            "poscar",
            "x\n1\n1 0 0\n0 1 0\n0 0 1\nNa\n2\nDirect\n0 0 0\n".to_owned(),
            "ends before",
        ),
        (
            "poscar",
            "x\n1\n1 0 0\n0 1 0\n0 0 0\nNa\n1\nDirect\n0 0 0\n".to_owned(),
            "no volume",
        ),
        ("xyz", "two\n\nC 0 0 0\n".to_owned(), "number of atoms"),
        ("xyz", "1\n\nXx 0 0 0\n".to_owned(), "not an element"),
        (
            "xyz",
            "1\nLattice=\"1 0 0 0 1 0\"\nC 0 0 0\n".to_owned(),
            "nine numbers",
        ),
        ("xyz", "2\n\nC 0 0 0\n".to_owned(), "ends before"),
    ] {
        let error = read(format, &text).unwrap_err().to_string();
        assert!(error.contains(message), "{format} {text:?}: {error}");
    }
}

#[test]
fn file_names_pick_the_format() {
    for (name, format) in [
        ("NaCl.CIF", Some("cif")),
        ("1abc.pdb", Some("pdb")),
        ("POSCAR", Some("poscar")),
        ("CONTCAR_relaxed", Some("poscar")),
        ("rutile.vasp", Some("poscar")),
        ("water.xyz", Some("xyz")),
        ("si.extxyz", Some("xyz")),
        ("drawing.mol", None),
    ] {
        assert_eq!(format_of(name), format, "{name}");
    }
}

#[test]
fn the_model_frame_holds_the_cell() {
    let (doc, _) = open("cif", NACL);
    let m = model(&doc);
    let (lo, hi) = crate::models3d::frame(&doc, m).unwrap();
    // The cell's diagonal is 9.77 Å at 28 drawing units per ångström.
    assert!(hi.x - lo.x > 9.77 * 28. * 0.99, "{lo:?} {hi:?}");
}

#[test]
fn the_import_engine_reads_crystal_files_and_keeps_the_model() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    for (format, text) in [
        ("cif", NACL),
        ("pdb", UREA),
        ("poscar", RUTILE),
        ("xyz", SILICON),
    ] {
        let doc = runtime
            .block_on(crate::formats::read(text.as_bytes().to_vec(), format))
            .unwrap();
        assert_eq!(doc.models.len(), 1, "{format}");
        assert_eq!(doc.models[0].display, Display::TwoD);
        assert!(doc.models[0].cell.is_some());
    }
    let error = runtime
        .block_on(crate::formats::read(b"data_x\n".to_vec(), "cif"))
        .unwrap_err();
    assert!(error.contains("no unit cell"), "{error}");
}

#[test]
fn cif_reads_occupancy_assemblies_and_text_fields_by_the_syntax() {
    let atoms = "loop_\n_atom_site_label\n_atom_site_fract_x\n_atom_site_fract_y\n_atom_site_fract_z\n_atom_site_occupancy\n_atom_site_disorder_assembly\n_atom_site_disorder_group\n";
    let text = format!(
        "{CELL}_audit_creation_method loop_based\n_publ_section_comment\n;\nnote\n; _space_group_name_H-M_alt 'P 1'\n{atoms}C0 .2 .2 .2 0 . .\nO1 .2 .2 .2 1 . .\nN1A .5 .1 .1 .5 A 1\nN1B .1 .5 .1 .5 A 2\nS1C .1 .1 .5 .5 B 3\nS1D .7 .7 .5 .5 B 4\n"
    );
    let (doc, warnings) = open("cif", &text);
    let labels: Vec<_> = model(&doc).sites.iter().map(|s| s.label.as_str()).collect();
    // The unoccupied carbon is left out, and each assembly keeps its first group.
    assert_eq!(labels, ["O1", "N1A", "S1C"]);
    // The space group after the closing semicolon was read: no P1 warning.
    assert!(
        !warnings.iter().any(|w| w.contains("symmetry")),
        "{warnings:?}"
    );
    let huge = format!(
        "{CELL}_space_group_name_H-M_alt 'P 1'\nloop_\n_atom_site_label\n_atom_site_fract_x\n_atom_site_fract_y\n_atom_site_fract_z\n_atom_site_disorder_group\nC1 0 0 0 -9223372036854775808\n"
    );
    assert_eq!(open("cif", &huge).0.atoms.len(), 1);
}

#[test]
fn pdb_keeps_the_first_alternate_location_of_each_residue() {
    let atom = |serial: u32, name: &str, alt: char, residue: u32, x: f64| {
        format!(
            "HETATM{serial:5} {name:<4}{alt}LIG A{residue:4}    {x:8.3}{:8.3}{:8.3}  0.50 10.00           C\n",
            0., 0.
        )
    };
    let text = [
        atom(1, "C1", 'A', 1, 0.),
        atom(2, "C1", 'B', 1, 0.5),
        atom(3, "C2", 'C', 2, 5.),
        atom(4, "C2", 'D', 2, 5.5),
    ]
    .concat();
    let (doc, warnings) = open("pdb", &text);
    assert_eq!(doc.atoms.len(), 2);
    assert!(warnings[0].contains("left out 2 atoms"), "{warnings:?}");
}

#[test]
fn poscar_takes_three_scale_factors() {
    let text = "xyz\n1 2 3\n1 0 0\n0 1 0\n0 0 1\nAr\n1\nCartesian\n0.5 0.5 0.5\n";
    let (doc, _) = open("poscar", text);
    let m = model(&doc);
    let cell = m.cell.as_ref().unwrap();
    assert_eq!(cell.vectors, [[1., 0., 0.], [0., 2., 0.], [0., 0., 3.]]);
    assert_eq!(m.atoms[0].xyz, [0.5, 1., 1.5]);
    let error = read(
        "poscar",
        "x\n1 2\n1 0 0\n0 1 0\n0 0 1\nAr\n1\nDirect\n0 0 0\n",
    )
    .unwrap_err();
    assert!(error.to_string().contains("three positive"), "{error}");
}

#[test]
fn skewed_cells_find_every_copy_or_say_why() {
    // A cubic 10 Å lattice written with a long, skewed b = 3a + 10 ŷ: the
    // point 0.07 Å from b − 3a is the same site as the origin.
    let lattice = build::Lattice::new([[10., 0., 0.], [30., 10., 0.], [0., 0., 10.]]).unwrap();
    assert!(build::same_site(&lattice, [0.; 3], [0.05, 9.95, 0.]));
    assert!(!build::same_site(&lattice, [0.; 3], [0.5, 9.5, 0.]));
    // Bonds to copies more than three cells away are refused, not dropped.
    let text = "Fe\n1\n2.8 0 0\n14 2.8 0\n0 0 2.8\nFe\n1\nDirect\n0 0 0\n";
    let error = read("poscar", text).unwrap_err().to_string();
    assert!(error.contains("reduced"), "{error}");
    let huge = "1\nProperties=x:R:18446744073709551615:species:S:1:pos:R:3\nC 0 0 0\n";
    assert!(
        read("xyz", huge)
            .unwrap_err()
            .to_string()
            .contains("Properties")
    );
}

#[test]
fn a_crystal_formula_counts_its_atoms_without_chemistry() {
    let (doc, _) = open("cif", NACL);
    assert!(crate::models3d::crystal::present(&doc));
    let all = crate::models3d::crystal::composition(&doc, &[]).unwrap();
    assert_eq!(all.formula, "Cl4Na4");
    assert!((all.mass - 4. * 58.443).abs() < 0.05, "{}", all.mass);
    let (silicon, _) = open("xyz", SILICON);
    let one = crate::models3d::crystal::composition(&silicon, &[silicon.atoms[0].id]).unwrap();
    assert_eq!(one.formula, "Si");
}

#[test]
fn files_cannot_ask_for_endless_work() {
    // A cell almost flat across one face is refused before any search.
    let flat = "2\nLattice=\"999 0 0 999 0.000001 0 0 0 999\"\nC 0 0 0\nC 0 0 1\n";
    assert!(
        read("xyz", flat)
            .unwrap_err()
            .to_string()
            .contains("thinner")
    );
    // Repeated operators count once; too many distinct ones, or too many
    // entries at all, are refused.
    let atoms = "loop_\n_atom_site_label\n_atom_site_fract_x\n_atom_site_fract_y\n_atom_site_fract_z\nC1 0.1 0.2 0.3\n";
    let repeated = format!(
        "{CELL}loop_\n_symmetry_equiv_pos_as_xyz\n{}{atoms}",
        "'x,y,z'\n'x+1,y,z'\n".repeat(700)
    );
    let (doc, _) = open("cif", &repeated);
    assert_eq!(doc.atoms.len(), 1);
    let many: String = (0..200).map(|i| format!("'x+{}/400,y,z'\n", i)).collect();
    let error = read(
        "cif",
        &format!("{CELL}loop_\n_symmetry_equiv_pos_as_xyz\n{many}{atoms}"),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("at most 192"), "{error}");
    let flood: String = (0..100_000)
        .map(|i| format!("'x+{i}/200000,y,z'\n"))
        .collect();
    let error = read(
        "cif",
        &format!("{CELL}loop_\n_symmetry_equiv_pos_as_xyz\n{flood}{atoms}"),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("100000 symmetry operators"), "{error}");
}

#[test]
fn extended_xyz_follows_its_periodic_flags() {
    let text = |pbc: &str| {
        format!("2\nLattice=\"10 0 0 0 10 0 0 0 10\" pbc=\"{pbc}\"\nC 0.1 5 5\nC 9.9 5 5\n")
    };
    // Not periodic: a molecule of two far-apart atoms, no cell.
    let (doc, _) = open("xyz", &text("F F F"));
    assert_eq!(doc.atoms.len(), 2);
    assert!(model(&doc).cell.is_none());
    // Periodic: the two atoms are 0.2 Å apart across the boundary, one site.
    let (doc, _) = open("xyz", &text("T T T"));
    assert_eq!(doc.atoms.len(), 1);
    let error = read("xyz", &text("T T F")).unwrap_err().to_string();
    assert!(error.contains("pbc"), "{error}");
}

#[test]
fn only_crystal_atoms_skip_chemistry_analysis() {
    let (mut doc, _) = open("cif", NACL);
    let ethane = [
        doc.add_atom("C", Point::new(500., 0.)),
        doc.add_atom("C", Point::new(540., 0.)),
    ];
    doc.add_bond(ethane[0], ethane[1], 1, "plain");
    use crate::models3d::crystal::touches;
    assert!(touches(&doc, &[]));
    assert!(touches(&doc, &[doc.atoms[0].id]));
    assert!(!touches(&doc, &ethane));
}

#[test]
fn molecules_missing_hydrogens_keep_single_bonds() {
    // Urea without its hydrogens: the nitrogens stay short of their valence,
    // so no bond order is guessed.
    let text: String = UREA
        .lines()
        .filter(|l| !l.ends_with('H'))
        .collect::<Vec<_>>()
        .join("\n");
    let (doc, _) = open("pdb", &text);
    assert_eq!(doc.atoms.len(), 4);
    assert!(doc.bonds.iter().all(|b| b.order == 1));
    // Ethane without hydrogens: no bond beyond triple, so it stays single.
    let (doc, _) = open("xyz", "2\nethane carbons\nC 0 0 0\nC 1.54 0 0\n");
    assert_eq!(doc.bonds.len(), 1);
    assert_eq!(doc.bonds[0].order, 1);
    // Networks never take multiple bonds.
    let (doc, _) = open("xyz", SILICON);
    assert!(doc.bonds.iter().all(|b| b.order == 1));
}

#[test]
fn crystals_open_as_2d_drawings_in_their_cell_with_no_overlapping_labels() {
    for (format, text) in [
        ("cif", NACL),
        ("cif", CO2),
        ("poscar", RUTILE),
        ("xyz", SILICON),
        ("pdb", UREA),
    ] {
        let (doc, _) = open(format, text);
        assert_eq!(model(&doc).display, Display::TwoD, "{format}");
        let drawing = crate::scene::primitives(&doc);
        // The cell box: nine solid edges and three dashed hidden ones, all
        // parallel in threes, and the italic a, b and c labels.
        let edges: Vec<_> = drawing
            .iter()
            .filter_map(|p| match p {
                crate::scene::Primitive::Wire { from, to, dash, .. } => Some((*from, *to, *dash)),
                _ => None,
            })
            .collect();
        assert!(edges.iter().filter(|e| e.2 == 0.).count() >= 9, "{format}");
        let labels: Vec<_> = drawing
            .iter()
            .filter_map(|p| match p {
                crate::scene::Primitive::Text { text, style, .. } if style.italic => {
                    Some(text.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(labels, ["a", "b", "c"], "{format}");
        // No two atom labels overlap.
        let drawn = crate::models3d::drawn(&doc);
        let boxes: Vec<_> = drawn
            .atoms
            .iter()
            .filter_map(|a| Some((&a.element, crate::scene::atom_label_ink_bounds(a, &drawn)?)))
            .collect();
        for (i, (a, (lo, hi))) in boxes.iter().enumerate() {
            for (b, (other_lo, other_hi)) in &boxes[i + 1..] {
                let apart = hi.x < other_lo.x
                    || other_hi.x < lo.x
                    || hi.y < other_lo.y
                    || other_hi.y < lo.y;
                assert!(
                    apart,
                    "{format}: {a} {lo:?} {hi:?} overlaps {b} {other_lo:?} {other_hi:?}"
                );
            }
        }
    }
}

#[test]
fn ion_charges_are_hidden_in_2d_until_shown() {
    let (mut doc, _) = open("cif", NACL);
    // Kept on the atoms, for the formula, but not drawn.
    assert!(doc.atoms.iter().all(|a| a.charge != 0));
    assert!(
        crate::models3d::drawn(&doc)
            .atoms
            .iter()
            .all(|a| a.charge == 0)
    );
    doc.models[0].charges = true;
    assert!(
        crate::models3d::drawn(&doc)
            .atoms
            .iter()
            .all(|a| a.charge != 0)
    );
    let read = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert!(read.models[0].charges);
}

const FIXTURES: [(&str, &str); 5] = [
    ("cif", NACL),
    ("cif", CO2),
    ("poscar", RUTILE),
    ("xyz", SILICON),
    ("pdb", UREA),
];

/// The 2D cell's origin and the ends of the three box edges that leave it,
/// from the drawing. The opening view keeps the origin at the back, so those
/// edges are the dashed hidden ones.
fn drawn_cell(drawing: &[crate::scene::Primitive]) -> (Point, [Point; 3]) {
    cell_in(drawing, true)
}

/// [`drawn_cell`], with the origin's edges `hidden` or as they come.
fn cell_in(drawing: &[crate::scene::Primitive], hidden: bool) -> (Point, [Point; 3]) {
    use crate::scene::Primitive;
    let origin = drawing
        .iter()
        .find_map(|p| match p {
            Primitive::Sphere { center, .. } => Some(*center),
            _ => None,
        })
        .unwrap();
    let ends: Vec<_> = drawing
        .iter()
        .filter_map(|p| match p {
            Primitive::Wire { from, to, dash, .. } if from.distance(origin) < 1e-3 => {
                assert!(!hidden || *dash > 0., "a front edge at the origin");
                Some(*to)
            }
            Primitive::Wire { from, to, dash, .. } if to.distance(origin) < 1e-3 => {
                assert!(!hidden || *dash > 0., "a front edge at the origin");
                Some(*from)
            }
            _ => None,
        })
        .collect();
    assert_eq!(ends.len(), 3);
    (origin, [ends[0], ends[1], ends[2]])
}

#[test]
fn atoms_sit_on_the_2d_cell_box_where_their_fractions_put_them() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    for (format, text) in FIXTURES {
        // As opened, and as the import engine hands it to the app.
        let imported = runtime
            .block_on(crate::formats::read(text.as_bytes().to_vec(), format))
            .unwrap();
        for doc in [open(format, text).0, imported] {
            let m = model(&doc);
            let (origin, ends) = drawn_cell(&crate::scene::primitives(&doc));
            // Origin + Σ fₖ (endₖ − origin), with the drawn edges taken in
            // whichever order is a, b, c.
            let at = |f: [f64; 3], order: [usize; 3]| {
                order.iter().zip(f).fold(origin, |p, (&e, t)| {
                    let (end, t) = (ends[e], t as f32);
                    p.offset((end.x - origin.x) * t, (end.y - origin.y) * t)
                })
            };
            let orders = [
                [0, 1, 2],
                [0, 2, 1],
                [1, 0, 2],
                [1, 2, 0],
                [2, 0, 1],
                [2, 1, 0],
            ];
            let worst = orders
                .iter()
                .map(|order| {
                    doc.atoms
                        .iter()
                        .map(|a| at(fractional(m, a.id), *order).distance(a.position))
                        .fold(0., f32::max)
                })
                .fold(f32::INFINITY, f32::min);
            assert!(worst < 0.1, "{format}: atoms are {worst} units off the box");
        }
    }
    // Rock salt's origin Na is on the box's origin corner.
    let (doc, _) = open("cif", NACL);
    let (origin, _) = drawn_cell(&crate::scene::primitives(&doc));
    let m = model(&doc);
    assert!(doc.atoms.iter().any(|a| {
        fractional(m, a.id).iter().all(|f| f.abs() < 1e-3) && a.position.distance(origin) < 0.1
    }));
}

#[test]
fn axis_labels_sit_outside_the_box_clear_of_edges_and_atoms() {
    use crate::scene::Primitive;
    for (format, text) in FIXTURES {
        let (doc, _) = open(format, text);
        let drawing = crate::scene::primitives(&doc);
        let (origin, ends) = drawn_cell(&drawing);
        let drawn = crate::models3d::drawn(&doc);
        let atoms: Vec<_> = drawn
            .atoms
            .iter()
            .filter_map(|a| crate::scene::atom_label_ink_bounds(a, &drawn))
            .collect();
        let edges: Vec<_> = drawing
            .iter()
            .filter_map(|p| match p {
                Primitive::Wire { from, to, .. } => Some((*from, *to)),
                _ => None,
            })
            .collect();
        // The box's centre: half of each edge from the origin.
        let centre = ends.iter().fold(origin, |c, e| {
            c.offset((e.x - origin.x) / 2., (e.y - origin.y) / 2.)
        });
        let mut labels = 0;
        for p in &drawing {
            let Primitive::Text {
                position,
                text,
                size,
                style,
                ..
            } = p
            else {
                continue;
            };
            if !style.italic {
                continue;
            }
            labels += 1;
            let w = crate::style::styled_text_width(text, *size, style);
            let at = position.offset(w / 2., size * 0.6);
            let (lo, hi) = (at.offset(-w / 2., -size / 2.), at.offset(w / 2., size / 2.));
            for (a_lo, a_hi) in &atoms {
                let apart = hi.x < a_lo.x || a_hi.x < lo.x || hi.y < a_lo.y || a_hi.y < lo.y;
                assert!(apart, "{format}: {text} on an atom");
            }
            for (from, to) in &edges {
                let d = segment_distance(at, *from, *to);
                assert!(d > size * 0.5, "{format}: {text} {d} from an edge");
            }
            // Past its axis's end, away from the box.
            let end = ends
                .iter()
                .min_by(|a, b| a.distance(at).total_cmp(&b.distance(at)))
                .unwrap();
            assert!(
                at.distance(centre) > end.distance(centre),
                "{format}: {text} inside the box"
            );
        }
        assert_eq!(labels, 3, "{format}");
    }
}

fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let n = dx * dx + dy * dy;
    let t = if n > 0. {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / n).clamp(0., 1.)
    } else {
        0.
    };
    p.distance(a.offset(t * dx, t * dy))
}

#[test]
fn linear_carbons_show_their_label() {
    let (doc, _) = open("cif", CO2);
    let drawn = crate::models3d::drawn(&doc);
    for carbon in drawn.atoms.iter().filter(|a| a.element == "C") {
        assert!(crate::atom_labels::visible(carbon, &drawn));
        assert!(crate::scene::atom_label_ink_bounds(carbon, &drawn).is_some());
    }
    // Urea's carbon is trigonal and stays a bare vertex.
    let (doc, _) = open("pdb", UREA);
    let carbon = doc.atoms.iter().find(|a| a.element == "C").unwrap();
    assert!(!crate::atom_labels::visible(carbon, &doc));
}

#[test]
fn fitting_and_selecting_a_crystal_hold_its_whole_cell() {
    use crate::scene::Primitive;
    for (format, text) in FIXTURES {
        let (doc, _) = open(format, text);
        let ids: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
        let (lo, hi) = crate::scene::selection_bounds(&doc, &ids).unwrap();
        for p in crate::scene::primitives(&doc) {
            let ends = match p {
                Primitive::Wire { from, to, .. } => [from, to],
                Primitive::Text { position, .. } => [position, position],
                _ => continue,
            };
            for e in ends {
                assert!(
                    (lo.x - 1e-3..=hi.x + 1e-3).contains(&e.x)
                        && (lo.y - 1e-3..=hi.y + 1e-3).contains(&e.y),
                    "{format}: {e:?} outside {lo:?} {hi:?}"
                );
            }
        }
    }
}

/// Dashed stubs, lighter copies and thin seam lines in a drawing.
fn supercell_ink(doc: &Document) -> (usize, usize, usize) {
    use crate::scene::Primitive;
    let drawing = crate::scene::primitives(doc);
    let line = doc.drawing_style.world(doc.drawing_style.line_width_pt);
    let stub = |p: &Primitive| matches!(p, Primitive::Wire { dash, .. } if *dash > 0. && *dash < line * 3.);
    let stubs = drawing
        .iter()
        .filter(|p| match p {
            Primitive::Opacity { primitive, .. } => stub(primitive),
            other => stub(other),
        })
        .count();
    let copies = drawing
        .iter()
        .filter(|p| matches!(p, Primitive::Opacity { alpha, primitive } if (*alpha - 0.55).abs() < 1e-3 && matches!(**primitive, Primitive::Text { .. })))
        .count();
    let seams = drawing
        .iter()
        .filter(|p| matches!(p, Primitive::Wire { width, dash, .. } if *dash == 0. && *width < line * 0.9))
        .count();
    (stubs, copies, seams)
}

#[test]
fn a_supercell_draws_lighter_copies_seams_and_bonds_between_cells() {
    let (mut doc, _) = open("cif", NACL);
    // One cell: its 27 ions as crystallographers draw it, as lighter copies
    // (the model's own atoms' copies lie under them), no seams.
    let (stubs, copies, seams) = supercell_ink(&doc);
    assert_eq!((copies, seams), (27, 0));
    doc.models[0].repeat = [2, 1, 1];
    let (wide_stubs, copies, seams) = supercell_ink(&doc);
    // Two cells: ions at every half step, 5 × 3 × 3; one plane between the
    // cells, four lines.
    assert_eq!(copies, 45);
    assert_eq!(seams, 4);
    // Bonds that leave the whole block are still stubs.
    assert!(stubs > 0 && wide_stubs > 0, "{stubs} {wide_stubs}");
    // The box and its a label reach the far end of both cells.
    let (origin, ends) = drawn_cell(&crate::scene::primitives(&doc));
    let (_, single) = drawn_cell(&crate::scene::primitives(&open("cif", NACL).0));
    let (single_origin, _) = drawn_cell(&crate::scene::primitives(&open("cif", NACL).0));
    let size = |o: Point, e: &[Point; 3]| e.iter().map(|p| p.distance(o)).product::<f32>();
    assert!((size(origin, &ends) / size(single_origin, &single) - 2.).abs() < 0.05);
    // Saved and read back; at most four cells along each axis.
    let read = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(read.models[0].repeat, [2, 1, 1]);
    doc.models[0].repeat = [5, 1, 1];
    assert!(doc.validate().is_err());
}

/// A supercell's copies kept from an earlier frame draw as a fresh drawing
/// does, after an edit elsewhere and after one to the crystal.
#[test]
fn kept_supercell_copies_match_a_fresh_drawing() {
    let (mut doc, _) = open("cif", NACL);
    doc.models[0].repeat = [2, 2, 1];
    let fresh = |doc: &Document| {
        let doc = doc.clone();
        std::thread::spawn(move || crate::scene::svg(&doc))
            .join()
            .unwrap()
    };
    let _ = crate::scene::svg(&doc);
    doc.add_atom("C", Point::new(900., 900.));
    assert_eq!(crate::scene::svg(&doc), fresh(&doc));
    if let Some(atom) = doc.atoms.first_mut() {
        atom.position = atom.position.offset(3., 2.);
    }
    assert_eq!(crate::scene::svg(&doc), fresh(&doc));
}

#[test]
fn looking_along_an_axis_keeps_the_drawing_on_its_box() {
    use crate::models3d::{Along, look_along};
    let (mut doc, _) = open("cif", CO2);
    doc.models[0].repeat = [2, 2, 1];
    let before = doc.atoms.clone();
    assert!(look_along(&mut doc, 0, Along::C));
    assert_ne!(doc.atoms, before);
    let m = model(&doc);
    // c points nearly at the viewer: its edge is short beside a's and b's.
    let (origin, ends) = cell_in(&crate::scene::primitives(&doc), false);
    let mut lengths: Vec<_> = ends.iter().map(|e| e.distance(origin)).collect();
    lengths.sort_by(f32::total_cmp);
    assert!(lengths[0] < 0.3 * lengths[1], "{lengths:?}");
    // The atoms still sit where their fractions put them on the box.
    let at_origin = doc
        .atoms
        .iter()
        .find(|a| fractional(m, a.id).iter().all(|f| f.abs() < 1e-3));
    if let Some(atom) = at_origin {
        assert!(atom.position.distance(origin) < 0.1);
    }
    // An axis seen end-on has no label.
    let mut doc = open("cif", NACL).0;
    let cell = doc.models[0].cell.clone().unwrap();
    doc.models[0].rotation = Along::C.rotation(&cell);
    let turned = doc.models[0].clone();
    crate::models3d::lay_out_flat(&mut doc, &turned);
    let labels: Vec<_> = crate::scene::primitives(&doc)
        .into_iter()
        .filter_map(|p| match p {
            crate::scene::Primitive::Text { text, style, .. } if style.italic => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(labels, ["a", "b"]);
}

#[test]
fn moving_a_crystal_needs_no_chemistry_for_its_labels() {
    // Rock salt's Cl has six bonds, which valence rules would refuse.
    let (mut doc, _) = open("cif", NACL);
    for atom in &mut doc.atoms {
        atom.position = atom.position.offset(10., 0.);
    }
    let refresh =
        crate::atom_labels::refresh::Refresh::calculate(&doc, &Default::default()).unwrap();
    assert_eq!(refresh.notice, None);
}

#[test]
fn rock_salt_fills_its_cell_as_crystallographers_draw_it() {
    let (mut doc, _) = open("cif", NACL);
    let m = model(&doc).clone();
    let sites = crate::models3d::shown_sites(&doc, &m);
    // 27 ions: 8 in the cell and 19 more on its far faces, edges and corners.
    assert_eq!(sites.len(), 27);
    let at = |(id, c): &(u64, [i32; 3])| {
        let f = fractional(&m, *id);
        [0, 1, 2].map(|k| f[k] + f64::from(c[k]))
    };
    for corner in 0..8_usize {
        let want = [corner & 1, (corner >> 1) & 1, (corner >> 2) & 1].map(|v| v as f64);
        assert!(
            sites
                .iter()
                .any(|s| (0..3).all(|k| (at(s)[k] - want[k]).abs() < 1e-3)),
            "no atom on corner {want:?}"
        );
    }
    // The drawing still holds each atom once.
    assert_eq!(doc.atoms.len(), 8);
    // Two cells along each axis: ions at every half step, 5 × 5 × 5.
    doc.models[0].repeat = [2, 2, 2];
    assert_eq!(crate::models3d::shown_sites(&doc, model(&doc)).len(), 125);
    // A molecular crystal copies whole molecules: CO₂ on the corners.
    let (doc, _) = open("cif", CO2);
    let sites = crate::models3d::shown_sites(&doc, model(&doc));
    assert_eq!(sites.len() % 3, 0, "{}", sites.len());
    assert!(sites.len() > doc.atoms.len());
}

#[test]
fn beside_its_3d_model_the_2d_drawing_keeps_its_cell() {
    use crate::models3d::{Display, set_display};
    let (mut doc, _) = open("cif", NACL);
    let axes = |doc: &Document| {
        crate::scene::primitives(doc)
            .into_iter()
            .filter(|p| matches!(p, crate::scene::Primitive::Text { text, style, .. } if style.italic && text == "a"))
            .count()
    };
    assert_eq!(axes(&doc), 1);
    set_display(&mut doc, 0, Display::Both);
    // One a for the 2D drawing's cell, one for the 3D model's.
    assert_eq!(axes(&doc), 2);
}

/// The drawn cell's three edges from the origin, as vectors on screen.
fn edge_vectors(doc: &Document) -> Vec<Point> {
    let (origin, ends) = cell_in(&crate::scene::primitives(doc), false);
    let mut v: Vec<_> = ends
        .iter()
        .map(|e| Point::new(e.x - origin.x, e.y - origin.y))
        .collect();
    v.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    v
}

#[test]
fn the_cell_turns_and_mirrors_with_a_flat_or_straight_drawing() {
    let close = |a: &[Point], b: &[Point]| {
        a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p.distance(*q) < 0.5)
    };
    // Silicon's two atoms lie on a line: turning the drawing turns the box.
    let (mut doc, _) = open("xyz", SILICON);
    let before = edge_vectors(&doc);
    for atom in &mut doc.atoms {
        atom.position = Point::new(-atom.position.y, atom.position.x);
    }
    let mut turned: Vec<_> = before.iter().map(|v| Point::new(-v.y, v.x)).collect();
    turned.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    assert!(
        close(&edge_vectors(&doc), &turned),
        "{:?} {turned:?}",
        edge_vectors(&doc)
    );
    // Urea's atoms lie in a plane: mirroring the drawing mirrors the box.
    let (mut doc, _) = open("pdb", UREA);
    let before = edge_vectors(&doc);
    for atom in &mut doc.atoms {
        atom.position.x = -atom.position.x;
    }
    let mut mirrored: Vec<_> = before.iter().map(|v| Point::new(-v.x, v.y)).collect();
    mirrored.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    assert!(
        close(&edge_vectors(&doc), &mirrored),
        "{:?} {mirrored:?}",
        edge_vectors(&doc)
    );
}

#[test]
fn a_crystal_split_across_selections_is_measured_in_parts() {
    let (doc, _) = open("cif", CO2);
    let all: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
    // One molecule against the rest, as aligning molecules splits it.
    let first = doc.bonds[0].a;
    let molecule: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| b.a == first || b.b == first)
        .flat_map(|b| [b.a, b.b])
        .collect();
    let rest: Vec<_> = all
        .iter()
        .copied()
        .filter(|id| !molecule.contains(id))
        .collect();
    let whole = crate::scene::selections_bounds(&doc, std::slice::from_ref(&all));
    let split = crate::scene::selections_bounds(&doc, &[molecule, rest]);
    let width = |b: &Option<(Point, Point)>| b.map_or(0., |(lo, hi)| hi.x - lo.x);
    assert!(
        width(&split[0]) < 0.6 * width(&whole[0]),
        "{split:?} {whole:?}"
    );
}

#[test]
fn bond_orders_do_not_depend_on_atom_order() {
    // Azulene with every hydrogen, its two shared ring carbons first: the
    // outer ten-carbon ring alternates, the shared bond stays single.
    let atom = |element: &str| Atom {
        element: element.into(),
        charge: 0,
        label: String::new(),
        xyz: [0.; 3],
        adp: None,
    };
    let mut atoms: Vec<_> = (0..10).map(|_| atom("C")).collect();
    let mut bonds = vec![(0, 1)];
    // Seven-ring 0-2-3-4-5-6-1, five-ring 0-7-8-9-1.
    for pair in [
        (0, 2),
        (2, 3),
        (3, 4),
        (4, 5),
        (5, 6),
        (6, 1),
        (0, 7),
        (7, 8),
        (8, 9),
        (9, 1),
    ] {
        bonds.push(pair);
    }
    for c in 2..10 {
        atoms.push(atom("H"));
        bonds.push((c, atoms.len() - 1));
    }
    let orders = build::orders(&atoms, &bonds, &vec![false; atoms.len()]);
    assert_eq!(orders[0], 1, "the shared bond");
    for c in 0..10 {
        let doubles = bonds
            .iter()
            .zip(&orders)
            .filter(|((a, b), o)| (*a == c || *b == c) && **o == 2)
            .count();
        assert_eq!(doubles, 1, "carbon {c}: {orders:?}");
    }
}

#[test]
fn a_plain_xyz_molecule_keeps_its_linear_carbon_label() {
    let (doc, _) = open("xyz", "3\nCO2\nC 0 0 0\nO 1.16 0 0\nO -1.16 0 0\n");
    let carbon = doc.atoms.iter().find(|a| a.element == "C").unwrap();
    assert_eq!(
        carbon.display.carbons,
        Some(crate::atom_labels::Carbons::All)
    );
    assert!(crate::atom_labels::visible(carbon, &doc));
}

#[test]
fn far_face_copies_draw_their_stubs_too() {
    use crate::scene::Primitive;
    let (doc, _) = open("cif", NACL);
    let line = doc.drawing_style.world(doc.drawing_style.line_width_pt);
    // Stubs drawn from the lighter copies on the far faces.
    let ghost_stubs = crate::scene::primitives(&doc)
        .into_iter()
        .filter(|p| match p {
            Primitive::Opacity { primitive, .. } => {
                matches!(**primitive, Primitive::Wire { dash, .. } if dash > 0. && dash < line * 3.)
            }
            _ => false,
        })
        .count();
    assert!(ghost_stubs > 0);
}

#[test]
fn both_places_the_3d_frame_past_a_whole_repeat() {
    use crate::models3d::{Display, frame, set_display};
    let (mut doc, _) = open("cif", NACL);
    doc.models[0].repeat = [3, 1, 1];
    let drawing = crate::scene::bounds(&crate::scene::primitives(&doc));
    set_display(&mut doc, 0, Display::Both);
    let (lo, _) = frame(&doc, &doc.models[0]).unwrap();
    assert!(lo.x > drawing.1.x, "{lo:?} {drawing:?}");
}

#[test]
fn copies_on_the_box_sit_on_a_halo_over_its_edges() {
    use crate::scene::Primitive;
    let lighter_text = |p: &Primitive| {
        matches!(p, Primitive::Opacity { alpha, primitive }
            if (*alpha - 0.55).abs() < 1e-3 && matches!(**primitive, Primitive::Text { .. }))
    };
    for (format, text) in [
        ("cif", NACL),
        ("xyz", SILICON),
        ("poscar", RUTILE),
        ("cif", CO2),
    ] {
        let (doc, _) = open(format, text);
        let drawing = crate::scene::primitives(&doc);
        let last_edge = drawing
            .iter()
            .rposition(|p| matches!(p, Primitive::Wire { dash, .. } if *dash == 0.))
            .unwrap();
        // The copies' labels: lighter text after the box's front edges, on a
        // halo that knocks the edges out beneath them.
        let halo = drawing
            .iter()
            .position(|p| {
                matches!(p, Primitive::Halo { primitives, .. }
                    if primitives.iter().any(|q| matches!(q, Primitive::Text { .. })))
            })
            .unwrap();
        assert!(halo > last_edge, "{format}: halo before the edges");
        let copies = drawing[halo..].iter().filter(|p| lighter_text(p)).count();
        assert!(copies > doc.atoms.len(), "{format}: {copies}");
        assert!(
            !drawing[..last_edge].iter().any(lighter_text),
            "{format}: a copy's label under the edges"
        );
    }
}

#[test]
fn large_repeats_still_keep_atom_labels_apart() {
    // 4 × 4 × 4 rock salt: 512 labels. Looking straight down c would stack
    // every Na on a Cl, so the view chosen along c must tilt off it.
    let (mut doc, _) = open("cif", NACL);
    doc.models[0].repeat = [4, 4, 4];
    let cell = doc.models[0].cell.clone().unwrap();
    assert!(crate::models3d::look_along(
        &mut doc,
        0,
        crate::models3d::Along::C
    ));
    assert_ne!(
        doc.models[0].rotation,
        crate::models3d::Along::C.rotation(&cell)
    );
}

#[test]
fn crystal_substitutions_keep_the_cell_copies_and_formula_after_save() {
    use crate::{atom_text, document::History, transaction};
    for (source, label, expected) in [("Na", "K+", "Cl4KNa3"), ("Cl", "Br", "BrCl3Na4+")] {
        let (mut doc, _) = open("cif", NACL);
        doc.models[0].repeat = [2, 1, 1];
        let before = doc.clone();
        // Prime the renderer's kept-copy cache before changing the element.
        let _ = crate::scene::svg(&before);
        let id = doc.atoms.iter().find(|a| a.element == source).unwrap().id;
        doc = atom_text::apply(&doc, id, label, atom_text::Mode::Auto).unwrap();
        let mut history = History::default();
        transaction::apply(&mut doc, &mut history, before.clone(), false).unwrap();
        assert_eq!(doc.models, before.models, "{source} → {label}");
        assert!(doc.atom(id).unwrap().no_implicit);
        let symbol = if source == "Na" { "K" } else { "Br" };
        let svg = crate::scene::svg(&doc);
        assert!(
            svg.matches(&format!(">{symbol}</text>")).count() > 1,
            "copies must show {symbol}"
        );
        let fresh = doc.clone();
        let fresh_svg = std::thread::spawn(move || crate::scene::svg(&fresh))
            .join()
            .unwrap();
        assert_eq!(
            svg, fresh_svg,
            "the kept-copy cache must follow the element edit"
        );
        assert_eq!(
            crate::models3d::crystal::composition(&doc, &[])
                .unwrap()
                .formula,
            expected
        );
        let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
        assert_eq!(reopened.models, doc.models);
        assert_eq!(reopened.atoms, doc.atoms);
        assert!(history.undo(&mut doc));
        assert_eq!(doc, before);
        assert!(!history.undo(&mut doc));
    }
}

#[test]
fn typed_crystal_charge_changes_keep_explicit_sites_and_the_cell() {
    use crate::{atom_text, transaction};
    for (label, charge) in [("Na+2", 2), ("Na", 0)] {
        let (before, _) = open("cif", NACL);
        let id = before.atoms.iter().find(|a| a.element == "Na").unwrap().id;
        let mut doc = atom_text::apply(&before, id, label, atom_text::Mode::Auto).unwrap();
        let _ = transaction::reconcile(&mut doc, before.clone()).unwrap();
        assert_eq!(doc.models, before.models);
        assert_eq!(doc.atom(id).unwrap().charge, charge);
        assert!(doc.atom(id).unwrap().no_implicit);
    }
}
