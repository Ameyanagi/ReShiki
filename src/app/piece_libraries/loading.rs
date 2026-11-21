//! One scan per folder at a time, with at most the newest refresh waiting.
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Mine,
    Lab(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub serial: u64,
    pub target: Target,
}

#[derive(Default)]
pub struct Queue {
    serial: u64,
    waiting: BTreeMap<Target, u64>,
    running: BTreeMap<Target, u64>,
}

impl Queue {
    pub fn request(&mut self, labs: impl Iterator<Item = PathBuf>) -> Vec<Job> {
        self.serial += 1;
        self.waiting = std::iter::once(Target::Mine)
            .chain(labs.map(Target::Lab))
            .map(|target| (target, self.serial))
            .collect();
        let ready: Vec<_> = self
            .waiting
            .keys()
            .filter(|target| !self.running.contains_key(*target))
            .cloned()
            .collect();
        ready.into_iter().map(|target| self.start(target)).collect()
    }

    pub fn serial(&self) -> u64 {
        self.serial
    }

    pub fn loading(&self, target: &Target) -> bool {
        self.waiting.contains_key(target)
    }

    pub fn pending(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// Whether this result belongs to the current request, and any newest
    /// refresh that can now start. An unlinked folder never starts again.
    pub fn finish(&mut self, job: &Job) -> (bool, Option<Job>) {
        if self.running.get(&job.target) != Some(&job.serial) {
            return (false, None);
        }
        self.running.remove(&job.target);
        match self.waiting.get(&job.target) {
            Some(serial) if *serial == job.serial => {
                self.waiting.remove(&job.target);
                (true, None)
            }
            Some(_) => (false, Some(self.start(job.target.clone()))),
            None => (false, None),
        }
    }

    fn start(&mut self, target: Target) -> Job {
        let serial = self.waiting[&target];
        self.running.insert(target.clone(), serial);
        Job { serial, target }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(paths: &[&str]) -> impl Iterator<Item = PathBuf> {
        paths
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn local_scan_can_finish_while_a_share_is_still_loading() {
        let mut queue = Queue::default();
        let jobs = queue.request(roots(&["/slow/share"]));
        let mine = jobs.iter().find(|job| job.target == Target::Mine).unwrap();
        assert_eq!(queue.finish(mine), (true, None));
        assert!(!queue.loading(&Target::Mine));
        assert!(queue.loading(&Target::Lab("/slow/share".into())));
        assert!(queue.pending());
    }

    #[test]
    fn rapid_refreshes_coalesce_and_old_results_cannot_clear_the_new_request() {
        let mut queue = Queue::default();
        let first = queue.request(roots(&["/lab"]));
        for _ in 0..20 {
            assert!(queue.request(roots(&["/lab"])).is_empty());
        }
        let serial = queue.serial();
        for old in first {
            let (accepted, next) = queue.finish(&old);
            assert!(!accepted);
            let next = next.unwrap();
            assert_eq!(next.serial, serial);
            assert_eq!(queue.finish(&old), (false, None));
            assert!(queue.loading(&next.target));
            assert_eq!(queue.finish(&next), (true, None));
        }
        assert!(!queue.pending());
    }

    #[test]
    fn unlinked_share_is_not_restarted_or_accepted() {
        let mut queue = Queue::default();
        let old = queue.request(roots(&["/removed"]));
        let added = queue.request(roots(&["/new"]));
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].target, Target::Lab("/new".into()));
        let removed = old
            .iter()
            .find(|job| matches!(job.target, Target::Lab(_)))
            .unwrap();
        assert_eq!(queue.finish(removed), (false, None));
        assert!(!queue.loading(&removed.target));
        assert!(queue.loading(&added[0].target));
    }

    #[test]
    fn relinking_a_running_share_requires_a_fresh_scan() {
        let mut queue = Queue::default();
        let old = queue.request(roots(&["/lab"]));
        queue.request(roots(&[]));
        assert!(queue.request(roots(&["/lab"])).is_empty());
        let old = old
            .iter()
            .find(|job| matches!(job.target, Target::Lab(_)))
            .unwrap();
        let (accepted, next) = queue.finish(old);
        assert!(!accepted);
        let next = next.unwrap();
        assert_eq!(next.serial, queue.serial());
        assert_eq!(queue.finish(&next), (true, None));
    }
}
