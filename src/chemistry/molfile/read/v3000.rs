use super::*;
use std::collections::{HashMap, HashSet};

impl Reader<'_> {
    fn v3(&mut self) -> Result<String> {
        let mut output = String::new();
        loop {
            let line = self.next()?;
            let content = line
                .strip_prefix("M  V30 ")
                .ok_or_else(|| self.invalid("Missing V3000 line prefix"))?;
            let continuation = content.strip_suffix('-');
            output.push_str(continuation.unwrap_or(content));
            if output.len() > 1024 * 1024 {
                return Err(ReadError::Limit);
            }
            if continuation.is_none() {
                return Ok(output);
            }
        }
    }
    fn expect(&mut self, prefix: &str) -> Result<()> {
        if self.v3()?.to_ascii_uppercase().starts_with(prefix) {
            Ok(())
        } else {
            Err(self.invalid(format!("Missing {prefix}")))
        }
    }
    fn tokens<'a>(&self, line: &'a str) -> Result<Vec<&'a str>> {
        let (mut start, mut depth, mut quoted) = (0usize, 0usize, false);
        let mut tokens = Vec::new();
        for (i, c) in line.char_indices() {
            match c {
                '"' if depth == 0 => quoted = !quoted,
                '(' if !quoted => {
                    depth += 1;
                    if depth > 64 {
                        return Err(ReadError::Limit);
                    }
                }
                ')' if !quoted => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| self.invalid("Unbalanced property list"))?;
                }
                ' ' | '\t' if !quoted && depth == 0 => {
                    if i > start {
                        tokens.push(self.field(line, start, i)?.trim_matches('"'));
                    }
                    start = i + 1;
                }
                _ => (),
            }
        }
        if quoted || depth != 0 {
            return Err(self.invalid("Unclosed quoted field or property list"));
        }
        if start < line.len() {
            tokens.push(self.field(line, start, line.len())?.trim_matches('"'));
        }
        Ok(tokens)
    }
    fn token<'a>(&self, tokens: &[&'a str], i: usize) -> Result<&'a str> {
        tokens
            .get(i)
            .copied()
            .ok_or_else(|| self.invalid("Missing V3000 field"))
    }
    fn assignment<'a>(&self, token: &'a str) -> Result<(String, &'a str)> {
        let (key, value) = token
            .split_once('=')
            .filter(|(_, v)| !v.contains('='))
            .ok_or_else(|| self.invalid("Invalid property assignment"))?;
        Ok((key.to_ascii_uppercase(), value))
    }
}

pub(super) fn read(r: &mut Reader<'_>, p: &mut Parsed) -> Result<()> {
    r.expect("BEGIN CTAB")?;
    let line = r.v3()?;
    let tokens = r.tokens(&line)?;
    if !r.token(&tokens, 0)?.eq_ignore_ascii_case("COUNTS") {
        return Err(r.invalid("Missing V3000 counts"));
    }
    let n = r.count(r.integer(r.token(&tokens, 1)?)?, 100_000)?;
    let e = r.count(r.integer(r.token(&tokens, 2)?)?, 300_000)?;
    let groups = tokens
        .get(3)
        .map(|s| r.integer(s))
        .transpose()?
        .unwrap_or(0);
    let objects = tokens
        .get(4)
        .map(|s| r.integer(s))
        .transpose()?
        .unwrap_or(0);
    if groups != 0 {
        return Err(ReadError::Pending("substance groups"));
    }
    if objects != 0 {
        return Err(ReadError::Pending("3D constraint records"));
    }
    let mut indices = HashMap::new();
    if n != 0 {
        r.expect("BEGIN ATOM")?;
        for _ in 0..n {
            let line = r.v3()?;
            let tokens = r.tokens(&line)?;
            let id = r.integer(r.token(&tokens, 0)?)?;
            if indices.insert(id, p.graph.atoms.len()).is_some() {
                return Err(r.invalid("Duplicate atom ID"));
            }
            let mut atom = r.symbol(r.token(&tokens, 1)?, true)?;
            let position = Point3 {
                x: r.coordinate(r.token(&tokens, 2)?, false)?,
                y: r.coordinate(r.token(&tokens, 3)?, false)?,
                z: r.coordinate(r.token(&tokens, 4)?, false)?,
            };
            let map = r.integer(r.token(&tokens, 5)?)?.max(0);
            let mut props = FileAtom::default();
            for &token in tokens.iter().skip(6) {
                let (key, value) = r.assignment(token)?;
                match key.as_str() {
                    "CHG" => {
                        atom.charge = i8::try_from(r.integer(value)?)
                            .map_err(|_| r.invalid("Excessive charge"))?
                    }
                    "RAD" => {
                        let code = r.integer(value)?;
                        if code != 0 {
                            atom.radical_electrons = radical(code)?;
                        }
                    }
                    "MASS" => {
                        let mass = r
                            .integer(value)
                            .or_else(|_| r.coordinate(value, true).map(|v| v.floor() as i32))?;
                        atom.isotope =
                            u16::try_from(mass).map_err(|_| r.invalid("Invalid isotope"))?;
                    }
                    "VAL" if value != "0" => props.valence = r.integer(value)?,
                    "CFG" if !(0..=3).contains(&r.integer(value)?) => {
                        return Err(r.invalid("Invalid atom CFG"));
                    }
                    "HCOUNT" | "RBCNT" | "SUBST" if value != "0" => {
                        return Err(ReadError::Unsupported("atom query"));
                    }
                    "UNSAT" if value == "1" => {
                        return Err(ReadError::Unsupported("unsaturation query"));
                    }
                    "RGROUPS" => {
                        let content = value
                            .strip_prefix('(')
                            .and_then(|s| s.strip_suffix(')'))
                            .ok_or_else(|| r.invalid("Invalid RGROUPS list"))?;
                        let entries = content.split_whitespace().collect::<Vec<_>>();
                        let count = r.count(r.integer(r.token(&entries, 0)?)?, 100_000)?;
                        if entries.len() < count + 1 {
                            return Err(r.invalid("Missing RGROUPS entries"));
                        }
                        if count > 0 {
                            return Err(ReadError::Unsupported("R-group query"));
                        }
                    }
                    "ATTCHPT" if value != "0" => {
                        r.integer(value)?;
                        if props.attachment {
                            return Err(r.invalid("Duplicate attachment point"));
                        }
                        props.attachment = true;
                    }
                    "ATTCHORD" if value.starts_with('(') => {
                        return Err(ReadError::Pending("template attachment order"));
                    }
                    "ATTCHORD" => {
                        r.integer(value)?;
                    }
                    "STBOX" | "EXACHG" | "INVRET" | "SEQID" => {
                        r.integer(value)?;
                    }
                    _ => (),
                }
            }
            p.atom(
                atom,
                position,
                AtomMetadata {
                    map_number: map,
                    map_present: map > 0,
                    ..AtomMetadata::default()
                },
                props,
            );
        }
        r.expect("END ATOM")?;
    }
    if e != 0 {
        r.expect("BEGIN BOND")?;
        let mut ids = HashSet::new();
        for _ in 0..e {
            let line = r.v3()?;
            let tokens = r.tokens(&line)?;
            if !ids.insert(r.integer(r.token(&tokens, 0)?)?) {
                return Err(r.invalid("Duplicate bond ID"));
            }
            let kind = order(r.integer(r.token(&tokens, 1)?)?, true)?;
            let endpoint = |i| -> Result<usize> {
                indices
                    .get(&r.integer(r.token(&tokens, i)?)?)
                    .copied()
                    .ok_or_else(|| r.invalid("Missing bond atom"))
            };
            let (a, b) = (endpoint(2)?, endpoint(3)?);
            let mut dir = Direction::None;
            for &token in tokens.iter().skip(4) {
                let (key, value) = r.assignment(token)?;
                match key.as_str() {
                    "CFG" => match r.integer(value)? {
                        0 => (),
                        1 => {
                            dir = Direction::Wedge;
                            p.chirality = true;
                        }
                        3 => {
                            dir = Direction::Hash;
                            p.chirality = true;
                        }
                        2 => {
                            if kind == 1 {
                                dir = Direction::Unknown;
                            } else if kind == 2 {
                                dir = Direction::EitherDouble;
                            }
                        }
                        _ => return Err(r.invalid("Invalid bond CFG")),
                    },
                    "TOPO" if value != "0" => {
                        return Err(ReadError::Unsupported("bond topology query"));
                    }
                    "RXCTR" => {
                        r.integer(value)?;
                    }
                    // These properties do not modify connectivity in the native MOL reader.
                    "STBOX" | "ENDPTS" | "ATTACH" => (),
                    _ => (),
                }
            }
            p.bond(a, b, kind, dir);
        }
        r.expect("END BOND")?;
    }
    let mut line = r.v3()?.to_ascii_uppercase();
    while line.starts_with("LINKNODE") {
        line = r.v3()?.to_ascii_uppercase();
    }
    while line.starts_with("BEGIN") {
        if line.starts_with("BEGIN COLLECTION") {
            return Err(ReadError::Pending("enhanced stereo collections"));
        }
        if line.starts_with("BEGIN SGROUP") {
            return Err(r.invalid("Unexpected substance group block"));
        }
        if line.starts_with("BEGIN OBJ3D") {
            return Err(r.invalid("Unexpected 3D constraint block"));
        }
        // Bound traversal by the input limit and require a real terminating record.
        while !line.starts_with("END") {
            line = r.v3()?;
        }
        line = r.v3()?.to_ascii_uppercase();
    }
    if !line.starts_with("END CTAB") {
        return Err(r.invalid("Missing END CTAB"));
    }
    if !r.next()?.starts_with("M  END") {
        return Err(r.invalid("Missing M END"));
    }
    Ok(())
}
