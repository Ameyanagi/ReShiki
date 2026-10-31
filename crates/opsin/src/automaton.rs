//! Checked native tables of upstream dk.brics automata; no Java serialization.
use crate::InitializationError;
use std::collections::BTreeMap;

static TABLES: &[u8] = include_bytes!("../resources/automata.bin");
/// A transition's low and high symbols and target state, as big-endian u32s.
const TRANSITION: usize = 12;

/// Steps through its checked table in the embedded resource, which stays in
/// memory anyway, rather than through a decoded copy of it.
#[derive(Debug, Clone)]
pub(crate) struct Automaton {
    pub initial: usize,
    pub symbols: Vec<u16>,
    /// Where each state starts in `TABLES`: its acceptance flag, its
    /// transition count, then its transitions in ascending symbol order.
    states: Vec<u32>,
}

#[derive(Debug)]
pub(crate) struct LiveTransitions {
    row_start: Vec<usize>,
    transitions: Vec<(usize, usize)>,
}

impl LiveTransitions {
    pub fn row(&self, state: usize) -> &[(usize, usize)] {
        &self.transitions[self.row_start[state]..self.row_start[state + 1]]
    }
}

/// A big-endian u32 of `TABLES` that `load` checked.
fn word(at: usize) -> u32 {
    u32::from_be_bytes(TABLES[at..at + 4].try_into().expect("four bytes"))
}

impl Automaton {
    pub fn live_transitions(&self) -> LiveTransitions {
        let mut row_start = Vec::with_capacity(self.states.len() + 1);
        let mut transitions = Vec::new();
        for state in 0..self.states.len() {
            row_start.push(transitions.len());
            for (symbol_index, &symbol) in self.symbols.iter().enumerate() {
                if let Some(next_state) = self.step(state, symbol) {
                    transitions.push((symbol_index, next_state));
                }
            }
        }
        row_start.push(transitions.len());
        LiveTransitions {
            row_start,
            transitions,
        }
    }

    pub fn step(&self, state: usize, symbol: u16) -> Option<usize> {
        let at = *self.states.get(state)? as usize;
        let first = at + 5;
        let symbol = u32::from(symbol);
        // The first transition whose low symbol is above `symbol`.
        let (mut low, mut high) = (0, word(at + 1) as usize);
        while low < high {
            let middle = (low + high) / 2;
            if word(first + middle * TRANSITION) <= symbol {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let transition = first + low.checked_sub(1)? * TRANSITION;
        (symbol <= word(transition + 4)).then(|| word(transition + 8) as usize)
    }

    pub fn is_accept(&self, state: usize) -> bool {
        TABLES[self.states[state] as usize] == 1
    }

    /// Matches the longest accepted prefix, as `RunAutomaton.run` does.
    pub fn run(&self, input: &[u8], offset: usize) -> Option<usize> {
        let mut state = self.initial;
        let mut longest = self.is_accept(state).then_some(0);
        for (i, byte) in input[offset..].iter().enumerate() {
            let Some(next) = self.step(state, u16::from(*byte)) else {
                break;
            };
            state = next;
            if self.is_accept(state) {
                longest = Some(i + 1);
            }
        }
        longest
    }
}

pub(crate) fn load() -> Result<BTreeMap<String, Automaton>, InitializationError> {
    let mut reader = Reader(TABLES);
    if reader.take(8)? != b"OPSINDFA" || reader.number()? != 2 {
        return Err(InitializationError(
            "Unrecognised OPSIN DFA resource format".into(),
        ));
    }
    let count = reader.number()?;
    if count > 256 {
        return Err(InitializationError("Invalid OPSIN DFA count".into()));
    }
    let mut automata = BTreeMap::new();
    for _ in 0..count {
        let length = reader.number()?;
        if length > 128 {
            return Err(InitializationError("Invalid DFA name length".into()));
        }
        let name = std::str::from_utf8(reader.take(length)?)
            .map_err(|e| InitializationError(e.to_string()))?
            .to_owned();
        let initial = reader.number()?;
        let size = reader.number()?;
        let symbol_count = reader.number()?;
        if size == 0 || size > 100_000 || initial >= size || symbol_count > 65_536 {
            return Err(InitializationError("Invalid OPSIN DFA dimensions".into()));
        }
        let mut symbols = Vec::with_capacity(symbol_count);
        for _ in 0..symbol_count {
            symbols.push(reader.symbol()?);
        }
        if symbols.windows(2).any(|w| w[0] >= w[1]) {
            return Err(InitializationError("Unordered OPSIN DFA symbols".into()));
        }
        let mut states = Vec::with_capacity(size);
        for _ in 0..size {
            states.push(
                u32::try_from(TABLES.len() - reader.0.len())
                    .map_err(|_| InitializationError("Oversized OPSIN DFA resource".into()))?,
            );
            if reader.take(1)?[0] > 1 {
                return Err(InitializationError("Invalid DFA acceptance flag".into()));
            }
            let transition_count = reader.number()?;
            if transition_count > 65_536 {
                return Err(InitializationError("Invalid DFA transition count".into()));
            }
            let mut previous_high = None;
            for _ in 0..transition_count {
                let low = reader.symbol()?;
                let high = reader.symbol()?;
                let target = reader.number()?;
                if low > high || target >= size {
                    return Err(InitializationError("Invalid DFA transition".into()));
                }
                if previous_high.is_some_and(|previous| previous >= low) {
                    return Err(InitializationError(
                        "Overlapping DFA transition intervals".into(),
                    ));
                }
                previous_high = Some(high);
            }
        }
        if automata
            .insert(
                name,
                Automaton {
                    initial,
                    symbols,
                    states,
                },
            )
            .is_some()
        {
            return Err(InitializationError("Duplicate OPSIN automaton name".into()));
        }
    }
    if !reader.0.is_empty() {
        return Err(InitializationError("Trailing OPSIN DFA bytes".into()));
    }
    Ok(automata)
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], InitializationError> {
        let Some((value, remaining)) = self.0.split_at_checked(length) else {
            return Err(InitializationError("Truncated OPSIN DFA resource".into()));
        };
        self.0 = remaining;
        Ok(value)
    }
    fn number(&mut self) -> Result<usize, InitializationError> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("four bytes");
        Ok(u32::from_be_bytes(bytes) as usize)
    }
    fn symbol(&mut self) -> Result<u16, InitializationError> {
        u16::try_from(self.number()?)
            .map_err(|_| InitializationError("Invalid UTF-16 DFA symbol".into()))
    }
}
