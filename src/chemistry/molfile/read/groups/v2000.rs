use super::*;

impl Groups {
    pub(in crate::chemistry::molfile::read) fn read_v2000(
        &mut self,
        r: &Reader<'_>,
        p: &Parsed,
        text: &str,
    ) -> Result<()> {
        let kind = r.field(text, 3, 6)?;
        let mut f = Fixed {
            reader: r,
            text,
            pos: 6,
        };
        if kind == "STY" {
            for _ in 0..f.count()? {
                f.minimum(8)?;
                let id = f.integer(false)?;
                let group = Group::new(r, r.field(text, f.pos + 1, f.pos + 4)?)?;
                self.entries.entry(id).or_insert(group);
                f.pos += 4;
            }
            return Ok(());
        }
        if matches!(kind, "SST" | "SLB" | "SCN" | "SPL" | "SNC" | "SBT" | "SDS") {
            if kind == "SDS" {
                if !text.starts_with("M  SDS EXP") {
                    return Err(r.invalid("Invalid expanded-group record"));
                }
                f.pos = 10;
            }
            for _ in 0..f.count()? {
                f.minimum(if kind == "SCN" {
                    7
                } else if kind == "SDS" {
                    4
                } else {
                    8
                })?;
                let id = f.integer(false)?;
                if !self.entries.contains_key(&id) {
                    return Ok(());
                }
                match kind {
                    "SDS" => (),
                    "SST" | "SCN" => {
                        let len = if kind == "SCN" { 2 } else { 3 };
                        let value = r.field(text, f.pos + 1, f.pos + 1 + len)?;
                        let allowed: &[&str] = if kind == "SCN" {
                            &["HH", "HT", "EU"]
                        } else {
                            &["ALT", "RAN", "BLO"]
                        };
                        if !allowed.contains(&value) {
                            return Err(r.invalid("Invalid substance-group subtype or connection"));
                        }
                        f.pos += 4;
                    }
                    _ => {
                        let value = f.integer(false)?;
                        if kind == "SNC" && !(0..=256).contains(&value)
                            || kind == "SBT" && !matches!(value, 0 | 1)
                        {
                            return Err(
                                r.invalid("Invalid substance-group component or bracket type")
                            );
                        }
                    }
                }
            }
            return Ok(());
        }
        let id = f.integer(false)?;
        let Some(group) = self.entries.get_mut(&id) else {
            return Ok(());
        };
        match kind {
            "SAL" | "SPA" | "SBL" => {
                for _ in 0..f.count()? {
                    f.minimum(4)?;
                    let mark = f.integer(false)?;
                    let index = r.index(
                        mark,
                        if kind == "SBL" {
                            p.graph.bonds.len()
                        } else {
                            p.graph.atoms.len()
                        },
                    )?;
                    match kind {
                        "SAL" => group.atom(index),
                        "SBL" => group.bond(index),
                        _ => group.parent(r, index)?,
                    }
                }
            }
            "SDI" => {
                if f.count()? != 4 {
                    return Err(r.invalid("Invalid bracket coordinate count"));
                }
                for _ in 0..4 {
                    f.float()?;
                }
            }
            "SMT" | "SCL" => {
                f.minimum(2)?;
            }
            "SBV" => {
                let index = r.index(f.integer(false)?, p.graph.bonds.len())?;
                if group.kind == "SUP" {
                    f.float()?;
                    f.float()?;
                }
                group.crossing(r, p, index)?;
            }
            "SDT" => {
                for (start, end, target) in [
                    (11, 41, &mut group.field),
                    (63, 65, &mut group.query),
                    (65, text.len(), &mut group.operator),
                ] {
                    if let Some(value) = text.get(start..end.min(text.len())) {
                        let value = value.trim_end();
                        if !value.is_empty() {
                            *target = Some(value.to_owned());
                        }
                    }
                }
            }
            "SDD" => (),
            "SCD" | "SED" => {
                if self.data_group != 0 && self.data_group != id {
                    return Err(r.invalid("Mixed substance-group data continuation"));
                }
                if self.data_group == 0 && kind == "SCD" {
                    self.data_group = id;
                } else if kind == "SED" {
                    self.data_group = 0;
                }
                if kind == "SCD" && self.data_lines > 2 {
                    return Err(r.invalid("Too many substance-group continuation lines"));
                }
                if f.pos + 1 < text.len() {
                    let content = r.field(
                        text,
                        f.pos + 1,
                        text.floor_char_boundary((f.pos + 70).min(text.len())),
                    )?;
                    self.data.push_str(content);
                    if kind == "SED" {
                        let data = self.data.trim_end();
                        group.data.push(
                            data.get(..data.floor_char_boundary(200.min(data.len())))
                                .ok_or(ReadError::Limit)?
                                .to_owned(),
                        );
                        self.data.clear();
                        self.data_lines = 0;
                    } else {
                        self.data_lines += 1;
                    }
                }
            }
            "SAP" => {
                for _ in 0..f.count()? {
                    f.minimum(11)?;
                    r.index(f.integer(false)?, p.graph.atoms.len())?;
                    let leaving = f.integer(false)?;
                    if leaving != 0 {
                        r.index(leaving, p.graph.atoms.len())?;
                    }
                    f.pos += 3;
                }
            }
            _ => return Err(r.invalid("Unknown substance-group record")),
        }
        Ok(())
    }
}
