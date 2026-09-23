use super::*;
use std::collections::{BTreeSet, HashSet, VecDeque};

impl Writer<'_> {
    pub(super) fn crossings(&mut self, middle: usize) -> Result<()> {
        let doc = self.doc;
        let mut segments = doc
            .bonds
            .iter()
            .enumerate()
            .map(|(i, b)| {
                Ok((
                    i,
                    P::from(self.atom(b.a)?.position),
                    P::from(self.atom(b.b)?.position),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        segments.sort_by(|(i, a, b), (j, c, d)| {
            a.x.min(b.x).total_cmp(&c.x.min(d.x)).then_with(|| i.cmp(j))
        });
        let mut active: Vec<(usize, P, P)> = Vec::new();
        let mut budget = 200_000;
        let mut crossings = vec![Vec::new(); doc.bonds.len()];
        'sweep: for (i, a, b) in segments {
            active.retain(|(_, c, d)| c.x.max(d.x) >= a.x.min(b.x));
            for &(j, c, d) in &active {
                if budget == 0 {
                    break 'sweep;
                }
                budget -= 1;
                let first = doc
                    .bonds
                    .get(i)
                    .ok_or_else(|| invalid("Missing crossing bond"))?;
                let second = doc
                    .bonds
                    .get(j)
                    .ok_or_else(|| invalid("Missing crossing bond"))?;
                if first.a == second.a
                    || first.a == second.b
                    || first.b == second.a
                    || first.b == second.b
                {
                    continue;
                }
                let (ax, ay, bx, by) = (b.x - a.x, b.y - a.y, d.x - c.x, d.y - c.y);
                let det = ax * by - ay * bx;
                if det.abs() < 0.001 {
                    continue;
                }
                let (dx, dy) = (c.x - a.x, c.y - a.y);
                let t = (dx * by - dy * bx) / det;
                let u = (dx * ay - dy * ax) / det;
                if t > 0.04 && t < 0.96 && u > 0.04 && u < 0.96 {
                    crossings
                        .get_mut(i)
                        .ok_or_else(|| invalid("Missing crossing list"))?
                        .push(j);
                    crossings
                        .get_mut(j)
                        .ok_or_else(|| invalid("Missing crossing list"))?
                        .push(i);
                }
            }
            active.push((i, a, b));
        }
        if crossings.iter().all(Vec::is_empty) {
            return Ok(());
        }
        let mut ordered: Vec<_> = (0..doc.bonds.len()).collect();
        ordered.sort_by_key(|&i| doc.bonds.get(i).map(|b| (b.z_order, i)));
        if middle + ordered.len() + doc.graphics.len() > 32760 {
            return Err(invalid("Too many bond layers for editable interchange"));
        }
        for n in self.tree.descendants(self.page)? {
            if let Some(z) = self.tree.get(n, "Z")? {
                let z: usize = z.parse().map_err(|_| invalid("Invalid drawing layer"))?;
                if z > middle {
                    self.tree.set(n, "Z", (z + ordered.len()).to_string())?;
                }
            }
        }
        for (rank, i) in ordered.into_iter().enumerate() {
            let n = *self
                .bond_nodes
                .get(i)
                .ok_or_else(|| invalid("Missing crossing node"))?;
            self.tree.set(n, "Z", (middle + rank).to_string())?;
            let crossing = crossings
                .get(i)
                .ok_or_else(|| invalid("Missing crossing list"))?;
            if !crossing.is_empty() {
                let ids = crossing
                    .iter()
                    .map(|j| {
                        self.tree.value(
                            *self
                                .bond_nodes
                                .get(*j)
                                .ok_or_else(|| invalid("Missing crossing node"))?,
                            "id",
                        )
                    })
                    .collect::<Result<Vec<_>>>()?;
                self.tree.set(n, "CrossingBonds", ids.join(" "))?;
            }
        }
        Ok(())
    }
    pub(super) fn groups(&mut self) -> Result<()> {
        let doc = self.doc;
        if doc.groups.is_empty() {
            return Ok(());
        }
        let sets: Vec<HashSet<_>> = doc
            .groups
            .iter()
            .map(|g| g.members.iter().copied().collect())
            .collect();
        // Attachment targets share a drawing fragment with their point even
        // though the target membership is not a chemical bond.
        let edge_count = crate::attachments::edges(doc).count();
        self.spend(edge_count)?;
        for set in &sets {
            self.spend(edge_count)?;
            if crate::attachments::edges(doc).any(|(a, b)| set.contains(&a) != set.contains(&b)) {
                return Err(invalid(
                    "A group cuts through a molecule; group the whole molecule before editable export",
                ));
            }
        }
        let mut remaining: BTreeSet<_> = doc.atoms.iter().map(|a| a.id).collect();
        let mut neighbors: HashMap<u64, Vec<u64>> = HashMap::new();
        for (a, b) in crate::attachments::edges(doc) {
            neighbors.entry(a).or_default().push(b);
            neighbors.entry(b).or_default().push(a);
        }
        let mut components = Vec::new();
        while let Some(&first) = remaining.first() {
            let mut component = HashSet::new();
            let mut queue = VecDeque::from([first]);
            remaining.remove(&first);
            while let Some(id) = queue.pop_front() {
                component.insert(id);
                for &other in neighbors.get(&id).into_iter().flatten() {
                    if remaining.remove(&other) {
                        queue.push_back(other);
                    }
                }
            }
            components.push(component);
        }
        let children = self.tree.node(self.fragment)?.children.clone();
        for (index, component) in components.into_iter().enumerate() {
            let target = if index == 0 {
                self.fragment
            } else {
                let id = self.id()?;
                let z = self.tree.value(self.fragment, "Z")?;
                self.tree
                    .add(Some(self.page), "fragment", [("id", id), ("Z", z)])?
            };
            let ids = component
                .iter()
                .map(|id| self.atom_xml(*id))
                .collect::<Result<HashSet<_>>>()?;
            for &n in &children {
                self.spend(1)?;
                let el = self.tree.node(n)?;
                let belongs = match el.tag {
                    "n" => self.tree.get(n, "id")?.is_some_and(|id| ids.contains(id)),
                    "b" => self.tree.get(n, "B")?.is_some_and(|id| ids.contains(id)),
                    "graphic" => el.children.iter().any(|&k| {
                        self.tree.node(k).is_ok_and(|n| n.tag == "represent")
                            && self
                                .tree
                                .get(k, "object")
                                .is_ok_and(|id| id.is_some_and(|id| ids.contains(id)))
                    }),
                    _ => false,
                };
                if belongs && target != self.fragment {
                    self.tree.attach(target, n)?;
                }
            }
            for (id, n) in &mut self.objects {
                if component.contains(id) {
                    *n = target;
                }
            }
        }
        let mut containers = Vec::new();
        for g in &doc.groups {
            let id = self.id()?;
            containers.push(self.tree.add(
                None,
                "group",
                [("id", id), ("Integral", yes(g.integral).into())],
            )?);
        }
        for (i, set) in sets.iter().enumerate() {
            self.spend(sets.iter().map(HashSet::len).sum())?;
            let parent = sets
                .iter()
                .enumerate()
                .filter(|(_, other)| other.len() > set.len() && set.is_subset(other))
                .min_by_key(|(_, other)| other.len())
                .and_then(|(j, _)| containers.get(j))
                .copied()
                .unwrap_or(self.page);
            self.tree.attach(
                parent,
                *containers
                    .get(i)
                    .ok_or_else(|| invalid("Missing drawing group"))?,
            )?;
        }
        let mut moved = HashSet::new();
        for (id, n) in self.objects.clone() {
            self.spend(sets.len())?;
            let owner = sets
                .iter()
                .enumerate()
                .filter(|(_, set)| set.contains(&id))
                .min_by_key(|(_, set)| set.len())
                .map(|(i, _)| i);
            if let Some(i) = owner
                && moved.insert(n)
            {
                self.tree.attach(
                    *containers
                        .get(i)
                        .ok_or_else(|| invalid("Missing drawing group"))?,
                    n,
                )?;
            }
        }
        for n in containers {
            let values = self
                .tree
                .descendants(n)?
                .into_iter()
                .filter_map(|k| self.tree.get(k, "Z").transpose())
                .collect::<Result<Vec<_>>>()?;
            let z = values
                .iter()
                .map(|s| {
                    s.parse::<usize>()
                        .map_err(|_| invalid("Invalid drawing layer"))
                })
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .min()
                .unwrap_or(0);
            self.tree.set(n, "Z", z.to_string())?;
        }
        Ok(())
    }
    pub(super) fn abbreviations(&mut self) -> Result<()> {
        let doc = self.doc;
        self.next = self
            .tree
            .descendants(0)?
            .iter()
            .filter_map(|&n| self.tree.get(n, "id").transpose())
            .map(|s| s.and_then(|s| s.parse::<usize>().map_err(|_| invalid("Invalid XML id"))))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .max()
            .unwrap_or(0)
            + 1;
        for group in &doc.abbreviations {
            let member_ids = group
                .members
                .iter()
                .map(|id| self.atom_xml(*id))
                .collect::<Result<HashSet<_>>>()?;
            let anchor = self.atom_node(group.anchor)?;
            let parent = self
                .tree
                .node(anchor)?
                .parent
                .ok_or_else(|| invalid("Missing abbreviation fragment"))?;
            for id in &group.members {
                let atom = self.atom(*id)?;
                let n = self.atom_node(*id)?;
                if self.tree.node(n)?.parent != Some(parent) {
                    return Err(invalid("Abbreviation crosses drawing fragments"));
                }
                if !atom.marks.is_empty() || atom.display.number.is_some() {
                    return Err(invalid(
                        "Expand abbreviations with attached marks or atom numbers before editable export",
                    ));
                }
            }
            let mut internal = Vec::new();
            let mut external = Vec::new();
            for &n in &self.tree.node(parent)?.children {
                if self.tree.node(n)?.tag != "b" {
                    continue;
                }
                let a = self
                    .tree
                    .get(n, "B")?
                    .is_some_and(|s| member_ids.contains(s));
                let b = self
                    .tree
                    .get(n, "E")?
                    .is_some_and(|s| member_ids.contains(s));
                if a && b {
                    internal.push(n);
                } else if a != b {
                    external.push(n);
                }
            }
            if external.len() > 1 {
                return Err(invalid(
                    "Multiple abbreviation attachments are not supported",
                ));
            }
            let id = self.id()?;
            let position = self.tree.value(anchor, "p")?;
            let z = self.tree.get(anchor, "Z")?.unwrap_or("0").to_owned();
            let outer = self.tree.add(
                Some(parent),
                "n",
                [
                    ("id", id),
                    ("p", position.clone()),
                    ("NodeType", "Fragment".into()),
                    ("Z", z),
                ],
            )?;
            let id = self.id()?;
            let inner = self.tree.add(Some(outer), "fragment", [("id", id)])?;
            for id in &group.members {
                self.tree.attach(inner, self.atom_node(*id)?)?;
            }
            for n in internal {
                self.tree.attach(inner, n)?;
            }
            let mut left = false;
            if let Some(&bond) = external.first() {
                let side = if self
                    .tree
                    .get(bond, "B")?
                    .is_some_and(|s| member_ids.contains(s))
                {
                    "B"
                } else {
                    "E"
                };
                let other = if side == "B" { "E" } else { "B" };
                let other_id = self.tree.value(bond, other)?;
                let other_node = self
                    .tree
                    .descendants(0)?
                    .into_iter()
                    .find(|&n| {
                        self.tree.node(n).is_ok_and(|n| n.tag == "n")
                            && self
                                .tree
                                .get(n, "id")
                                .is_ok_and(|id| id == Some(other_id.as_str()))
                    })
                    .ok_or_else(|| invalid("Missing outside abbreviation atom"))?;
                let other_p = self.tree.value(other_node, "p")?;
                let first = |s: &str| -> Result<f64> {
                    s.split_whitespace()
                        .next()
                        .ok_or_else(|| invalid("Missing abbreviation position"))?
                        .parse()
                        .map_err(|_| invalid("Invalid abbreviation position"))
                };
                left = first(&other_p)? > first(&position)? + 0.01;
                let connection = self.id()?;
                self.tree.add(
                    Some(inner),
                    "n",
                    [
                        ("id", connection.clone()),
                        ("p", other_p),
                        ("NodeType", "ExternalConnectionPoint".into()),
                    ],
                )?;
                let id = self.id()?;
                let order = self.tree.get(bond, "Order")?.unwrap_or("1").to_owned();
                self.tree.add(
                    Some(inner),
                    "b",
                    [
                        ("id", id),
                        ("B", connection.clone()),
                        ("E", self.tree.value(anchor, "id")?),
                        ("Order", order),
                    ],
                )?;
                self.tree.set(inner, "ConnectionOrder", connection)?;
                self.tree
                    .set(outer, "BondOrdering", self.tree.value(bond, "id")?)?;
                self.tree.set(bond, side, self.tree.value(outer, "id")?)?;
            }
            let mut style = self
                .atom(group.anchor)?
                .text_style
                .clone()
                .unwrap_or_default();
            style.formula = true;
            style.script = crate::typography::Script::Normal;
            let mut attributes = vec![
                ("p", position),
                (
                    "LabelAlignment",
                    if group.alignment.is_auto() {
                        if left { "Right" } else { "Left" }
                    } else {
                        group.alignment.cdxml()
                    }
                    .into(),
                ),
            ];
            if !group.alignment.is_auto() {
                attributes.push(("LabelJustification", group.alignment.cdxml().into()));
            }
            self.text(
                outer,
                &group.label,
                &TextFormat {
                    style,
                    ..Default::default()
                },
                attributes,
            )?;
        }
        Ok(())
    }
}
