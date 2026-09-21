use super::*;

pub(super) fn read(r: &mut Reader<'_>, p: &mut Parsed, n: usize, e: usize) -> Result<()> {
    for _ in 0..n {
        let line = r.next()?;
        let symbol = r.field(line, 31, 34)?.trim();
        let mut atom = r.symbol(symbol, false)?;
        let position = Point3 {
            x: r.coordinate(r.field(line, 0, 10)?, true)?,
            y: r.coordinate(r.field(line, 10, 20)?, true)?,
            z: r.coordinate(r.field(line, 20, 30)?, true)?,
        };
        let mass = r.optional_integer(line, 34, 36)?;
        if mass != 0 {
            let common = ELEMENTS
                .get(usize::from(atom.atomic_number))
                .ok_or_else(|| r.invalid("Unknown element"))?
                .common_isotope;
            // Native atom isotope storage is uint16, including old negative
            // dummy-atom offsets. Keep that defined wrap without integer UB.
            atom.isotope = (i32::from(common) + mass).rem_euclid(65536) as u16;
        }
        let charge = r.optional_integer(line, 36, 39)?;
        if charge != 0 {
            atom.charge = i8::try_from(4 - charge).map_err(|_| r.invalid("Excessive charge"))?;
        }
        if r.optional_integer(line, 42, 45)? >= 1 {
            return Err(ReadError::Unsupported("hydrogen query"));
        }
        let valence = r.optional_integer(line, 48, 51)?;
        let map = r.optional_integer(line, 60, 63)?;
        // Parity is parsed for validity but native import perceives stereo from
        // coordinates/wedges, not this advisory atom field.
        for start in [39, 45, 54, 57, 63, 66] {
            r.optional_integer(line, start, start + 3)?;
        }
        let map_present = line.get(60..63).is_some_and(|s| s != "  0");
        p.atom(
            atom,
            position,
            AtomMetadata {
                map_number: map,
                map_present,
                ..AtomMetadata::default()
            },
            FileAtom {
                valence,
                hyd_override: false,
                attachment: None,
                dummy_label: dummy_label(symbol),
            },
        );
    }
    for _ in 0..e {
        let line = r.next()?;
        let a = r.index(r.field_integer(line, 0, 3)?, n)?;
        let b = r.index(r.field_integer(line, 3, 6)?, n)?;
        let (kind, props) = order(r.field_integer(line, 6, 9)?, false);
        let code = r.optional_integer(line, 9, 12).unwrap_or(0);
        let dir = match code {
            1 => Direction::Wedge,
            6 => Direction::Hash,
            3 => Direction::EitherDouble,
            4 => Direction::Unknown,
            _ => Direction::None,
        };
        p.chirality |= !matches!(dir, Direction::None | Direction::Unknown);
        if r.optional_integer(line, 15, 18).unwrap_or(0) != 0 {
            return Err(ReadError::Unsupported("bond topology query"));
        }
        p.bond(a, b, kind, dir, props);
    }
    properties(r, p)
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
                let count = r.count(r.field_integer(line, 6, 9)?, 999)?;
                for i in 0..count {
                    let start = 9 + i * 8;
                    let index = r.index(
                        r.field_integer(line, start, start + 4)?,
                        if prop == "ZBO" {
                            p.graph.bonds.len()
                        } else {
                            p.graph.atoms.len()
                        },
                    )?;
                    let field = line.get(start + 4..start + 8).unwrap_or("");
                    let value = if field.trim().is_empty() {
                        if prop == "ISO" {
                            continue;
                        }
                        if prop == "HYD" { -1 } else { 0 }
                    } else {
                        r.integer(field)?
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
                r.index(r.field_integer(line, 7, 10)?, p.graph.atoms.len())?;
                r.field(line, 14, 15)?;
                if r.field_integer(line, 10, 13)? != 0 {
                    return Err(ReadError::Unsupported("atom list query"));
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
                let count = r.count(r.field_integer(line, 6, 9)?, 999)?;
                for i in 0..count {
                    let start = 9 + 16 * i;
                    r.index(
                        r.field_integer(line, start, start + 4)?,
                        p.graph.atoms.len(),
                    )?;
                    if r.field_integer(line, start + 4, start + 8)? < 2 {
                        return Err(r.invalid("Invalid link-node repeat count"));
                    }
                    r.index(
                        r.field_integer(line, start + 8, start + 12)?,
                        p.graph.atoms.len(),
                    )?;
                    let other = r.optional_integer(line, start + 12, start + 16)?;
                    // The pinned native reader rejects the single-substituent
                    // V2000 form; preserve its accepted-input boundary.
                    r.index(other, p.graph.atoms.len())?;
                }
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
