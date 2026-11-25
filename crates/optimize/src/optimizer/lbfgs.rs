//! Limited-memory inverse BFGS in free Cartesian coordinates.

use std::collections::VecDeque;

use super::coordinates::project;
use crate::linalg::{dot, norm};

struct Pair {
    s: Vec<f64>,
    y: Vec<f64>,
    ys: f64,
    yy: f64,
}

#[derive(Default)]
pub(super) struct Lbfgs {
    pairs: VecDeque<Pair>,
}

impl Lbfgs {
    pub fn clear(&mut self) {
        self.pairs.clear();
    }

    pub fn has_history(&self) -> bool {
        !self.pairs.is_empty()
    }

    pub fn update(&mut self, s: Vec<f64>, y: Vec<f64>) {
        let ys = dot(&y, &s);
        let yy = dot(&y, &y);
        if !(ys > 0. && ys.is_finite() && yy > 0. && yy.is_finite()) {
            self.clear();
            return;
        }
        if self.pairs.len() == 20 {
            self.pairs.pop_front();
        }
        self.pairs.push_back(Pair { s, y, ys, yy });
    }

    pub fn direction(&mut self, g: &[f64], rigid: &[Vec<f64>]) -> Option<Vec<f64>> {
        // Nocedal & Wright, Numerical Optimization (2nd ed.), Algorithm 7.4.
        let mut q = g.to_vec();
        let mut alphas = Vec::with_capacity(self.pairs.len());
        for pair in self.pairs.iter().rev() {
            let a = dot(&pair.s, &q) / pair.ys;
            alphas.push(a);
            for (v, y) in q.iter_mut().zip(&pair.y) {
                *v -= a * y;
            }
        }
        let gamma = self.pairs.back().map_or(2., |p| p.ys / p.yy);
        q.iter_mut().for_each(|v| *v *= gamma);
        for (pair, a) in self.pairs.iter().zip(alphas.into_iter().rev()) {
            let b = dot(&pair.y, &q) / pair.ys;
            for (v, s) in q.iter_mut().zip(&pair.s) {
                *v += (a - b) * s;
            }
        }
        q.iter_mut().for_each(|v| *v = -*v);
        project(&mut q, rigid);
        let slope = dot(g, &q);
        if !slope.is_finite() || slope >= 0. {
            self.clear();
            q.iter_mut().zip(g).for_each(|(v, g)| *v = -2. * g);
        }
        let largest = q
            .as_chunks::<3>()
            .0
            .iter()
            .map(|atom| norm(atom))
            .fold(0., f64::max);
        if largest > 0.3 {
            q.iter_mut().for_each(|v| *v *= 0.3 / largest);
        }
        let slope = dot(g, &q);
        (slope.is_finite() && slope < 0.).then_some(q)
    }
}
