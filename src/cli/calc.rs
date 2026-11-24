//! CLI-only calculation sessions call the xTB worker directly.

#[cfg(any(feature = "xtb", test))]
struct Geometry {
    numbers: Vec<u8>,
    positions: Vec<[f64; 3]>,
}

/// Read exactly the first plain XYZ frame without rounding its coordinates.
#[cfg(any(feature = "xtb", test))]
fn xyz(input: impl std::io::BufRead) -> Result<Geometry, String> {
    let mut lines = input
        .lines()
        .map(|line| line.map_err(|error| format!("cannot read XYZ: {error}")));
    let count = lines
        .next()
        .transpose()?
        .and_then(|line| line.trim().parse::<usize>().ok())
        .filter(|count| (1..=reshiki_xtb_worker::protocol::MAX_ATOMS).contains(count))
        .ok_or("the first XYZ line must be an atom count from 1 to 500")?;
    lines
        .next()
        .transpose()?
        .ok_or("the XYZ comment line is missing")?;
    let mut numbers = Vec::with_capacity(count);
    let mut positions = Vec::with_capacity(count);
    for atom in 0..count {
        let row = atom + 3;
        let line = lines
            .next()
            .transpose()?
            .ok_or_else(|| format!("XYZ ends before atom {} of {count}", atom + 1))?;
        let mut columns = line.split_whitespace();
        let symbol = columns
            .next()
            .ok_or_else(|| format!("XYZ line {row} needs an element and three coordinates"))?;
        let number = crate::chemistry::ELEMENTS
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(number, element)| {
                element
                    .symbol
                    .eq_ignore_ascii_case(symbol)
                    .then_some(number)
            })
            .and_then(|number| u8::try_from(number).ok())
            .ok_or_else(|| format!("XYZ line {row}: `{symbol}` is not an element symbol"))?;
        let coordinate = |value: Option<&str>| {
            value
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite())
                .ok_or_else(|| format!("XYZ line {row} needs three finite coordinates"))
        };
        let position = [
            coordinate(columns.next())?,
            coordinate(columns.next())?,
            coordinate(columns.next())?,
        ];
        if columns.next().is_some() {
            return Err(format!("XYZ line {row} must have exactly four columns"));
        }
        numbers.push(number);
        positions.push(position);
    }
    Ok(Geometry { numbers, positions })
}

#[cfg(feature = "xtb")]
pub(super) fn run(
    command: super::args::Calc,
    out: &mut dyn std::io::Write,
) -> Result<(), super::host::CliError> {
    use super::host::CliError;
    use reshiki_xtb_worker::{
        client::{Cancel, Limits, Session},
        protocol::{Compute, Method, Open, Units},
    };
    use serde_json::json;
    use std::{
        fs::File,
        io::{BufReader, Read},
    };

    let file_error =
        |error| CliError::Failed(format!("cannot read {}: {error}", command.file.display()));
    if !std::fs::metadata(&command.file)
        .map_err(file_error)?
        .is_file()
    {
        return Err(CliError::Failed(format!(
            "{} is not a regular XYZ file",
            command.file.display()
        )));
    }
    let file = File::open(&command.file).map_err(file_error)?;
    let limit = reshiki_agent::ops::budget::Budgets::default().max_text_bytes;
    let mut frame = BufReader::new(file).take(limit as u64 + 1);
    let geometry = xyz(&mut frame).map_err(CliError::Failed)?;
    if frame.limit() == 0 {
        return Err(CliError::Failed(format!(
            "the first XYZ frame exceeds {limit} bytes"
        )));
    }
    let executable = std::env::current_exe().map_err(|error| {
        CliError::Failed(format!("cannot locate the xTB worker executable: {error}"))
    })?;
    let limits = Limits::default();
    let open = Open {
        numbers: geometry.numbers,
        positions: geometry.positions,
        units: Units::Angstrom,
        charge: command.charge,
        uhf: command.uhf,
        method: Method::Gfn2,
        solvent: None,
        threads: 1,
        heap_bytes: limits.heap_bytes,
    };
    fn failed(error: impl std::fmt::Display) -> CliError {
        CliError::Failed(error.to_string())
    }
    let (mut session, hello) = Session::start(&executable, limits).map_err(failed)?;
    let cancel = Cancel::default();
    let opened = session.open(&open, &cancel).map_err(failed)?;
    let computed = session
        .compute(
            &Compute {
                positions: None,
                units: Units::Angstrom,
                gradient: command.gradient,
            },
            &cancel,
        )
        .map_err(failed)?;
    session.close().map_err(failed)?;
    let mut result = serde_json::Map::from_iter([
        ("energy".into(), json!(computed.energy)),
        ("method".into(), json!(opened.method)),
        ("numerics".into(), json!(opened.numerics)),
        ("engine".into(), json!(hello.engine)),
    ]);
    if command.gradient {
        let gradient = computed
            .gradient
            .ok_or_else(|| CliError::Failed("the xTB worker returned no gradient".into()))?;
        result.insert("gradient".into(), json!(gradient));
    }
    writeln!(out, "{}", serde_json::Value::Object(result)).map_err(CliError::Stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_first_frame_without_rounding_or_later_frame_validation() {
        let geometry = xyz(
            b"3\nwater\nO 0 0 0\nH 0.9584 0 0\nH -0.2396 0.9271 0\ninvalid later frame: \xFF"
                .as_slice(),
        )
        .unwrap();
        assert_eq!(geometry.numbers, [8, 1, 1]);
        assert_eq!(
            geometry.positions,
            [[0., 0., 0.], [0.9584, 0., 0.], [-0.2396, 0.9271, 0.]]
        );
    }

    #[test]
    fn reads_element_symbols_without_ascii_case_sensitivity() {
        let geometry = xyz(b"3\ncase variants\nh 0 0 0\nCL 1 0 0\ncl 2 0 0\n".as_slice()).unwrap();
        assert_eq!(geometry.numbers, [1, 17, 17]);
    }

    #[test]
    fn rejects_invalid_or_truncated_first_frames() {
        for text in [
            "",
            "0\ncomment\n",
            "501\ncomment\n",
            "1",
            "2\ncomment\nH 0 0 0\n",
            "1\ncomment\nXx 0 0 0\n",
            "1\ncomment\n* 0 0 0\n",
            "1\ncomment\nH 0 0\n",
            "1\ncomment\nH NaN 0 0\n",
            "1\ncomment\nH 0 inf 0\n",
            "1\ncomment\nH 0 0 1e999\n",
            "1\ncomment\nH 0 0 0 extra\n",
            "1\ncomment\n\n",
        ] {
            assert!(xyz(text.as_bytes()).is_err(), "{text:?}");
        }
    }
}
