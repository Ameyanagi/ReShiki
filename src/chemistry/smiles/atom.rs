use super::{Atom, AtomMetadata, Error, Reader, Result};
use crate::chemistry::ELEMENTS;

impl Reader<'_> {
    pub(super) fn atom(&mut self) -> Result<(Atom, AtomMetadata, Option<String>)> {
        let bracket = self.eat(b'[');
        let isotope = if bracket && self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.number()? as u16
        } else {
            0
        };
        let hydrogen;
        let (number, aromatic, dummy) = if bracket && self.eat(b'#') {
            let n = self.number()?;
            // The native atom stores this token in an unsigned byte. Chemical
            // validation subsequently rejects values outside the element table.
            hydrogen = false;
            (n as u8, false, false)
        } else {
            let start = self.pos;
            let (number, aromatic, len) = self.element(bracket).ok_or_else(|| self.invalid())?;
            hydrogen = self.bytes.get(start..start + len) == Some(b"H".as_slice());
            let dummy = number == 0;
            self.pos += len;
            (number, aromatic, dummy)
        };
        let mut atom = Atom {
            atomic_number: number,
            isotope,
            aromatic,
            no_implicit: bracket,
            ..Atom::default()
        };
        let mut meta = AtomMetadata::default();
        if bracket {
            if !hydrogen && self.eat(b'@') {
                if self.eat(b'@') {
                    meta.chiral_tag = 1;
                } else {
                    let mut end = self.pos;
                    while matches!(self.bytes.get(end), Some(b' ' | b'\'')) {
                        end += 1;
                    }
                    let rest = self.bytes.get(end..).ok_or(Error::Limit)?;
                    if let Some((class, limit)) =
                        [(b"TH", 2), (b"AL", 2), (b"SP", 3), (b"TB", 20), (b"OH", 30)]
                            .into_iter()
                            .find(|(c, _)| rest.starts_with(*c))
                    {
                        self.pos = end + 2;
                        let explicit = self.peek().is_some_and(|b| b.is_ascii_digit());
                        let permutation = if explicit { self.number()? } else { 0 };
                        if permutation > limit || explicit && permutation == 0 {
                            return Err(self.invalid());
                        }
                        meta.chiral_tag = match class {
                            b"TH" => {
                                if permutation == 2 {
                                    1
                                } else {
                                    2
                                }
                            }
                            b"AL" => 5,
                            b"SP" => 6,
                            b"TB" => 7,
                            b"OH" => 8,
                            _ => return Err(self.invalid()),
                        };
                        if class != b"TH" {
                            meta.chiral_permutation = Some(permutation);
                        }
                    } else {
                        meta.chiral_tag = 2;
                    }
                }
            }
            if self.eat(b'H') {
                atom.explicit_hydrogens = if self.peek().is_some_and(|b| b.is_ascii_digit()) {
                    self.number()? as u8
                } else {
                    1
                };
            }
            if let Some(sign @ (b'+' | b'-')) = self.peek() {
                self.pos += 1;
                let n = if self.eat(sign) {
                    2
                } else if self.peek().is_some_and(|b| b.is_ascii_digit()) {
                    self.number()? as i32
                } else {
                    1
                };
                atom.charge = (if sign == b'-' { -n } else { n }) as i8;
            }
            if self.eat(b':') {
                meta.map_number = self.number()? as i32;
                meta.map_present = true;
            }
            self.require(b']')?;
        }
        Ok((atom, meta, dummy.then(|| "*".to_owned())))
    }
    fn element(&self, bracket: bool) -> Option<(u8, bool, usize)> {
        let rest = self.bytes.get(self.pos..)?;
        if bracket {
            for (i, name) in [
                "Uun", "Uuu", "Uub", "Uut", "Uuq", "Uup", "Uuh", "Uus", "Uuo",
            ]
            .iter()
            .enumerate()
            {
                if rest.starts_with(name.as_bytes()) {
                    return Some((110 + i as u8, false, 3));
                }
            }
            if rest.first() == Some(&b'\'') {
                for (i, element) in ELEMENTS.iter().enumerate().skip(104) {
                    if rest
                        .get(1..)
                        .is_some_and(|s| s.starts_with(element.symbol.as_bytes()))
                        && rest.get(element.symbol.len() + 1) == Some(&b'\'')
                    {
                        return Some((i as u8, false, element.symbol.len() + 2));
                    }
                }
            }
            for (symbol, n) in [("si", 14), ("as", 33), ("se", 34), ("te", 52)] {
                if rest.starts_with(symbol.as_bytes()) {
                    return Some((n, true, 2));
                }
            }
            if let Some((n, symbol)) = ELEMENTS
                .iter()
                .enumerate()
                .filter(|(_, e)| rest.starts_with(e.symbol.as_bytes()))
                .max_by_key(|(_, e)| e.symbol.len())
            {
                return Some((n as u8, false, symbol.symbol.len()));
            }
        }
        for (symbol, n) in [("Cl", 17), ("Br", 35)] {
            if rest.starts_with(symbol.as_bytes()) {
                return Some((n, false, 2));
            }
        }
        let byte = *rest.first()?;
        let n = match byte {
            b'*' => 0,
            b'B' | b'b' => 5,
            b'C' | b'c' => 6,
            b'N' | b'n' => 7,
            b'O' | b'o' => 8,
            b'F' => 9,
            b'P' | b'p' => 15,
            b'S' | b's' => 16,
            b'I' => 53,
            _ => return None,
        };
        Some((n, byte.is_ascii_lowercase(), 1))
    }
}
