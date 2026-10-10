//! Checked native tables of upstream dk.brics automata; no Java serialization.
use crate::InitializationError;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(crate) struct Automaton {
    pub initial: usize,
    pub symbols: Vec<u16>,
    states: Vec<State>,
}

#[derive(Debug, Clone)]
struct State {
    accept: bool,
    transitions: Vec<Transition>,
}

#[derive(Debug, Clone)]
struct Transition {
    low: u16,
    high: u16,
    target: usize,
}

impl Automaton {
    pub fn step(&self, state: usize, symbol: u16) -> Option<usize> {
        let transitions = &self.states.get(state)?.transitions;
        let index = transitions
            .partition_point(|t| t.low <= symbol)
            .checked_sub(1)?;
        let transition = &transitions[index];
        (symbol <= transition.high).then_some(transition.target)
    }

    pub fn is_accept(&self, state: usize) -> bool {
        self.states[state].accept
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
    let mut reader = Reader(include_bytes!("../resources/automata.bin"));
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
            let accept = match reader.take(1)?[0] {
                0 => false,
                1 => true,
                _ => return Err(InitializationError("Invalid DFA acceptance flag".into())),
            };
            let transition_count = reader.number()?;
            if transition_count > 65_536 {
                return Err(InitializationError("Invalid DFA transition count".into()));
            }
            let mut transitions = Vec::with_capacity(transition_count);
            for _ in 0..transition_count {
                let low = reader.symbol()?;
                let high = reader.symbol()?;
                let target = reader.number()?;
                if low > high || target >= size {
                    return Err(InitializationError("Invalid DFA transition".into()));
                }
                transitions.push(Transition { low, high, target });
            }
            if transitions.windows(2).any(|w| w[0].high >= w[1].low) {
                return Err(InitializationError(
                    "Overlapping DFA transition intervals".into(),
                ));
            }
            states.push(State {
                accept,
                transitions,
            });
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
