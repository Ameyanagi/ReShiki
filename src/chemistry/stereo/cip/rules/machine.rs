//! Explicit continuations for native recursive comparison and insertion sort.
//! No Rust call-stack depth depends on molecular depth.
use super::{Context, Error, Rules, Sorted, invalid, scalar};

#[derive(Clone, Copy)]
pub(super) enum Scope {
    Composite,
    Prefix(usize),
}
pub(super) enum Value {
    Number(i8),
    Sorted(Sorted),
    Groups(Vec<Vec<usize>>),
}
impl Value {
    fn storage(&self) -> usize {
        match self {
            Self::Number(_) => 0,
            Self::Sorted(s) => s.edges.capacity(),
            Self::Groups(g) => group_storage(g),
        }
    }
}
fn group_storage(groups: &Vec<Vec<usize>>) -> usize {
    groups.capacity() * 3 + groups.iter().map(Vec::capacity).sum::<usize>()
}
fn number(value: Option<Value>) -> Result<i8, Error> {
    match value {
        Some(Value::Number(n)) => Ok(n),
        _ => Err(invalid("Missing comparison result")),
    }
}
fn sorted(value: Option<Value>) -> Result<Vec<usize>, Error> {
    match value {
        Some(Value::Sorted(s)) => Ok(s.edges),
        _ => Err(invalid("Missing sorted edges")),
    }
}
fn at<T: Copy>(values: &[T], index: usize) -> Result<T, Error> {
    values
        .get(index)
        .copied()
        .ok_or_else(|| invalid("Invalid CIP work index"))
}

pub(super) enum Frame {
    Direct {
        rule: usize,
        a: usize,
        b: usize,
    },
    Composite {
        a: usize,
        b: usize,
        index: usize,
        waiting: bool,
    },
    Substituents {
        node: usize,
        a: usize,
        b: usize,
        scope: Scope,
        deep: bool,
        index: usize,
        waiting: bool,
    },
    Sort(Sorting),
    Groups(Grouping),
    Sequence(Sequence),
}
pub(super) struct Sorting {
    node: usize,
    edges: Vec<usize>,
    scope: Scope,
    deep: bool,
    i: usize,
    j: usize,
    unique: bool,
    pseudo: usize,
    waiting: bool,
}
pub(super) struct Grouping {
    edges: Vec<usize>,
    groups: Vec<Vec<usize>>,
    contents: usize,
    scope: Scope,
    index: usize,
    waiting: bool,
}
#[derive(Clone, Copy)]
enum Phase {
    Start,
    Initial,
    Load,
    SortA(bool),
    SortB(bool),
    Pair(bool),
    Compared(bool),
}
pub(super) struct Sequence {
    rule: usize,
    queue: Vec<(usize, usize)>,
    position: usize,
    phase: Phase,
    an: usize,
    bn: usize,
    a: Vec<usize>,
    b: Vec<usize>,
    index: usize,
}
enum Step {
    Done(Value),
    Again(Frame),
    Call(Frame, Frame),
}
impl Frame {
    pub(super) fn direct(rule: usize, a: usize, b: usize) -> Self {
        Self::Direct { rule, a, b }
    }
    pub(super) fn composite(a: usize, b: usize) -> Self {
        Self::Composite {
            a,
            b,
            index: 0,
            waiting: false,
        }
    }
    pub(super) fn sequence(rule: usize, a: usize, b: usize) -> Self {
        Self::Sequence(Sequence {
            rule,
            queue: vec![(a, b)],
            position: 0,
            phase: Phase::Start,
            an: 0,
            bn: 0,
            a: Vec::new(),
            b: Vec::new(),
            index: 0,
        })
    }
    pub(super) fn sort(node: usize, edges: Vec<usize>, scope: Scope, deep: bool) -> Self {
        Self::Sort(Sorting {
            node,
            edges,
            scope,
            deep,
            i: 1,
            j: 1,
            unique: true,
            pseudo: 0,
            waiting: false,
        })
    }
    pub(super) fn groups(edges: Vec<usize>, scope: Scope) -> Self {
        Self::Groups(Grouping {
            edges,
            scope,
            groups: Vec::new(),
            contents: 0,
            index: 0,
            waiting: false,
        })
    }
    fn substituents(node: usize, a: usize, b: usize, scope: Scope, deep: bool) -> Self {
        Self::Substituents {
            node,
            a,
            b,
            scope,
            deep,
            index: 0,
            waiting: false,
        }
    }
    fn storage(&self) -> usize {
        match self {
            Self::Sort(s) => s.edges.capacity(),
            // Count retained capacity without rescanning every earlier group.
            Self::Groups(g) => g.edges.capacity() + g.groups.capacity() * 3 + g.contents,
            Self::Sequence(s) => s.queue.capacity() * 2 + s.a.capacity() + s.b.capacity(),
            _ => 0,
        }
    }
    fn step(
        self,
        rules: &Rules,
        ctx: &mut Context<'_, '_>,
        input: Option<Value>,
    ) -> Result<Step, Error> {
        Ok(match self {
            Self::Direct { rule, a, b } => Step::Done(Value::Number(scalar::compare(
                ctx.graph,
                at(&rules.rules, rule)?,
                a,
                b,
            )?)),
            Self::Composite {
                a,
                b,
                mut index,
                waiting,
            } => {
                if waiting {
                    let cmp = number(input)?;
                    if cmp != 0 {
                        return Ok(Step::Done(Value::Number(cmp)));
                    }
                    index += 1;
                }
                if index == rules.rules.len() {
                    Step::Done(Value::Number(0))
                } else {
                    Step::Call(
                        Self::Composite {
                            a,
                            b,
                            index,
                            waiting: true,
                        },
                        Self::sequence(index, a, b),
                    )
                }
            }
            Self::Substituents {
                node,
                a,
                b,
                scope,
                deep,
                mut index,
                waiting,
            } => {
                if waiting {
                    let cmp = number(input)?;
                    if cmp != 0 {
                        return Ok(Step::Done(Value::Number(cmp)));
                    }
                    index += 1;
                } else {
                    let (ab, bb) = (
                        ctx.graph.edge(a)?.begin == node,
                        ctx.graph.edge(b)?.begin == node,
                    );
                    if ab != bb {
                        return Ok(Step::Done(Value::Number(if bb { 1 } else { -1 })));
                    }
                }
                let count = match scope {
                    Scope::Composite => 1,
                    Scope::Prefix(n) => n,
                };
                if index == count {
                    Step::Done(Value::Number(0))
                } else {
                    let child = match scope {
                        Scope::Composite => Self::composite(a, b),
                        Scope::Prefix(_) => {
                            if deep {
                                Self::sequence(index, a, b)
                            } else {
                                Self::direct(index, a, b)
                            }
                        }
                    };
                    Step::Call(
                        Self::Substituents {
                            node,
                            a,
                            b,
                            scope,
                            deep,
                            index,
                            waiting: true,
                        },
                        child,
                    )
                }
            }
            Self::Sort(mut s) => {
                if s.waiting {
                    let cmp = number(input)?;
                    if cmp.abs() > 1 {
                        s.pseudo += 1;
                    }
                    if cmp < 0 {
                        s.edges
                            .get_mut(s.j.saturating_sub(1)..=s.j)
                            .ok_or_else(|| invalid("Invalid CIP sort position"))?
                            .reverse();
                        s.j = s.j.saturating_sub(1);
                    } else {
                        if cmp == 0 {
                            s.unique = false;
                        }
                        s.j = 0;
                    }
                }
                if s.j == 0 {
                    s.i += 1;
                    s.j = s.i;
                }
                if s.i >= s.edges.len() {
                    Step::Done(Value::Sorted(Sorted {
                        edges: s.edges,
                        unique: s.unique,
                        pseudo: s.pseudo == 1,
                    }))
                } else {
                    let child = Self::substituents(
                        s.node,
                        at(&s.edges, s.j - 1)?,
                        at(&s.edges, s.j)?,
                        s.scope,
                        s.deep,
                    );
                    s.waiting = true;
                    Step::Call(Self::Sort(s), child)
                }
            }
            Self::Groups(mut g) => {
                if g.waiting || g.index == 0 {
                    if g.index >= g.edges.len() {
                        return Ok(Step::Done(Value::Groups(g.groups)));
                    }
                    if g.index == 0 || number(input)? != 0 {
                        g.groups.push(Vec::new());
                    }
                    let group = g
                        .groups
                        .last_mut()
                        .ok_or_else(|| invalid("Missing CIP group"))?;
                    let capacity = group.capacity();
                    group.push(at(&g.edges, g.index)?);
                    g.contents += group.capacity() - capacity;
                    g.index += 1;
                }
                if g.index == g.edges.len() {
                    Step::Done(Value::Groups(g.groups))
                } else {
                    let prev = at(&g.edges, g.index - 1)?;
                    let child = Self::substituents(
                        ctx.graph.edge(prev)?.begin,
                        prev,
                        at(&g.edges, g.index)?,
                        g.scope,
                        true,
                    );
                    g.waiting = true;
                    Step::Call(Self::Groups(g), child)
                }
            }
            Self::Sequence(s) => return s.step(ctx, input),
        })
    }
}
impl Sequence {
    fn step(mut self, ctx: &mut Context<'_, '_>, input: Option<Value>) -> Result<Step, Error> {
        match self.phase {
            Phase::Start => {
                ctx.iterations.consume()?;
                let (a, b) = at(&self.queue, 0)?;
                self.phase = Phase::Initial;
                let child = Frame::direct(self.rule, a, b);
                return Ok(Step::Call(Frame::Sequence(self), child));
            }
            Phase::Initial => {
                let cmp = number(input)?;
                if cmp != 0 {
                    return Ok(Step::Done(Value::Number(cmp)));
                }
                self.phase = Phase::Load;
            }
            Phase::Load => {
                if self.position == self.queue.len() {
                    return Ok(Step::Done(Value::Number(0)));
                }
                let (a, b) = at(&self.queue, self.position)?;
                self.an = ctx.graph.edge(a)?.end;
                self.bn = ctx.graph.edge(b)?.end;
                self.a = ctx.graph.edges(ctx.mol, self.an)?.to_vec();
                self.b = ctx.graph.edges(ctx.mol, self.bn)?.to_vec();
                return Ok(self.sort_a(false));
            }
            Phase::SortA(deep) => {
                self.a = sorted(input)?;
                self.phase = Phase::SortB(deep);
                let child = Frame::sort(
                    self.bn,
                    std::mem::take(&mut self.b),
                    Scope::Prefix(self.rule + 1),
                    deep,
                );
                return Ok(Step::Call(Frame::Sequence(self), child));
            }
            Phase::SortB(deep) => {
                self.b = sorted(input)?;
                self.index = 0;
                self.phase = Phase::Pair(deep);
            }
            Phase::Pair(deep) => {
                if self.index >= self.a.len().min(self.b.len()) {
                    if !deep {
                        let cmp = scalar::order(self.a.len(), self.b.len());
                        if cmp != 0 {
                            return Ok(Step::Done(Value::Number(cmp)));
                        }
                        return Ok(self.sort_a(true));
                    }
                    self.position += 1;
                    self.phase = Phase::Load;
                } else {
                    let (a, b) = (at(&self.a, self.index)?, at(&self.b, self.index)?);
                    if ctx.graph.edge(a)?.end == self.an {
                        if ctx.graph.edge(b)?.end != self.bn {
                            return Err(invalid("Mismatched CIP parent edges"));
                        }
                        self.index += 1;
                    } else {
                        self.phase = Phase::Compared(deep);
                        let child = Frame::direct(self.rule, a, b);
                        return Ok(Step::Call(Frame::Sequence(self), child));
                    }
                }
            }
            Phase::Compared(deep) => {
                let cmp = number(input)?;
                if cmp != 0 {
                    return Ok(Step::Done(Value::Number(cmp)));
                }
                if deep {
                    self.queue
                        .push((at(&self.a, self.index)?, at(&self.b, self.index)?));
                }
                self.index += 1;
                self.phase = Phase::Pair(deep);
            }
        }
        Ok(Step::Again(Frame::Sequence(self)))
    }
    fn sort_a(mut self, deep: bool) -> Step {
        self.phase = Phase::SortA(deep);
        let child = Frame::sort(
            self.an,
            std::mem::take(&mut self.a),
            Scope::Prefix(self.rule + 1),
            deep,
        );
        Step::Call(Frame::Sequence(self), child)
    }
}

pub(super) struct Machine {
    stack: Vec<Frame>,
    storage: usize,
}
impl Machine {
    fn push(&mut self, frame: Frame) -> Result<(), Error> {
        self.storage = self
            .storage
            .checked_add(frame.storage())
            .ok_or(Error::Limit)?;
        if self.storage > 8_000_000 || self.stack.len() >= 100_000 {
            return Err(Error::Limit);
        }
        self.stack.push(frame);
        Ok(())
    }
    pub(super) fn run(
        rules: &Rules,
        ctx: &mut Context<'_, '_>,
        frame: Frame,
    ) -> Result<Value, Error> {
        ctx.graph.check(ctx.mol)?;
        let mut machine = Self {
            stack: Vec::new(),
            storage: 0,
        };
        machine.push(frame)?;
        let mut value = None;
        for _ in 0..50_000_000 {
            let Some(frame) = machine.stack.pop() else {
                return value.ok_or_else(|| invalid("Missing CIP machine result"));
            };
            machine.storage = machine
                .storage
                .checked_sub(frame.storage())
                .ok_or_else(|| invalid("Invalid CIP work storage"))?;
            match frame.step(rules, ctx, value.take())? {
                Step::Done(result) => {
                    if machine
                        .storage
                        .checked_add(result.storage())
                        .ok_or(Error::Limit)?
                        > 8_000_000
                    {
                        return Err(Error::Limit);
                    }
                    value = Some(result);
                }
                Step::Again(frame) => machine.push(frame)?,
                Step::Call(parent, child) => {
                    machine.push(parent)?;
                    machine.push(child)?;
                }
            }
        }
        Err(Error::Limit)
    }
}
