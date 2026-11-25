//! Geometry sections of Gaussian, ORCA and GAMESS input files.
use super::{Error, Format, Job, JobKind, QcFile, Step, element, frame};
use reshiki_qcdata::{ANGSTROM_PER_BOHR, Frame};
use std::collections::HashMap;

type Line<'a> = (usize, &'a str);
type Position = [f64; 3];
type Atom = (u8, u16);

const CHECKPOINT: &str =
    "The geometry is read from the checkpoint file (Geom=Check); open the log or fchk instead";
const PARAMETERS: &str = "Parameterized coordinates (%paras) are not read";
const GHOST: &str = "Ghost atoms are not supported; save a geometry containing only real atoms";

fn lines(text: &str) -> impl Iterator<Item = Line<'_>> {
    text.lines().enumerate().map(|(i, row)| (i + 1, row.trim()))
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
}

fn real(text: &str) -> Option<f64> {
    text.parse::<f64>()
        .or_else(|e| {
            if text.contains(['D', 'd']) {
                text.replace(['D', 'd'], "E").parse()
            } else {
                Err(e)
            }
        })
        .ok()
        .filter(|v| v.is_finite())
}

fn cartesian(fields: [&str; 3], line: usize) -> Result<Position, Error> {
    let [Some(x), Some(y), Some(z)] = fields.map(real) else {
        return Err(Error::at(line, "Expected numeric Cartesian coordinates"));
    };
    Ok([x, y, z])
}

fn spin(charge: &str, mult: &str, line: usize) -> Result<(i32, u32), Error> {
    Ok((
        charge
            .parse()
            .map_err(|_| Error::at(line, "Invalid molecular charge"))?,
        mult.parse()
            .map_err(|_| Error::at(line, "Invalid spin multiplicity"))?,
    ))
}

fn geometry(format: Format, job: Job) -> QcFile {
    QcFile {
        format,
        program: None,
        program_version: None,
        jobs: vec![job],
        warnings: Vec::new(),
    }
}

fn geometry_job(frame: Frame, label: &str) -> Job {
    Job {
        index: 0,
        kind: JobKind::Geometry,
        method: None,
        basis: None,
        keywords: Vec::new(),
        completion: None,
        steps: vec![Step {
            frame,
            energy_hartree: None,
            label: (!label.is_empty()).then(|| label.into()),
        }],
        result: None,
    }
}

fn section<'a>(lines: &mut &'a [Line<'a>]) -> &'a [Line<'a>] {
    let from = lines
        .iter()
        .position(|(_, row)| !row.is_empty())
        .unwrap_or(lines.len());
    let (_, remaining) = lines.split_at(from);
    let end = remaining
        .iter()
        .position(|(_, row)| row.is_empty())
        .unwrap_or(remaining.len());
    let (found, tail) = remaining.split_at(end);
    *lines = tail;
    found
}

fn assignment<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.match_indices(key)
        .filter_map(|(at, _)| {
            let (before, rest) = text.split_at(at);
            if before.as_bytes().last().is_some_and(|previous| {
                !previous.is_ascii_whitespace() && !matches!(previous, b',' | b'#')
            }) {
                return None;
            }
            let tail = rest.strip_prefix(key)?.trim_start();
            let value = if let Some(value) = tail.strip_prefix('=') {
                value.trim_start()
            } else if tail.starts_with('(') {
                tail
            } else {
                return None;
            };
            if let Some(group) = value.strip_prefix('(') {
                group.split_once(')').map(|(options, _)| options)
            } else {
                words(value).next()
            }
        })
        .last()
}

/// Read the first Gaussian Link1 job with an inline geometry.
pub(super) fn gaussian(text: &str) -> Result<QcFile, Error> {
    let all: Vec<_> = lines(text).collect();
    let separator = |line: &Line<'_>| line.1.eq_ignore_ascii_case("--Link1--");
    let mut jobs = all.split(separator).peekable();
    let mut linked = false;
    let mut checkpoint = false;
    while let Some(lines) = jobs.next() {
        match gaussian_job(lines) {
            Ok(Some(mut file)) => {
                if linked || jobs.peek().is_some() {
                    file.warnings
                        .push("Read the geometry of the first job".into());
                }
                return Ok(file);
            }
            Ok(None) => (),
            Err(Error::Unsupported(message)) if message == CHECKPOINT => checkpoint = true,
            Err(error) => return Err(error),
        }
        linked = true;
    }
    if checkpoint {
        Err(Error::Unsupported(CHECKPOINT.into()))
    } else {
        Err(Error::Invalid("The Gaussian input has no geometry".into()))
    }
}

fn gaussian_job<'a>(mut lines: &'a [Line<'a>]) -> Result<Option<QcFile>, Error> {
    let Some(start) = lines.iter().position(|(_, row)| row.starts_with('#')) else {
        return Ok(None);
    };
    lines = lines.split_at(start).1;
    let route = section(&mut lines)
        .iter()
        .map(|(_, row)| *row)
        .collect::<Vec<_>>()
        .join(" ");
    let lower = route.to_ascii_lowercase();
    if assignment(&lower, "geom")
        .is_some_and(|value| words(value).any(|s| matches!(s, "check" | "allcheck" | "checkpoint")))
    {
        return Err(Error::Unsupported(CHECKPOINT.into()));
    }
    let scale = match assignment(&lower, "units") {
        Some("bohr" | "au") => ANGSTROM_PER_BOHR,
        None | Some("angstrom" | "angstroms" | "angs") => 1.,
        Some(units) => {
            return Err(Error::Unsupported(format!(
                "Gaussian units {units} are not supported; use ångström or bohr"
            )));
        }
    };

    let title = section(&mut lines)
        .iter()
        .map(|(_, row)| *row)
        .collect::<Vec<_>>()
        .join(" ");
    let Some(((line, state), rows)) = section(&mut lines).split_first() else {
        return Ok(None);
    };
    let mut fields = words(state);
    let (Some(charge), Some(mult)) = (fields.next(), fields.next()) else {
        return Err(Error::at(
            *line,
            "Expected Gaussian charge and spin multiplicity",
        ));
    };
    let (charge, mult) = spin(charge, mult, *line)?;
    let extra_pairs = fields.next().is_some();
    let end = rows
        .iter()
        .position(|(_, row)| variable_header(row))
        .unwrap_or(rows.len());
    if end == 0 {
        return Ok(None);
    }
    let (rows, variable_lines) = rows.split_at(end);
    let variables = gaussian_variables(variable_lines.iter().chain(lines));
    let frame = coordinates(
        rows,
        CoordinateKind::ZMatrix,
        &variables,
        scale,
        charge,
        mult,
    )?;
    let mut job = geometry_job(frame, &title);
    job.keywords = route
        .split_whitespace()
        .filter_map(|word| {
            let word = word.trim_start_matches('#');
            (!word.is_empty()).then(|| word.to_owned())
        })
        .collect();
    if let Some((method, basis)) = job.keywords.iter().find_map(|word| {
        let mut depth = 0;
        let slash = word.char_indices().find_map(|(i, c)| {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                '/' if depth == 0 => return Some(i),
                _ => (),
            }
            None
        })?;
        let (method, suffix) = word.split_at(slash);
        let basis = suffix.strip_prefix('/')?;
        (!method.is_empty() && !method.contains('=') && !basis.is_empty())
            .then_some((method, basis))
    }) {
        job.method = Some(method.to_owned());
        job.basis = Some(basis.to_owned());
    }
    let mut file = geometry(Format::GaussianInput, job);
    if extra_pairs {
        file.warnings
            .push("Read the first Gaussian charge and multiplicity pair".into());
    }
    Ok(Some(file))
}

fn variable_header(row: &str) -> bool {
    row.eq_ignore_ascii_case("variables:") || row.eq_ignore_ascii_case("constants:")
}

fn gaussian_variables<'a>(lines: impl Iterator<Item = &'a Line<'a>>) -> HashMap<String, f64> {
    let mut variables = HashMap::new();
    for &(_, row) in lines {
        if row.is_empty() || variable_header(row) {
            continue;
        }
        let mut fields = words(row).flat_map(|word| word.split('=').filter(|s| !s.is_empty()));
        let (Some(name), Some(value), None) = (fields.next(), fields.next(), fields.next()) else {
            break;
        };
        if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
            break;
        }
        let Some(value) = real(value) else { break };
        variables.insert(name.to_ascii_uppercase(), value);
    }
    variables
}

#[derive(Clone, Copy)]
enum CoordinateKind {
    Cartesian,
    ZMatrix,
    OrcaInternal,
}

fn orca_kind(kind: &str) -> Result<CoordinateKind, Error> {
    match kind.to_ascii_lowercase().as_str() {
        "xyz" => Ok(CoordinateKind::Cartesian),
        "int" | "internal" => Ok(CoordinateKind::OrcaInternal),
        "gzmt" => Ok(CoordinateKind::ZMatrix),
        _ => Err(Error::Unsupported(format!(
            "ORCA coordinate type {kind} is not supported; save the geometry as XYZ"
        ))),
    }
}

fn orca_scale(units: &str) -> Result<f64, Error> {
    match units.to_ascii_lowercase().as_str() {
        "bohrs" | "bohr" => Ok(ANGSTROM_PER_BOHR),
        "angs" | "angstrom" | "angstroms" => Ok(1.),
        _ => Err(Error::Unsupported(format!(
            "ORCA units {units} are not supported; use Angs or Bohrs"
        ))),
    }
}

/// Read an ORCA geometry defined inline with stars or in `%coords`.
pub(super) fn orca(text: &str) -> Result<QcFile, Error> {
    let mut all = Vec::new();
    let mut keywords = Vec::new();
    let mut basis = false;
    let mut linked = false;
    for (line, row) in lines(text) {
        let row = row.split('#').next().unwrap_or_default().trim();
        if row.eq_ignore_ascii_case("$new_job") {
            linked = true;
            break;
        }
        if let Some(simple) = row.strip_prefix('!') {
            keywords.extend(simple.split_whitespace().map(str::to_owned));
            continue;
        }
        let mut tokens = row.split_whitespace();
        if tokens
            .next()
            .is_some_and(|first| first.eq_ignore_ascii_case("%paras"))
        {
            return Err(Error::Unsupported(PARAMETERS.into()));
        }
        if basis {
            basis = !row
                .split_whitespace()
                .any(|w| w.eq_ignore_ascii_case("end"));
            continue;
        }
        // A per-atom basis definition can begin after an atom or on its own line.
        let lower = row.to_ascii_lowercase();
        let new_basis = ["newgto", "addgto"].into_iter().find_map(|key| {
            lower.match_indices(key).find_map(|(at, _)| {
                let (prefix, suffix) = lower.split_at(at);
                let before = prefix.as_bytes().last().is_none_or(u8::is_ascii_whitespace);
                let after = suffix
                    .strip_prefix(key)
                    .is_some_and(|tail| tail.is_empty() || tail.starts_with(char::is_whitespace));
                (before && after).then_some(at)
            })
        });
        let row = if let Some(at) = new_basis {
            let (before, after) = row.split_at(at);
            basis = !after
                .split_whitespace()
                .any(|w| w.eq_ignore_ascii_case("end"));
            before.trim_end()
        } else {
            row
        };
        if !row.is_empty() {
            all.push((line, row));
        }
    }
    if basis {
        return Err(Error::Invalid(
            "ORCA atom basis block is missing end".into(),
        ));
    }

    let mut scale = 1.;
    for keyword in &keywords {
        if keyword.eq_ignore_ascii_case("bohrs") || keyword.eq_ignore_ascii_case("angs") {
            scale = orca_scale(keyword)?;
        }
    }
    let mut block = None;
    let mut rest = all.as_slice();
    while let Some((&(line, row), tail)) = rest.split_first() {
        rest = tail;
        if let Some(star) = row.strip_prefix('*') {
            let mut fields = words(star);
            let (Some(kind), Some(charge), Some(mult)) =
                (fields.next(), fields.next(), fields.next())
            else {
                return Err(Error::at(
                    line,
                    "Expected ORCA coordinate type, charge and multiplicity",
                ));
            };
            if kind.eq_ignore_ascii_case("xyzfile") {
                let name = fields.collect::<Vec<_>>().join(" ");
                if name.is_empty() {
                    return Err(Error::at(line, "Expected an XYZ filename"));
                }
                return Err(Error::Unsupported(format!(
                    "The geometry is in {}; open that file",
                    name.trim_matches('"')
                )));
            }
            let kind = orca_kind(kind)?;
            let (charge, mult) = spin(charge, mult, line)?;
            let rows = take_until(&mut rest, "*", line)?;
            block.get_or_insert((kind, charge, mult, rows));
        } else if let Some(setting) = row
            .split_whitespace()
            .next()
            .filter(|s| s.eq_ignore_ascii_case("%coords"))
        {
            let mut kind = CoordinateKind::Cartesian;
            let (mut charge, mut mult) = (0, 1);
            let mut coords = None;
            let mut control = row.split_at(setting.len()).1.trim();
            let mut control_line = line;
            loop {
                let mut fields = control
                    .split(|c: char| c.is_ascii_whitespace() || c == '=')
                    .filter(|s| !s.is_empty());
                let mut ended = false;
                while let Some(key) = fields.next() {
                    if key.eq_ignore_ascii_case("end") {
                        ended = true;
                        break;
                    }
                    if key.eq_ignore_ascii_case("coords") {
                        coords = Some(take_until(&mut rest, "end", control_line)?);
                        break;
                    }
                    if let key @ ("ctyp" | "charge" | "mult" | "units") =
                        key.to_ascii_lowercase().as_str()
                    {
                        let value = fields.next().ok_or_else(|| {
                            Error::at(control_line, format!("Expected a value for ORCA {key}"))
                        })?;
                        match key {
                            "ctyp" => kind = orca_kind(value)?,
                            "units" => scale = orca_scale(value)?,
                            "charge" => charge = spin(value, "1", control_line)?.0,
                            _ => mult = spin("0", value, control_line)?.1,
                        }
                    }
                }
                if ended {
                    break;
                }
                let Some((&(line, row), tail)) = rest.split_first() else {
                    return Err(Error::at(control_line, "ORCA %coords block is missing end"));
                };
                (control_line, control, rest) = (line, row, tail);
            }
            if let Some(rows) = coords {
                block.get_or_insert((kind, charge, mult, rows));
            }
        }
    }
    let (kind, charge, mult, rows) =
        block.ok_or_else(|| Error::Invalid("The ORCA input has no geometry".into()))?;
    let mut mass_override = false;
    let mut clean = Vec::new();
    for &(line, row) in rows {
        if row.contains(['{', '}']) {
            return Err(Error::Unsupported(PARAMETERS.into()));
        }
        let mass = row.match_indices(['M', 'm']).find(|&(at, _)| {
            let (before, _) = row.split_at(at);
            before
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_whitespace)
                && row.split_at(at + 1).1.trim_start().starts_with('=')
        });
        let row = if let Some((at, _)) = mass {
            mass_override = true;
            row.split_at(at).0.trim_end()
        } else {
            row
        };
        clean.push((line, row));
    }
    let frame = coordinates(&clean, kind, &HashMap::new(), scale, charge, mult)?;
    let mut job = geometry_job(frame, "");
    job.keywords = keywords;
    let mut file = geometry(Format::OrcaInput, job);
    if linked {
        file.warnings
            .push("Read the geometry of the first job".into());
    }
    if mass_override {
        file.warnings
            .push("ORCA atom mass overrides (M =) were ignored".into());
    }
    Ok(file)
}

fn take_until<'a>(
    lines: &mut &'a [Line<'a>],
    end: &str,
    line: usize,
) -> Result<&'a [Line<'a>], Error> {
    let at = lines
        .iter()
        .position(|(_, row)| row.eq_ignore_ascii_case(end))
        .ok_or_else(|| {
            Error::at(
                line,
                format!("ORCA coordinates are missing their closing {end}"),
            )
        })?;
    let (rows, tail) = lines.split_at(at);
    *lines = tail.split_at(1).1;
    Ok(rows)
}

fn atom_label(label: &str, line: usize) -> Result<Atom, Error> {
    let name = label
        .split(['(', '-'])
        .next()
        .unwrap_or_default()
        .trim_end_matches(|c: char| c.is_ascii_digit());
    if name.eq_ignore_ascii_case("bq")
        || label.to_ascii_lowercase().contains("-bq")
        || label.contains(':')
    {
        return Err(Error::Unsupported(GHOST.into()));
    }
    if name.eq_ignore_ascii_case("x") || name.eq_ignore_ascii_case("xx") {
        return Ok((0, 0));
    }
    let atom = element(name).or_else(|| {
        label
            .parse::<u8>()
            .ok()
            .filter(|z| (1..=118).contains(z))
            .map(|z| (z, 0))
    });
    let (z, mut isotope) =
        atom.ok_or_else(|| Error::at(line, format!("'{label}' is not an element")))?;
    for decoration in label.split(['(', ',', ')']).skip(1) {
        if let Some((key, value)) = decoration.split_once('=')
            && key.trim().eq_ignore_ascii_case("iso")
        {
            isotope = value
                .trim()
                .parse()
                .map_err(|_| Error::at(line, format!("Invalid isotope on {label}")))?;
        }
    }
    Ok((z, isotope))
}

fn coordinates(
    rows: &[Line<'_>],
    kind: CoordinateKind,
    variables: &HashMap<String, f64>,
    scale: f64,
    charge: i32,
    mult: u32,
) -> Result<Frame, Error> {
    let mut labels = HashMap::new();
    let mut atoms = Vec::new();
    let mut positions = Vec::new();
    for &(line, row) in rows {
        // A decorated label may contain commas or spaces inside its parentheses.
        let mut depth = 0;
        let at = row
            .char_indices()
            .find_map(|(at, c)| {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    c if depth == 0 && (c.is_ascii_whitespace() || c == ',') => return Some(at),
                    _ => (),
                }
                None
            })
            .unwrap_or(row.len());
        let (label, tail) = row.split_at(at);
        let atom = atom_label(label, line)?;
        let fields: Vec<_> = words(tail).collect();
        let count = positions.len().min(3);
        let position = match (kind, fields.as_slice()) {
            (CoordinateKind::Cartesian | CoordinateKind::ZMatrix, &[x, y, z]) => {
                cartesian([x, y, z], line)?
            }
            (CoordinateKind::ZMatrix, &["0" | "-1", x, y, z]) => cartesian([x, y, z], line)?,
            _ => {
                let mut references = [("", ""); 3];
                match kind {
                    CoordinateKind::ZMatrix if fields.len() == count * 2 => {
                        for (pair, &[reference, value]) in
                            references.iter_mut().zip(fields.as_chunks::<2>().0)
                        {
                            *pair = (reference, value);
                        }
                    }
                    CoordinateKind::OrcaInternal => {
                        let &[a, b, c, r, angle, dihedral] = fields.as_slice() else {
                            return Err(Error::at(
                                line,
                                "Expected three atom references and three internal coordinates",
                            ));
                        };
                        references = [(a, r), (b, angle), (c, dihedral)];
                        if references
                            .iter()
                            .skip(count)
                            .any(|&(reference, _)| reference != "0")
                        {
                            return Err(Error::at(
                                line,
                                "Unused ORCA atom references must be zero",
                            ));
                        }
                    }
                    CoordinateKind::Cartesian => {
                        return Err(Error::at(line, "Expected an element and x, y, z"));
                    }
                    _ => {
                        return Err(Error::at(
                            line,
                            format!("Expected a Z-matrix row with {count} reference atoms"),
                        ));
                    }
                }
                let mut anchors = [[0.; 3]; 3];
                let mut internal = [0.; 3];
                for (((label, value), anchor), coordinate) in references
                    .into_iter()
                    .take(count)
                    .zip(&mut anchors)
                    .zip(&mut internal)
                {
                    let index = if let Ok(n) = label.parse::<usize>() {
                        n.checked_sub(1)
                    } else {
                        labels.get(&label.to_ascii_uppercase()).copied().flatten()
                    };
                    *anchor = index
                        .and_then(|n| positions.get(n).copied())
                        .ok_or_else(|| {
                            Error::at(
                                line,
                                format!("Z-matrix reference '{label}' must name one earlier atom"),
                            )
                        })?;
                    *coordinate = real(value)
                        .or_else(|| {
                            let (name, sign) = if let Some(name) = value.strip_prefix('-') {
                                (name, -1.)
                            } else {
                                (value.trim_start_matches('+'), 1.)
                            };
                            variables.get(&name.to_ascii_uppercase()).map(|v| v * sign)
                        })
                        .ok_or_else(|| {
                            Error::at(line, format!("Unknown Z-matrix value '{value}'"))
                        })?;
                }
                place(anchors.split_at(count).0, internal, line)?
            }
        };
        labels
            .entry(
                label
                    .split(['(', '-'])
                    .next()
                    .unwrap_or_default()
                    .to_ascii_uppercase(),
            )
            .and_modify(|index| *index = None)
            .or_insert(Some(positions.len()));
        atoms.push(atom);
        positions.push(position);
    }

    let (atoms, positions): (Vec<_>, Vec<_>) = atoms
        .into_iter()
        .zip(positions)
        .filter(|(atom, _)| atom.0 != 0)
        .map(|(atom, position)| (atom, position.map(|x| x * scale)))
        .unzip();
    frame(&atoms, positions, charge, mult)
}

fn sub([ax, ay, az]: Position, [bx, by, bz]: Position) -> Position {
    [ax - bx, ay - by, az - bz]
}

fn cross([x, y, z]: Position, [a, b, c]: Position) -> Position {
    [y * c - z * b, z * a - x * c, x * b - y * a]
}

fn unit(v: Position) -> Option<Position> {
    let [x, y, z] = v;
    let norm = x.hypot(y).hypot(z);
    (norm > 0. && norm.is_finite()).then(|| v.map(|x| x / norm))
}

fn place(
    anchors: &[Position],
    [r, angle, dihedral]: Position,
    line: usize,
) -> Result<Position, Error> {
    let Some(&a) = anchors.first() else {
        return Ok([0.; 3]);
    };
    if r <= 0. {
        return Err(Error::at(line, "Z-matrix bond length must be positive"));
    }
    let Some(&b) = anchors.get(1) else {
        let [x, y, z] = a;
        return Ok([x, y, z + r]);
    };
    let axis = unit(sub(b, a))
        .ok_or_else(|| Error::at(line, "Z-matrix bond and angle references coincide"))?;
    let (sin_angle, cos_angle) = angle.to_radians().sin_cos();
    let mut position = a;
    // A linear bond does not use a reference plane.
    if sin_angle.abs() < 1e-15 {
        for (coordinate, direction) in position.iter_mut().zip(axis) {
            *coordinate += r * cos_angle * direction;
        }
        return Ok(position);
    }
    let in_plane = anchors.get(2).map_or_else(
        || {
            let [x, _, _] = axis;
            if x.abs() < 0.9 {
                [1., 0., 0.]
            } else {
                [0., 1., 0.]
            }
        },
        |&c| sub(c, b),
    );
    let normal = unit(cross(axis, in_plane)).ok_or_else(|| {
        Error::at(
            line,
            "Collinear Z-matrix dihedral references; add a dummy atom or use Cartesian coordinates",
        )
    })?;
    let side = cross(normal, axis);
    let (sin_phi, cos_phi) = dihedral.to_radians().sin_cos();
    // The perpendicular component sets the signed dihedral of new–a–b–c.
    for (((coordinate, axis), side), normal) in position.iter_mut().zip(axis).zip(side).zip(normal)
    {
        *coordinate += r * (cos_angle * axis + sin_angle * (cos_phi * side - sin_phi * normal));
    }
    Ok(position)
}

fn gamess_content(text: &str) -> (String, String) {
    let mut clean = String::with_capacity(text.len());
    let mut controls = String::new();
    let (mut namelist, mut comment, mut line_comment) = (false, false, false);
    let (mut data, mut control, mut start) = (false, false, 0);
    for (at, c) in text.char_indices() {
        if c == '\n' {
            if data && clean[start..].trim().eq_ignore_ascii_case("$END") {
                data = false;
            }
            line_comment = false;
            clean.push(c);
            start = clean.len();
            if control {
                controls.push(c);
            }
            continue;
        }
        let c = if line_comment {
            ' '
        } else if c == '!' {
            if namelist {
                comment = !comment;
            } else {
                line_comment = true;
            }
            ' '
        } else if comment {
            ' '
        } else {
            if c == '$' && !data {
                let group = text[at + 1..]
                    .split(|c: char| !c.is_ascii_alphanumeric())
                    .next()
                    .unwrap_or_default();
                if !group.is_empty() {
                    data = group.eq_ignore_ascii_case("DATA");
                    namelist = !group.eq_ignore_ascii_case("END") && !data;
                    control = group.eq_ignore_ascii_case("CONTRL");
                    if control {
                        controls.push(' ');
                    }
                }
            }
            c
        };
        clean.push(c);
        if control {
            controls.push(c);
        }
    }
    (clean, controls)
}

/// Read a C1 Cartesian or simple Z-matrix geometry in a GAMESS `$DATA` group.
pub(super) fn gamess(text: &str) -> Result<QcFile, Error> {
    // Namelist comments toggle at each bang, including across physical lines.
    // Preserve newlines so every diagnostic still names the source line.
    let (clean, control) = gamess_content(text);
    let all: Vec<_> = lines(&clean).collect();
    // GAMESS permits repeated groups and assignments; the last value wins.
    let control = control.to_ascii_uppercase();
    let coord = assignment(&control, "COORD").unwrap_or("UNIQUE");
    let unsupported_coord = || {
        Error::Unsupported(format!(
            "GAMESS COORD={coord} is not supported; save Cartesian coordinates with COORD=UNIQUE or CART"
        ))
    };
    if coord == "FRAGONLY" {
        return Err(unsupported_coord());
    }
    let mut rows = all.into_iter();
    rows.by_ref()
        .find(|(_, row)| row.eq_ignore_ascii_case("$DATA"))
        .ok_or_else(|| Error::Invalid("The GAMESS input has no $DATA group".into()))?;
    let (_, title) = rows
        .next()
        .ok_or_else(|| Error::Invalid("The GAMESS $DATA group has no title".into()))?;
    let (line, group) = rows
        .next()
        .ok_or_else(|| Error::Invalid("The GAMESS $DATA group has no point group".into()))?;
    if !group.eq_ignore_ascii_case("C1") {
        let point_group = group.to_ascii_uppercase();
        let mut fields = point_group.split_whitespace();
        let point_group = match (fields.next(), fields.next()) {
            (Some(family), Some(order)) if order.bytes().all(|c| c.is_ascii_digit()) => {
                family.replacen('N', order, 1)
            }
            (Some(_), _) => point_group,
            _ => return Err(Error::at(line, "Expected the GAMESS point group")),
        };
        return Err(Error::Unsupported(format!(
            "GAMESS input with symmetry {point_group} lists only the unique atoms; save it with C1 symmetry to open it"
        )));
    }
    if !matches!(coord, "UNIQUE" | "CART" | "PRINAXIS" | "ZMT") {
        return Err(unsupported_coord());
    }
    let scale = match assignment(&control, "UNITS") {
        Some("BOHR") => ANGSTROM_PER_BOHR,
        None | Some("ANGS") => 1.,
        Some(units) => {
            return Err(Error::Unsupported(format!(
                "GAMESS UNITS={units} is not supported; use ANGS or BOHR"
            )));
        }
    };
    let (charge, mult) = spin(
        assignment(&control, "ICHARG").unwrap_or("0"),
        assignment(&control, "MULT").unwrap_or("1"),
        line,
    )?;

    let mut data = Vec::new();
    let mut ended = false;
    for (line, row) in rows {
        if row.eq_ignore_ascii_case("$END") {
            ended = true;
            break;
        }
        data.push((line, row));
    }
    if !ended {
        return Err(Error::Invalid(
            "The GAMESS $DATA group is missing $END".into(),
        ));
    }
    if coord == "ZMT" {
        let mut rest = data.as_slice();
        let rows = section(&mut rest);
        let end = rows
            .iter()
            .position(|(_, row)| variable_header(row))
            .unwrap_or(rows.len());
        let (rows, variable_lines) = rows.split_at(end);
        let variables = gaussian_variables(variable_lines.iter().chain(rest));
        let frame = coordinates(
            rows,
            CoordinateKind::ZMatrix,
            &variables,
            scale,
            charge,
            mult,
        )?;
        return Ok(geometry(Format::GamessInput, geometry_job(frame, title)));
    }

    let mut atoms = Vec::new();
    let mut positions = Vec::new();
    let mut rows = data.into_iter().peekable();
    let mut basis = false;
    while let Some((line, row)) = rows.next() {
        if row.is_empty() {
            basis = false;
            continue;
        }
        let fields: Vec<_> = words(row).collect();
        if coord == "UNIQUE"
            && !atoms.is_empty()
            && let [shell, count, scales @ ..] = fields.as_slice()
            && let Ok(count) = count.parse::<usize>()
            && count > 0
            && scales.iter().all(|value| real(value).is_some())
        {
            let builtin = matches!(
                shell.to_ascii_uppercase().as_str(),
                "STO" | "N21" | "N31" | "N311"
            );
            let explicit = matches!(
                shell.to_ascii_uppercase().as_str(),
                "S" | "P" | "L" | "SP" | "D" | "F" | "G" | "H" | "I"
            );
            // A scaled shell card can have five fields like a sulfur atom.
            // Numbered primitives distinguish it from another Cartesian row.
            if builtin
                || explicit
                    && (basis
                        || fields.len() != 5
                        || element(shell).is_none_or(|(z, _)| usize::from(z) != count)
                        || rows
                            .peek()
                            .is_some_and(|(_, row)| words(row).next() == Some("1")))
            {
                basis = true;
                // Built-in cards select a basis; explicit shells supply NPRIM records.
                if !builtin {
                    for index in 1..=count {
                        let (primitive_line, primitive) = rows.next().ok_or_else(|| {
                            Error::at(line, format!("GAMESS basis is missing primitive {index}"))
                        })?;
                        let primitive: Vec<_> = words(primitive).collect();
                        if !primitive.split_first().is_some_and(|(number, values)| {
                            values.len() >= 2
                                && number.parse::<usize>().ok() == Some(index)
                                && values.iter().all(|value| real(value).is_some())
                        }) {
                            return Err(Error::at(
                                primitive_line,
                                format!("Expected GAMESS basis primitive {index}"),
                            ));
                        }
                    }
                }
                continue;
            }
        }
        if basis {
            return Err(Error::at(
                line,
                "Expected a GAMESS basis card or a blank line before the next atom",
            ));
        }
        let &[name, nuclear, x, y, z] = fields.as_slice() else {
            return Err(Error::at(
                line,
                "Expected a GAMESS atom name, nuclear charge and x, y, z",
            ));
        };
        let atomic_number = real(nuclear)
            .map(f64::round)
            .filter(|z| (1. ..=118.).contains(z))
            .ok_or_else(|| Error::at(line, format!("Invalid nuclear charge of {name}")))?;
        atoms.push((atomic_number as u8, 0));
        positions.push(cartesian([x, y, z], line)?.map(|x| x * scale));
    }
    let frame = frame(&atoms, positions, charge, mult)?;
    Ok(geometry(Format::GamessInput, geometry_job(frame, title)))
}

#[cfg(test)]
#[path = "tests/input.rs"]
mod tests;
