use super::{Error, Result, at};
use crate::chemistry::depict::geometry::Point;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashSet},
    sync::OnceLock,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Template {
    pub degrees: Vec<Option<usize>>,
    pub edges: Vec<[usize; 2]>,
    pub positions: Vec<Point>,
    #[serde(skip)]
    pub counts: [usize; 5],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Catalog {
    source_commit: String,
    source_sha256: String,
    pub templates: Vec<Template>,
    #[serde(skip)]
    pub by_size: BTreeMap<usize, Vec<usize>>,
}
fn load() -> Result<Catalog> {
    let mut catalog: Catalog = serde_json::from_str(include_str!("builtin.json"))
        .map_err(|_| Error::Invalid("builtin template data"))?;
    if catalog.source_commit != "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
        || catalog.source_sha256
            != "69530df08d9e532a2ce89275359333fe24a2c50868fe91f1942970bc03ee890b"
        || catalog.templates.len() != 578
    {
        return Err(Error::Invalid("builtin template provenance"));
    }
    for (ordinal, template) in catalog.templates.iter_mut().enumerate() {
        let n = template.degrees.len();
        if !(3..=50).contains(&n) || template.positions.len() != n || template.edges.len() > 150 {
            return Err(Error::Invalid("builtin template dimensions"));
        }
        let mut degrees = vec![0usize; n];
        let mut pairs = HashSet::new();
        let mut introduced = vec![false; n];
        for &[a, b] in &template.edges {
            if a == b || !pairs.insert((a.min(b), a.max(b))) {
                return Err(Error::Invalid("builtin template edge"));
            }
            for id in [a, b] {
                *degrees
                    .get_mut(id)
                    .ok_or(Error::Invalid("builtin endpoint"))? += 1;
            }
            *introduced
                .get_mut(a.max(b))
                .ok_or(Error::Invalid("builtin endpoint"))? = true;
        }
        for (i, degree) in degrees.into_iter().enumerate() {
            if degree == 0
                || (i > 0 && !*at(&introduced, i)?)
                || at(&template.degrees, i)?.is_some_and(|d| !matches!(d, 2 | 3 | 4 | 6))
            {
                return Err(Error::Invalid("builtin query traversal"));
            }
            let p = at(&template.positions, i)?;
            if !p.x.is_finite() || !p.y.is_finite() {
                return Err(Error::Invalid("builtin coordinates"));
            }
            *template.counts.get_mut(degree.min(4)).ok_or(Error::Limit)? += 1;
        }
        catalog.by_size.entry(n).or_default().push(ordinal);
    }
    Ok(catalog)
}
pub(super) fn builtin() -> Result<&'static Catalog> {
    static CATALOG: OnceLock<Result<Catalog>> = OnceLock::new();
    CATALOG.get_or_init(load).as_ref().map_err(Clone::clone)
}
