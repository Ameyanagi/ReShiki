//! #1037: live comparisons check the RDKit-parity layout and wedges, before
//! ReShiki's own refinements: RDKit 2026.09.1's path angle expansion, the
//! atom-on-bond template trigger and bridge repair, and the cleanup
//! chain/template retries (#735), 30° straightening (#745), conventional
//! orientation and salt arrangement (#885), and bridgehead/spiro wedges and
//! ring-fusion H (#793). Those refinements have their own tests. Import's
//! template and long-chain retries keep their v0.13.1 behaviour. Every test in
//! this binary calls this first.
pub fn enable() {
    reshiki::chemistry::document::RDKIT_PARITY.store(true, std::sync::atomic::Ordering::Relaxed);
}
