use super::*;

fn cartesian(line: &str, charge_column: bool) {
    let columns: Vec<_> = line.split_whitespace().collect();
    let start = if charge_column { 2 } else { 1 };
    assert_eq!(columns.len(), start + 3, "Cartesian atom: {line}");
    assert!(columns[0].bytes().all(|byte| byte.is_ascii_alphabetic()));
    assert!((1..=2).contains(&columns[0].len()));
    if charge_column {
        assert!(columns[1].parse::<f64>().unwrap() > 0.0);
    }
    for coordinate in &columns[start..] {
        assert!(coordinate.parse::<f64>().unwrap().is_finite(), "{line}");
    }
}

fn charge_and_multiplicity(line: &str) {
    let columns: Vec<_> = line.split_whitespace().collect();
    assert_eq!(columns.len(), 2, "charge/multiplicity: {line}");
    columns[0].parse::<i32>().unwrap();
    assert!(columns[1].parse::<u32>().unwrap() > 0);
}

fn gaussian(text: &str, atoms: usize, sections: usize) {
    let lines: Vec<_> = text.lines().collect();
    let mut index = 0;
    while lines[index].trim_start().starts_with('!') {
        index += 1;
    }
    while lines[index].starts_with('%') {
        let (name, value) = lines[index].split_once('=').unwrap();
        assert!(name.len() > 1 && !value.trim().is_empty());
        index += 1;
    }
    assert!(lines[index].starts_with('#'));
    while !lines[index].trim().is_empty() {
        index += 1;
    }
    index += 1;
    for _ in 0..sections {
        assert!(!lines[index].trim().is_empty(), "Gaussian title");
        while !lines[index].trim().is_empty() {
            index += 1;
        }
        index += 1;
        charge_and_multiplicity(lines[index]);
        index += 1;
        let start = index;
        while index < lines.len() && !lines[index].trim().is_empty() {
            cartesian(lines[index], false);
            index += 1;
        }
        assert_eq!(index - start, atoms);
        assert!(
            index < lines.len(),
            "Gaussian geometry needs a terminating blank line"
        );
        index += 1;
    }
    assert!(lines[index..].iter().all(|line| line.trim().is_empty()));
}

fn orca(text: &str, atoms: usize) {
    let lines: Vec<_> = text.lines().collect();
    assert!(lines.iter().any(|line| line.trim_start().starts_with('!')));
    let index = lines
        .iter()
        .position(|line| {
            let columns: Vec<_> = line.split_whitespace().collect();
            columns.len() >= 2 && columns[0] == "*" && columns[1].eq_ignore_ascii_case("xyz")
        })
        .unwrap();
    let header: Vec<_> = lines[index].split_whitespace().collect();
    assert_eq!(header.len(), 4);
    charge_and_multiplicity(&header[2..].join(" "));
    let start = index + 1;
    let end = (start..lines.len())
        .find(|&index| lines[index].trim() == "*")
        .unwrap();
    assert_eq!(end - start, atoms);
    for line in &lines[start..end] {
        cartesian(line, false);
    }
}

fn gamess(text: &str, atoms: usize) {
    let upper = text.to_ascii_uppercase();
    let control = upper.split_once("$CONTRL").unwrap().1;
    let control = control.split_once("$END").unwrap().0;
    assert!(control.contains("SCFTYP="));
    assert!(control.contains("RUNTYP="));
    let lines: Vec<_> = text.lines().collect();
    let data = lines
        .iter()
        .position(|line| line.trim().eq_ignore_ascii_case("$DATA"))
        .unwrap();
    assert!(!lines[data + 1].trim().is_empty(), "GAMESS title");
    assert!(!lines[data + 2].trim().is_empty(), "GAMESS symmetry");
    let end = (data + 3..lines.len())
        .find(|&index| lines[index].trim().eq_ignore_ascii_case("$END"))
        .unwrap();
    let atom_lines: Vec<_> = lines[data + 3..end]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(atom_lines.len(), atoms);
    for line in atom_lines {
        cartesian(line, true);
    }
}

pub(super) fn check(program: Program, text: &str, atoms: usize, sections: usize) {
    match program {
        Program::Gaussian => gaussian(text, atoms, sections),
        Program::Orca => orca(text, atoms),
        Program::Gamess => {
            gamess(text, atoms);
            for line in text.lines() {
                assert!(line.len() <= 80, "GAMESS line is too long: {line}");
                if line.trim_start().starts_with('$') {
                    assert!(line.starts_with(" $"), "GAMESS group: {line}");
                }
            }
        }
        Program::Xyz => {
            let lines: Vec<_> = text.lines().collect();
            assert_eq!(lines[0].parse::<usize>().unwrap(), atoms);
            assert_eq!(lines.len(), atoms + 2);
            for line in &lines[2..] {
                cartesian(line, false);
            }
        }
    }
}

#[test]
fn cartesian_checkers_accept_real_program_inputs() {
    for name in [
        "dvb_ir.inp",
        "dvb_gopt.inp",
        "dvb_sp_un_dft.inp",
        "dvb_dispersion_bp86_d3zero.inp",
    ] {
        let text = fixture(&format!("third_party/cclib/ORCA6.0/{name}"));
        orca(&text, 20);
    }
    gaussian(
        &fixture("third_party/cclib/Gaussian16/dvb_dispersion_bp86_d3zero.in"),
        20,
        1,
    );
    gaussian(&fixture("third_party/pymatgen/C60_gau.com"), 60, 1);
    for (name, atoms) in [
        ("dvb_un_sp.inp", 20),
        ("nh3_ts_ir.inp", 4),
        ("dvb_dispersion_bp86_d3zero.inp", 20),
    ] {
        gamess(
            &fixture(&format!("third_party/cclib/GAMESS-US2018/{name}")),
            atoms,
        );
    }
    // Gaussian's unrestricted sample uses a Z-matrix, so only its keyword is checked below.
}

#[test]
fn dispersion_and_open_shell_keywords_match_upstream_samples() {
    let frame = water();
    for (program, method, multiplicity, keyword, sample) in [
        (
            Program::Gaussian,
            "BP86-D3",
            1,
            "empiricaldispersion=gd3",
            "cclib/Gaussian16/dvb_dispersion_bp86_d3zero.in",
        ),
        (
            Program::Orca,
            "BP86-D3",
            1,
            "d3zero",
            "cclib/ORCA6.0/dvb_dispersion_bp86_d3zero.inp",
        ),
        (
            Program::Gamess,
            "BP86-D3",
            1,
            "dc=.t. idcver=3",
            "cclib/GAMESS-US2018/dvb_dispersion_bp86_d3zero.inp",
        ),
        (
            Program::Gaussian,
            "B3LYP",
            2,
            "ub3lyp",
            "cclib/Gaussian16/dvb_un_sp.gjf",
        ),
        (
            Program::Orca,
            "B3LYP",
            2,
            "uks",
            "cclib/ORCA6.0/dvb_sp_un_dft.inp",
        ),
        (
            Program::Gamess,
            "B3LYP",
            2,
            "scftyp=uhf",
            "cclib/GAMESS-US2018/dvb_un_sp.inp",
        ),
    ] {
        let mut settings = settings(program, Job::SinglePoint);
        settings.method = method.into();
        settings.multiplicity = multiplicity;
        let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
        assert!(output.main.text.to_ascii_lowercase().contains(keyword));
        assert!(
            fixture(&format!("third_party/{sample}"))
                .to_ascii_lowercase()
                .contains(keyword)
        );
    }
}
