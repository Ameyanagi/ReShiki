use super::*;

struct Fields<'a> {
    text: &'a str,
    pos: usize,
}
impl<'a> Fields<'a> {
    fn byte(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }
    fn advance(&mut self) -> Option<u8> {
        let b = self.byte()?;
        self.pos += 1;
        Some(b)
    }
    fn whitespace(&mut self) {
        while self.byte().is_some_and(|b| b.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }
    fn word(&mut self, r: &Reader<'_>) -> Result<&'a str> {
        self.whitespace();
        let start = self.pos;
        while self.byte().is_some_and(|b| !b.is_ascii_whitespace()) {
            self.pos += 1;
        }
        r.field(self.text, start, self.pos)
    }
    fn integer(&mut self, r: &Reader<'_>) -> Result<i32> {
        self.whitespace();
        let start = self.pos;
        if matches!(self.byte(), Some(b'+' | b'-')) {
            self.pos += 1;
        }
        while self.byte().is_some_and(|b| b.is_ascii_digit()) {
            self.pos += 1;
        }
        let text = r.field(self.text, start, self.pos)?;
        let n = text
            .parse::<i64>()
            .map_err(|_| r.invalid("Invalid substance-group integer"))?;
        if !(-2147483648..=4294967295).contains(&n) {
            return Err(r.invalid("Excessive substance-group integer"));
        }
        Ok(n as i32)
    }
    fn float(&mut self, r: &Reader<'_>) -> Result<f64> {
        self.whitespace();
        let start = self.pos;
        while self
            .byte()
            .is_some_and(|b| b.is_ascii_digit() || b"+-.eE".contains(&b))
        {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(r.invalid("Missing substance-group coordinate"));
        }
        r.coordinate(r.field(self.text, start, self.pos)?, false)
    }
    fn label(&mut self, r: &Reader<'_>) -> Result<Option<&'a str>> {
        if self.byte().is_none() {
            return Ok(None);
        }
        if self.advance() != Some(b' ') {
            return Err(r.invalid("Missing substance-group label separator"));
        }
        let rest = self.text.get(self.pos..).ok_or(ReadError::Limit)?;
        if let Some((key, _)) = rest.split_once('=') {
            self.pos += key.len() + 1;
            Ok(Some(key))
        } else {
            self.pos = self.text.len();
            Ok(if rest.is_empty() { None } else { Some(rest) })
        }
    }
    fn string(&mut self, r: &Reader<'_>) -> Result<String> {
        if self.byte() == Some(b' ') {
            return Ok(String::new());
        }
        if self.byte() == Some(b'\'') {
            self.pos += 1;
            return Ok(String::new());
        }
        if self.byte() != Some(b'"') {
            return Ok(self.word(r)?.trim_end().to_owned());
        }
        self.pos += 1;
        let mut bytes = Vec::new();
        while let Some(b) = self.advance() {
            if b == b'"' {
                if self.byte() == Some(b'"') {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            bytes.push(b);
        }
        String::from_utf8(bytes)
            .map(|s| s.trim_end().to_owned())
            .map_err(|_| r.invalid("Invalid group string encoding"))
    }
    fn skip(&mut self, r: &Reader<'_>) -> Result<()> {
        if self.byte() == Some(b' ') {
            return Err(r.invalid("Unexpected blank overridden group default"));
        }
        if self.byte() == Some(b'(') {
            while let Some(b) = self.advance() {
                if b == b')' {
                    break;
                }
            }
            // The native default-list skip also consumes one following byte.
            self.advance();
        } else {
            self.string(r)?;
        }
        Ok(())
    }
    fn indices(&mut self, r: &Reader<'_>, max: usize) -> Result<Vec<i32>> {
        self.advance(); // Native accepts non-parenthesis delimiters with a warning.
        let n = self.integer(r)?;
        let n = r.count(n, max)?;
        let values = (0..n)
            .map(|_| self.integer(r))
            .collect::<Result<Vec<_>>>()?;
        self.advance();
        Ok(values)
    }
}

struct Bindings<'a> {
    atoms: &'a HashMap<i32, usize>,
    bonds: &'a HashMap<i32, usize>,
}
impl Bindings<'_> {
    fn atom(&self, r: &Reader<'_>, id: i32) -> Result<usize> {
        self.atoms
            .get(&id)
            .copied()
            .ok_or_else(|| r.invalid("Missing group atom bookmark"))
    }
    fn bond(&self, r: &Reader<'_>, id: i32) -> Result<usize> {
        self.bonds
            .get(&id)
            .copied()
            .ok_or_else(|| r.invalid("Missing group bond bookmark"))
    }
}

fn property(
    r: &Reader<'_>,
    p: &Parsed,
    g: &mut Group,
    b: &Bindings<'_>,
    key: &str,
    f: &mut Fields<'_>,
) -> Result<()> {
    match key {
        "ATOMS" | "PATOMS" => {
            for id in f.indices(r, p.graph.atoms.len())? {
                let atom = b.atom(r, id)?;
                if key == "ATOMS" {
                    g.atom(atom);
                } else {
                    g.parent(r, atom)?;
                }
            }
        }
        "CBONDS" | "XBONDS" => {
            for id in f.indices(r, p.graph.bonds.len())? {
                g.bond(b.bond(r, id)?);
            }
        }
        "XBHEAD" | "XBCORR" => {
            f.indices(r, p.graph.bonds.len())?;
        }
        "BRKXYZ" => {
            f.advance();
            if f.integer(r)? != 9 {
                return Err(r.invalid("Invalid bracket coordinate count"));
            }
            for _ in 0..9 {
                f.float(r)?;
            }
            f.advance();
        }
        "CSTATE" => {
            if f.advance() != Some(b'(') {
                return Err(r.invalid("Missing bond-vector parenthesis"));
            }
            let count = f.integer(r)?;
            let bond = b.bond(r, f.integer(r)?)?;
            if count != if g.kind == "SUP" { 4 } else { 1 } {
                return Err(r.invalid("Invalid bond-vector field count"));
            }
            if g.kind == "SUP" {
                for _ in 0..3 {
                    f.float(r)?;
                }
            }
            g.crossing(r, p, bond)?;
            f.advance();
        }
        "SAP" => {
            if f.advance() != Some(b'(') {
                return Err(r.invalid("Missing attachment parenthesis"));
            }
            f.integer(r)?; // Native parses, but does not constrain, this count.
            b.atom(r, f.integer(r)?)?;
            let leaving = f.word(r)?;
            if !leaving.eq_ignore_ascii_case("AIDX") {
                let n = r.integer(leaving)?;
                if n != 0 {
                    b.atom(r, n)?;
                }
            }
            if !f.word(r)?.ends_with(')') {
                return Err(r.invalid("Missing attachment label or closing parenthesis"));
            }
        }
        "PARENT" => {
            f.integer(r)?;
        }
        "COMPNO" => {
            let start = f.pos;
            let value = match f.integer(r) {
                Ok(value) => value,
                Err(_)
                    if f.text
                        .get(start..f.pos)
                        .is_some_and(|v| v.trim_matches([' ', '+', '-']).is_empty()) =>
                {
                    // A failed stream extraction stores zero and stops this
                    // record; defaults are still processed separately.
                    f.pos = f.text.len();
                    0
                }
                Err(error) => return Err(error),
            };
            if !(0..=256).contains(&value) {
                return Err(r.invalid("Invalid group component count"));
            }
        }
        _ => {
            let mut value = f.string(r)?;
            match key {
                "SUBTYPE" if !["ALT", "RAN", "BLO"].contains(&value.as_str()) => {
                    return Err(r.invalid("Invalid group subtype"));
                }
                "CONNECT" if !["HH", "HT", "EU"].contains(&value.as_str()) => {
                    return Err(r.invalid("Invalid group connection type"));
                }
                "CLASS"
                    if ![
                        "AA",
                        "dAA",
                        "DNA",
                        "RNA",
                        "SUGAR",
                        "BASE",
                        "PHOSPHATE",
                        "LINKER",
                        "CHEM",
                        "LGRP",
                        "MODAA",
                        "MODdAA",
                        "MODDNA",
                        "MODRNA",
                        "XLINKAA",
                        "XLINKdAA",
                        "XLINKDNA",
                        "XLINKRNA",
                    ]
                    .contains(&value.as_str()) =>
                {
                    return Err(r.invalid("Invalid group template class"));
                }
                "FIELDDATA" => {
                    value.truncate(value.floor_char_boundary(200.min(value.len())));
                    g.data.push(value);
                }
                "FIELDNAME" => g.field = Some(value),
                "QUERYTYPE" => g.query = Some(value),
                "QUERYOP" => g.operator = Some(value),
                _ => (),
            }
        }
    }
    Ok(())
}

pub(in crate::chemistry::molfile::read) fn read(
    r: &mut Reader<'_>,
    p: &mut Parsed,
    count: usize,
    atoms: &HashMap<i32, usize>,
    bonds: &HashMap<i32, usize>,
) -> Result<()> {
    let mut text = r.v3()?;
    let defaults = if text.starts_with("DEFAULT") && text.len() > 8 {
        let defaults = text.get(7..).ok_or(ReadError::Limit)?.trim_end().to_owned();
        text = r.v3()?;
        defaults
    } else {
        String::new()
    };
    let bindings = Bindings { atoms, bonds };
    let mut entries = BTreeMap::new();
    let mut work = 0usize;
    for _ in 0..count {
        work = work
            .saturating_add(text.len())
            .saturating_add(defaults.len());
        if work > 16 * 1024 * 1024 {
            return Err(ReadError::Limit);
        }
        let mut fields = Fields {
            text: &text,
            pos: 0,
        };
        let id = fields.integer(r)?;
        let kind = fields.word(r)?;
        let mut group = Group::new(r, kind)?;
        fields.integer(r)?; // External ID is presentation-only in this parser.
        let mut labels = HashSet::new();
        while let Some(key) = fields.label(r)? {
            property(r, p, &mut group, &bindings, key, &mut fields)?;
            labels.insert(key.to_owned());
        }
        let mut fields = Fields {
            text: &defaults,
            pos: 0,
        };
        while let Some(key) = fields.label(r)? {
            if labels.contains(key) {
                fields.skip(r)?;
            } else {
                property(r, p, &mut group, &bindings, key, &mut fields)?;
            }
        }
        if entries.insert(id, group).is_some() {
            return Err(r.invalid("Duplicate substance-group ID"));
        }
        text = r.v3()?;
    }
    if !text.to_ascii_uppercase().starts_with("END SGROUP") {
        return Err(r.invalid("Missing END SGROUP"));
    }
    p.groups.entries = entries;
    Ok(())
}
