use super::*;

pub(super) fn read(r: &mut Reader<'_>, p: &mut Parsed, n: usize, e: usize) -> Result<()> {
    for _ in 0..n {
        let line = r.next()?;
        // Text pasted from chats or web pages can lose leading spaces. When
        // the fixed columns fail, put them back from where the symbol starts
        // (column 31), so blank fields keep their columns; read whitespace
        // tokens only when that fails too.
        let record = atom_record(r, line, None)
            .or_else(|error| {
                restore_padding(line, 3, 31)
                    .and_then(|padded| atom_record(r, &padded, None).ok())
                    .ok_or(error)
            })
            .or_else(|error| {
                let tokens = line.split_ascii_whitespace().collect::<Vec<_>>();
                atom_record(r, line, Some(&tokens)).or(Err(error))
            })?;
        p.atom(record.atom, record.position, record.metadata, record.props);
    }
    for _ in 0..e {
        let line = r.next()?;
        let width = line.trim_end().len();
        let fixed = bond_record(r, line, n, None);
        // Fields are three columns wide and right-aligned, so a line that lost
        // leading spaces has the wrong length and its fixed reading can drop
        // the stereo. Putting the spaces back keeps blank fields and
        // run-together numbers such as `99100` or `100101` in their columns.
        let record = if fixed.is_ok() && width.is_multiple_of(3) {
            fixed
        } else {
            pad(line, (3 - width % 3) % 3)
                .and_then(|padded| bond_record(r, &padded, n, None).ok())
                .map_or_else(
                    || {
                        let tokens = line.split_ascii_whitespace().collect::<Vec<_>>();
                        bond_record(r, line, n, Some(&tokens)).or(fixed)
                    },
                    Ok,
                )
        }?;
        let dir = match record.stereo {
            1 => Direction::Wedge,
            6 => Direction::Hash,
            3 => Direction::EitherDouble,
            4 => Direction::Unknown,
            _ => Direction::None,
        };
        p.chirality |= !matches!(dir, Direction::None | Direction::Unknown);
        if record.topology != 0 {
            return Err(ReadError::Unsupported("bond topology query"));
        }
        let (kind, props) = order(record.kind, false);
        p.bond(record.a, record.b, kind, dir, props);
    }
    properties(r, p)
}

struct AtomRecord {
    atom: Atom,
    position: Point3,
    metadata: AtomMetadata,
    props: FileAtom,
}

fn atom_record(r: &Reader<'_>, line: &str, tokens: Option<&[&str]>) -> Result<AtomRecord> {
    let field = |start: usize, end: usize, token: usize| match tokens {
        Some(tokens) => tokens
            .get(token)
            .copied()
            .ok_or_else(|| r.invalid("Missing atom field")),
        None => r.field(line, start, end),
    };
    let integer = |start: usize, end: usize, token: usize| match tokens {
        Some(tokens) => tokens
            .get(token)
            .map(|s| r.integer(s))
            .transpose()
            .map(|n| n.unwrap_or(0)),
        None => r.optional_integer(line, start, end),
    };
    let symbol = field(31, 34, 3)?.trim();
    let mut atom = r.symbol(symbol, false)?;
    let position = Point3 {
        x: r.coordinate(field(0, 10, 0)?, tokens.is_none())?,
        y: r.coordinate(field(10, 20, 1)?, tokens.is_none())?,
        z: r.coordinate(field(20, 30, 2)?, tokens.is_none())?,
    };
    let mass = integer(34, 36, 4)?;
    if mass != 0 {
        let common = ELEMENTS
            .get(usize::from(atom.atomic_number))
            .ok_or_else(|| r.invalid("Unknown element"))?
            .common_isotope;
        // Native atom isotope storage is uint16, including old negative
        // dummy-atom offsets. Keep that defined wrap without integer UB.
        atom.isotope = (i64::from(common) + i64::from(mass)).rem_euclid(65536) as u16;
    }
    let charge = integer(36, 39, 5)?;
    if charge != 0 {
        let charge = 4i32
            .checked_sub(charge)
            .ok_or_else(|| r.invalid("Excessive charge"))?;
        atom.charge = i8::try_from(charge).map_err(|_| r.invalid("Excessive charge"))?;
    }
    if integer(42, 45, 7)? >= 1 {
        return Err(ReadError::Unsupported("hydrogen query"));
    }
    let valence = integer(48, 51, 9)?;
    let map = integer(60, 63, 13)?;
    // Parity is parsed for validity but native import perceives stereo from
    // coordinates/wedges, not this advisory atom field.
    for (start, token) in [(39, 6), (45, 8), (54, 11), (57, 12), (63, 14), (66, 15)] {
        integer(start, start + 3, token)?;
    }
    if tokens.is_some() {
        integer(51, 54, 10)?;
    }
    let map_present = match tokens {
        Some(tokens) => tokens.get(13).is_some_and(|s| *s != "0"),
        None => line.get(60..63).is_some_and(|s| s != "  0"),
    };
    Ok(AtomRecord {
        atom,
        position,
        metadata: AtomMetadata {
            map_number: map,
            map_present,
            ..AtomMetadata::default()
        },
        props: FileAtom {
            valence,
            hyd_override: false,
            attachment: None,
            dummy_label: dummy_label(symbol),
            query: FileQuery::None,
        },
    })
}

struct BondRecord {
    a: usize,
    b: usize,
    kind: i32,
    stereo: i32,
    topology: i32,
}

fn bond_record(
    r: &Reader<'_>,
    line: &str,
    n: usize,
    tokens: Option<&[&str]>,
) -> Result<BondRecord> {
    let integer = |start: usize, token: usize| match tokens {
        Some(tokens) => r.integer(
            tokens
                .get(token)
                .copied()
                .ok_or_else(|| r.invalid("Missing bond field"))?,
        ),
        None => r.field_integer(line, start, start + 3),
    };
    let optional = |start: usize, token: usize| match tokens {
        Some(tokens) => tokens
            .get(token)
            .and_then(|s| r.integer(s).ok())
            .unwrap_or(0),
        None => r.optional_integer(line, start, start + 3).unwrap_or(0),
    };
    Ok(BondRecord {
        a: r.index(integer(0, 0)?, n)?,
        b: r.index(integer(3, 1)?, n)?,
        kind: integer(6, 2)?,
        stereo: optional(9, 3),
        topology: optional(15, 5),
    })
}

fn properties(r: &mut Reader<'_>, p: &mut Parsed) -> Result<()> {
    let mut groups = std::mem::take(&mut p.groups);
    let mut first_charge = true;
    let mut first_line = true;
    loop {
        let line = r.next()?;
        if line.starts_with("M  END") {
            p.groups = groups;
            return Ok(());
        }
        if line.starts_with("$$$$") || first_line && line.is_empty() {
            return Err(r.invalid("Missing M END"));
        }
        if first_line && !line.starts_with(['M', 'A', 'V', 'G', 'S']) {
            return Err(ReadError::Unsupported("legacy atom-list query"));
        }
        first_line = false;
        match line.get(..6).unwrap_or(line) {
            "M  CHG" | "M  RAD" | "M  ISO" | "M  ZCH" | "M  HYD" | "M  ZBO" | "M  SUB"
            | "M  UNS" | "M  RBC" | "M  RGP" | "M  APO" => {
                let prop = r.field(line, 3, 6)?;
                if matches!(prop, "CHG" | "RAD") && first_charge {
                    p.graph.atoms.iter_mut().for_each(|a| a.charge = 0);
                    first_charge = false;
                }
                let entries = property_tokens(r, line, prop);
                let count = match &entries {
                    Some(entries) => entries.len(),
                    None => r.count(r.field_integer(line, 6, 9)?, 999)?,
                };
                for i in 0..count {
                    let start = 9 + i * 8;
                    let entry = entries.as_ref().and_then(|entries| entries.get(i));
                    let index = r.index(
                        match entry {
                            Some(&(index, _)) => index,
                            None => r.field_integer(line, start, start + 4)?,
                        },
                        if prop == "ZBO" {
                            p.graph.bonds.len()
                        } else {
                            p.graph.atoms.len()
                        },
                    )?;
                    let value = match entry {
                        Some(&(_, value)) => value,
                        None => {
                            let field = line.get(start + 4..start + 8).unwrap_or("");
                            if field.trim().is_empty() {
                                if prop == "ISO" {
                                    continue;
                                }
                                if prop == "HYD" { -1 } else { 0 }
                            } else {
                                r.integer(field)?
                            }
                        }
                    };
                    if matches!(prop, "SUB" | "UNS" | "RBC") {
                        if value != 0 {
                            return Err(ReadError::Unsupported("atom property query"));
                        }
                        continue;
                    }
                    if prop == "RGP" {
                        return Err(ReadError::Unsupported("R-group query"));
                    }
                    if prop == "APO" {
                        if !(0..=3).contains(&value) {
                            return Err(r.invalid("Invalid attachment point"));
                        }
                        if value != 0 {
                            let props = p
                                .atoms
                                .get_mut(index)
                                .ok_or_else(|| r.invalid("Missing attachment atom"))?;
                            if props.attachment.is_some() {
                                return Err(r.invalid("Duplicate attachment point"));
                            }
                            props.attachment = Some(if value == 3 { -1 } else { value });
                        }
                        continue;
                    }
                    if prop == "ZBO" {
                        let bond = p
                            .graph
                            .bonds
                            .get_mut(index)
                            .ok_or_else(|| r.invalid("Missing ZBO bond"))?;
                        bond.order = match value {
                            1..=3 => value as u8,
                            4 => 6,
                            7 => 7,
                            12 => 4,
                            17 => 5,
                            14 => 0,
                            _ => return Err(ReadError::Unsupported("ZBO bond type")),
                        };
                        p.bonds.get_mut(index).ok_or(ReadError::Limit)?.unspecified = false;
                        continue;
                    }
                    let atom = p
                        .graph
                        .atoms
                        .get_mut(index)
                        .ok_or_else(|| r.invalid("Missing property atom"))?;
                    match prop {
                        "CHG" | "ZCH" => {
                            atom.charge =
                                i8::try_from(value).map_err(|_| r.invalid("Excessive charge"))?
                        }
                        "RAD" => atom.radical_electrons = radical(value)?,
                        "ISO" if value >= 0 => {
                            atom.isotope =
                                u16::try_from(value).map_err(|_| r.invalid("Excessive isotope"))?
                        }
                        "HYD" if value >= 0 => {
                            atom.explicit_hydrogens = u8::try_from(value)
                                .map_err(|_| r.invalid("Excessive hydrogen count"))?;
                            p.atoms
                                .get_mut(index)
                                .ok_or_else(|| r.invalid("Missing hydrogen override"))?
                                .hyd_override = true;
                        }
                        _ => (),
                    }
                }
            }
            "M  ALS" => {
                let atom = r.index(r.field_integer(line, 7, 10)?, p.graph.atoms.len())?;
                let excluded = match r.field(line, 14, 15)? {
                    "T" => true,
                    "F" => false,
                    _ => return Err(r.invalid("Invalid atom-list exclusion flag")),
                };
                let count = r.count(r.field_integer(line, 10, 13)?, 118)?;
                if count == 0 {
                    return Err(r.invalid("An atom list cannot be empty"));
                }
                let mut choices = Vec::new();
                for i in 0..count {
                    let symbol = r.field(line, 16 + 4 * i, 20 + 4 * i)?.trim();
                    let a = r.symbol(symbol, false)?;
                    if a.atomic_number == 0 {
                        return Err(ReadError::Unsupported("non-element MOL alternatives"));
                    }
                    choices.push(crate::generic::Alternative::Element {
                        symbol: ELEMENTS
                            .get(usize::from(a.atomic_number))
                            .ok_or(ReadError::Limit)?
                            .symbol
                            .into(),
                        isotope: a.isotope.into(),
                    });
                }
                if count > 0 {
                    p.graph
                        .atoms
                        .get_mut(atom)
                        .ok_or(ReadError::Limit)?
                        .atomic_number = 0;
                    p.generic
                        .alternatives
                        .push(crate::generic::AlternativeSite {
                            atom: atom as u64 + 1,
                            choices,
                            excluded,
                        });
                }
            }
            "M  MRV" if line.starts_with("M  MRV SMA") => {
                return Err(ReadError::Unsupported("atom SMARTS query"));
            }
            "M  PXA" => {
                r.index(r.field_integer(line, 7, 10)?, p.graph.atoms.len())?;
            }
            "M  STY" | "M  SAL" | "M  SDT" | "M  SDD" | "M  SED" | "M  SCD" | "M  SST"
            | "M  SLB" | "M  SCN" | "M  SDS" | "M  SBL" | "M  SPA" | "M  SMT" | "M  SDI"
            | "M  SBV" | "M  SPL" | "M  SNC" | "M  SAP" | "M  SCL" | "M  SBT" => {
                groups.read_v2000(r, p, line)?
            }
            "M  LIN" => {
                return Err(ReadError::Unsupported(
                    "V2000 link nodes; use V3000 LINKNODE, CDXML or native ReShiki",
                ));
            }
            "M  CRS" => return Err(ReadError::Unsupported("SGroup subtype")),
            "S  SKP" => {
                let count = r.count(r.field_integer(line, 6, 9)?, 1_000_000)?;
                for _ in 0..count {
                    r.next()?;
                }
            }
            _ if line.starts_with('A') => {
                r.index(r.field_integer(line, 3, 6)?, p.graph.atoms.len())?;
                r.next()?;
            }
            _ if line.starts_with('G') => {
                r.next()?;
            }
            _ if line.starts_with('V') => {
                r.index(r.field_integer(line, 3, 6)?, p.graph.atoms.len())?;
            }
            // Native readers ignore unrecognized extension records.
            _ => (),
        }
    }
}

/// Entries of a property line whose tokens are exactly `M`, the tag, the count
/// and that many pairs. Well-formed lines read the same either way; blank
/// values and run-together numbers keep the fixed columns.
fn property_tokens(r: &Reader<'_>, line: &str, prop: &str) -> Option<Vec<(i32, i32)>> {
    let mut tokens = line.split_ascii_whitespace();
    if tokens.next()? != "M" || tokens.next()? != prop {
        return None;
    }
    let count = r.count(r.integer(tokens.next()?).ok()?, 999).ok()?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        entries.push((tokens.next()?.parse().ok()?, tokens.next()?.parse().ok()?));
    }
    tokens.next().is_none().then_some(entries)
}
