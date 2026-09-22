//! Descriptor-pair comparison adapted from RDKit Rule4b.cpp, Rule5New.cpp
//! and Pairlist.h. Copyright (C) 2020 Schrödinger, LLC; BSD-3-Clause.
use super::machine::{Frame, Spec, Step, Value, at, number, sorted};
use super::{Context, Error, Rule, invalid, scalar};
use crate::chemistry::stereo::cip::digraph::Descriptor;

fn reference(d: Descriptor) -> Descriptor {
    match d {
        Descriptor::R | Descriptor::M | Descriptor::SeqCis => Descriptor::R,
        Descriptor::S | Descriptor::P | Descriptor::SeqTrans => Descriptor::S,
        _ => Descriptor::None,
    }
}
pub(super) struct PairList(Vec<Descriptor>);
impl PairList {
    fn new(d: Descriptor) -> Self {
        let mut list = Self(Vec::new());
        list.add(d);
        list
    }
    fn add(&mut self, d: Descriptor) {
        let d = reference(d);
        if d != Descriptor::None {
            self.0.push(d);
        }
    }
    fn compare(&self, b: &Self) -> Result<i8, Error> {
        if self.0.len() != b.0.len() {
            return Err(invalid("CIP descriptor lists have different lengths"));
        }
        let Some((&ar, aa)) = self.0.split_first() else {
            return Ok(0);
        };
        let (&br, bb) =
            b.0.split_first()
                .ok_or_else(|| invalid("Missing CIP pair reference"))?;
        Ok(aa
            .iter()
            .zip(bb)
            .map(|(&a, &b)| scalar::order(a == ar, b == br))
            .find(|&c| c != 0)
            .unwrap_or(0))
    }
    pub fn storage(&self) -> usize {
        self.0.capacity()
    }
}
fn descriptors(v: Option<Value>) -> Result<Vec<Descriptor>, Error> {
    match v {
        Some(Value::Descriptors(d)) => Ok(d),
        _ => Err(invalid("Missing CIP references")),
    }
}
fn pairs(v: Option<Value>) -> Result<PairList, Error> {
    match v {
        Some(Value::Pairs(p)) => Ok(p),
        _ => Err(invalid("Missing CIP descriptor pairs")),
    }
}

pub(super) fn direct(
    ctx: &mut Context<'_, '_>,
    spec: Spec,
    kind: Rule,
    a: usize,
    b: usize,
) -> Result<Step, Error> {
    let (a, b) = (ctx.graph.edge(a)?, ctx.graph.edge(b)?);
    if a.begin != ctx.graph.current_root() || b.begin != ctx.graph.current_root() {
        if spec.reference == Descriptor::None {
            return Ok(Step::Done(Value::Number(0)));
        }
        let (ad, bd) = (ctx.graph.node(a.end)?.aux, ctx.graph.node(b.end)?.aux);
        let valid = |d| d != Descriptor::None && d != Descriptor::Other;
        let cmp = if valid(ad) && valid(bd) {
            scalar::order(
                reference(spec.reference) == reference(ad),
                reference(spec.reference) == reference(bd),
            )
        } else {
            0
        };
        return Ok(Step::Done(Value::Number(cmp)));
    }
    Ok(Step::Again(Frame::Pairing(Box::new(Pairing::new(
        spec, kind, a.end, b.end,
    )))))
}
#[derive(Clone, Copy)]
enum PairPhase {
    Start,
    ReferenceA,
    ReferenceB,
    NextFill,
    Filled,
    Compared,
}
pub(super) struct Pairing {
    spec: Spec,
    kind: Rule,
    a: usize,
    b: usize,
    phase: PairPhase,
    ra: Vec<Descriptor>,
    rb: Vec<Descriptor>,
    lists: Vec<PairList>,
    filled: usize,
}
impl Pairing {
    fn new(spec: Spec, kind: Rule, a: usize, b: usize) -> Self {
        Self {
            spec,
            kind,
            a,
            b,
            phase: PairPhase::Start,
            ra: Vec::new(),
            rb: Vec::new(),
            lists: Vec::new(),
            filled: 0,
        }
    }
    pub fn storage(&self) -> usize {
        self.ra.capacity()
            + self.rb.capacity()
            + self.lists.capacity() * 3
            + self.lists.iter().map(PairList::storage).sum::<usize>()
    }
    pub fn step(mut self: Box<Self>, input: Option<Value>) -> Result<Step, Error> {
        match self.phase {
            PairPhase::Start => {
                if matches!(self.kind, Rule::DescriptorPair) {
                    self.phase = PairPhase::ReferenceA;
                    let child = References::frame(self.spec, self.a);
                    return Ok(Step::Call(Frame::Pairing(self), child));
                }
                self.ra = vec![Descriptor::R, Descriptor::S];
                self.rb = self.ra.clone();
                self.phase = PairPhase::NextFill;
            }
            PairPhase::ReferenceA => {
                self.ra = descriptors(input)?;
                self.phase = PairPhase::ReferenceB;
                let child = References::frame(self.spec, self.b);
                return Ok(Step::Call(Frame::Pairing(self), child));
            }
            PairPhase::ReferenceB => {
                self.rb = descriptors(input)?;
                if self.ra.is_empty() != self.rb.is_empty() {
                    return Err(invalid("CIP substituents are not equivalent"));
                }
                if self.ra.is_empty() {
                    return Ok(Step::Done(Value::Number(0)));
                }
                if self.ra.len() == 1 {
                    self.phase = PairPhase::Compared;
                    let child = ComparePairs::frame(
                        self.spec,
                        self.a,
                        self.b,
                        at(&self.ra, 0)?,
                        at(&self.rb, 0)?,
                    );
                    return Ok(Step::Call(Frame::Pairing(self), child));
                }
                self.phase = PairPhase::NextFill;
            }
            PairPhase::Compared => return Ok(Step::Done(Value::Number(number(input)?))),
            PairPhase::Filled => {
                self.lists.push(pairs(input)?);
                self.filled += 1;
                self.phase = PairPhase::NextFill;
            }
            PairPhase::NextFill => {
                if self.filled == self.ra.len() + self.rb.len() {
                    let (a, b) = self
                        .lists
                        .split_at_mut_checked(self.ra.len())
                        .ok_or_else(|| invalid("Missing CIP reference lists"))?;
                    if matches!(self.kind, Rule::PseudoPair) {
                        let cmp_r = list(a, 0)?.compare(list(b, 0)?)?;
                        let cmp_s = list(a, 1)?.compare(list(b, 1)?)?;
                        let result = if cmp_r < 0 {
                            if cmp_s < 0 { -1 } else { -2 }
                        } else if cmp_r > 0 {
                            if cmp_s > 0 { 1 } else { 2 }
                        } else {
                            0
                        };
                        return Ok(Step::Done(Value::Number(result)));
                    }
                    descending(a)?;
                    descending(b)?;
                    for (i, pa) in a.iter().enumerate() {
                        let cmp = pa.compare(list(b, i)?)?;
                        if cmp != 0 {
                            return Ok(Step::Done(Value::Number(cmp)));
                        }
                    }
                    return Ok(Step::Done(Value::Number(0)));
                }
                let (node, d) = if self.filled < self.ra.len() {
                    (self.a, at(&self.ra, self.filled)?)
                } else {
                    (self.b, at(&self.rb, self.filled - self.ra.len())?)
                };
                self.phase = PairPhase::Filled;
                let child = Fill::frame(self.spec, node, d);
                return Ok(Step::Call(Frame::Pairing(self), child));
            }
        }
        Ok(Step::Again(Frame::Pairing(self)))
    }
}
fn list(values: &[PairList], i: usize) -> Result<&PairList, Error> {
    values
        .get(i)
        .ok_or_else(|| invalid("Missing CIP pair list"))
}
fn descending(values: &mut [PairList]) -> Result<(), Error> {
    if values.len() > 2 {
        return Err(invalid("Too many CIP pair references"));
    }
    if values.len() == 2 && list(values, 0)?.compare(list(values, 1)?)? < 0 {
        values.reverse();
    }
    Ok(())
}

/// Grouped BFS levels with constant-time retained-capacity accounting.
#[derive(Default)]
struct Groups {
    values: Vec<Vec<usize>>,
    contents: usize,
}
impl Groups {
    fn storage(&self) -> usize {
        self.values.capacity() * 3 + self.contents
    }
    fn push(&mut self, group: Vec<usize>) {
        self.contents += group.capacity();
        self.values.push(group);
    }
    fn extend(&mut self, index: usize, nodes: impl Iterator<Item = usize>) -> Result<(), Error> {
        let group = self
            .values
            .get_mut(index)
            .ok_or_else(|| invalid("Missing equivalent CIP group"))?;
        let old = group.capacity();
        group.extend(nodes);
        self.contents += group.capacity() - old;
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum RefPhase {
    Scan,
    Expand,
    Sorted,
    Grouped,
}
pub(super) struct References {
    spec: Spec,
    phase: RefPhase,
    level: Groups,
    next: Groups,
    columns: Groups,
    group: usize,
    node: usize,
    right: usize,
    left: usize,
}
impl References {
    fn frame(spec: Spec, node: usize) -> Frame {
        let mut level = Groups::default();
        level.push(vec![node]);
        Frame::References(Box::new(Self {
            spec,
            phase: RefPhase::Scan,
            level,
            next: Groups::default(),
            columns: Groups::default(),
            group: 0,
            node: 0,
            right: 0,
            left: 0,
        }))
    }
    pub fn storage(&self) -> usize {
        self.level.storage() + self.next.storage() + self.columns.storage()
    }
    pub fn step(
        mut self: Box<Self>,
        ctx: &mut Context<'_, '_>,
        input: Option<Value>,
    ) -> Result<Step, Error> {
        match self.phase {
            RefPhase::Scan => {
                if self.level.values.is_empty() {
                    return Ok(Step::Done(Value::Descriptors(Vec::new())));
                }
                let Some(group) = self.level.values.get(self.group) else {
                    self.group = 0;
                    self.node = 0;
                    self.phase = RefPhase::Expand;
                    return Ok(Step::Again(Frame::References(self)));
                };
                if let Some(&node) = group.get(self.node) {
                    match reference(ctx.graph.node(node)?.aux) {
                        Descriptor::R => self.right += 1,
                        Descriptor::S => self.left += 1,
                        _ => {}
                    }
                    self.node += 1;
                } else {
                    let result = match self.right.cmp(&self.left) {
                        std::cmp::Ordering::Greater => vec![Descriptor::R],
                        std::cmp::Ordering::Less => vec![Descriptor::S],
                        _ => {
                            if self.right != 0 {
                                vec![Descriptor::R, Descriptor::S]
                            } else {
                                Vec::new()
                            }
                        }
                    };
                    if !result.is_empty() {
                        return Ok(Step::Done(Value::Descriptors(result)));
                    }
                    self.group += 1;
                    self.node = 0;
                    self.left = 0;
                    self.right = 0;
                }
            }
            RefPhase::Expand => {
                let Some(group) = self.level.values.get(self.group) else {
                    self.level = std::mem::take(&mut self.next);
                    self.group = 0;
                    self.node = 0;
                    self.phase = RefPhase::Scan;
                    return Ok(Step::Again(Frame::References(self)));
                };
                if let Some(&node) = group.get(self.node) {
                    let edges = ctx.graph.nonterminal_out_edges(ctx.mol, node)?;
                    self.phase = RefPhase::Sorted;
                    let child = Frame::sort(node, edges, self.spec.sorter(), true);
                    return Ok(Step::Call(Frame::References(self), child));
                }
                for column in std::mem::take(&mut self.columns).values {
                    if !column.is_empty() {
                        self.next.push(column);
                    }
                }
                self.group += 1;
                self.node = 0;
            }
            RefPhase::Sorted => {
                self.phase = RefPhase::Grouped;
                let child = Frame::groups(sorted(input)?, self.spec.sorter());
                return Ok(Step::Call(Frame::References(self), child));
            }
            RefPhase::Grouped => {
                let groups = match input {
                    Some(Value::Groups(g)) => g,
                    _ => return Err(invalid("Missing reference groups")),
                };
                if self.node == 0 {
                    for _ in &groups {
                        self.columns.push(Vec::new());
                    }
                } else if self.columns.values.len() != groups.len() {
                    return Err(invalid("Different numbers of equivalent CIP groups"));
                }
                for (i, edges) in groups.into_iter().enumerate() {
                    let nodes = edges
                        .into_iter()
                        .map(|e| ctx.graph.edge(e).map(|e| e.end))
                        .collect::<Result<Vec<_>, _>>()?;
                    self.columns.extend(i, nodes.into_iter())?;
                }
                self.node += 1;
                self.phase = RefPhase::Expand;
            }
        }
        Ok(Step::Again(Frame::References(self)))
    }
}

fn enqueue(
    ctx: &Context<'_, '_>,
    node: usize,
    edges: Vec<usize>,
    queue: &mut Vec<usize>,
) -> Result<(), Error> {
    for e in edges {
        let edge = ctx.graph.edge(e)?;
        if edge.begin == node && !ctx.graph.node(edge.end)?.is_terminal() {
            queue.push(edge.end);
        }
    }
    Ok(())
}
pub(super) struct Fill {
    spec: Spec,
    reference: Descriptor,
    list: PairList,
    queue: Vec<usize>,
    pos: usize,
    waiting: bool,
}
impl Fill {
    fn frame(spec: Spec, node: usize, reference: Descriptor) -> Frame {
        Frame::Fill(Self {
            spec,
            reference,
            list: PairList::new(reference),
            queue: vec![node],
            pos: 0,
            waiting: false,
        })
    }
    pub fn storage(&self) -> usize {
        self.list.storage() + self.queue.capacity()
    }
    pub fn step(mut self, ctx: &mut Context<'_, '_>, input: Option<Value>) -> Result<Step, Error> {
        if self.waiting {
            enqueue(
                ctx,
                at(&self.queue, self.pos)?,
                sorted(input)?,
                &mut self.queue,
            )?;
            self.pos += 1;
        }
        let Some(&node) = self.queue.get(self.pos) else {
            return Ok(Step::Done(Value::Pairs(self.list)));
        };
        self.list.add(ctx.graph.node(node)?.aux);
        let edges = ctx.graph.edges(ctx.mol, node)?.to_vec();
        let child = Frame::sort(node, edges, self.spec.replacement(self.reference), true);
        self.waiting = true;
        Ok(Step::Call(Frame::Fill(self), child))
    }
}
pub(super) struct ComparePairs {
    spec: Spec,
    aq: Vec<usize>,
    bq: Vec<usize>,
    pos: usize,
    ra: Descriptor,
    rb: Descriptor,
    phase: u8,
}
impl ComparePairs {
    fn frame(spec: Spec, a: usize, b: usize, ra: Descriptor, rb: Descriptor) -> Frame {
        Frame::ComparePairs(Self {
            spec,
            aq: vec![a],
            bq: vec![b],
            pos: 0,
            ra,
            rb,
            phase: 0,
        })
    }
    pub fn storage(&self) -> usize {
        self.aq.capacity() + self.bq.capacity()
    }
    pub fn step(mut self, ctx: &mut Context<'_, '_>, input: Option<Value>) -> Result<Step, Error> {
        if self.phase == 2 {
            enqueue(ctx, at(&self.bq, self.pos)?, sorted(input)?, &mut self.bq)?;
            self.pos += 1;
            self.phase = 0;
            return Ok(Step::Again(Frame::ComparePairs(self)));
        }
        if self.pos >= self.aq.len().min(self.bq.len()) {
            return Ok(Step::Done(Value::Number(0)));
        }
        let (a, b) = (at(&self.aq, self.pos)?, at(&self.bq, self.pos)?);
        let child = if self.phase == 0 {
            let alike = reference(ctx.graph.node(a)?.aux) == self.ra;
            let blike = reference(ctx.graph.node(b)?.aux) == self.rb;
            let cmp = scalar::order(alike, blike);
            if cmp != 0 {
                return Ok(Step::Done(Value::Number(cmp)));
            }
            self.phase = 1;
            Frame::sort(
                a,
                ctx.graph.edges(ctx.mol, a)?.to_vec(),
                self.spec.replacement(self.ra),
                true,
            )
        } else {
            enqueue(ctx, a, sorted(input)?, &mut self.aq)?;
            self.phase = 2;
            Frame::sort(
                b,
                ctx.graph.edges(ctx.mol, b)?.to_vec(),
                self.spec.replacement(self.rb),
                true,
            )
        };
        Ok(Step::Call(Frame::ComparePairs(self), child))
    }
}
