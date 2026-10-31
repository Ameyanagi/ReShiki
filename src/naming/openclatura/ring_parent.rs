//! Structured ring-parent identity and topology-to-hydride classification.
use super::chains::RingSystem;
use super::ring_renderer::{is_von_baeyer_descriptor, render_ring_descriptor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParentHydrideKind {
    GeneratedMonocycle,
    VonBaeyer,
    Spiro,
    LegacyRing,
}

#[derive(Clone, Debug)]
pub struct RingParent {
    pub hydride_kind: ParentHydrideKind,
    pub descriptor: Option<String>,
}

impl RingParent {
    pub fn from_ring_system(system: &RingSystem) -> Self {
        let (hydride_kind, descriptor) = if system.is_bicycle {
            (
                ParentHydrideKind::VonBaeyer,
                Some(render_ring_descriptor(
                    "bicyclo",
                    &[system.x, system.y, system.z],
                )),
            )
        } else if system.is_spiro {
            (
                ParentHydrideKind::Spiro,
                Some(render_ring_descriptor("spiro", &[system.x, system.y])),
            )
        } else if system.is_polycycle {
            let descriptor = system.polycycle_descriptor.clone();
            let hydride_kind = if descriptor
                .as_deref()
                .is_some_and(|d| d.starts_with("dispiro["))
            {
                ParentHydrideKind::Spiro
            } else if is_von_baeyer_descriptor(descriptor.as_deref()) {
                ParentHydrideKind::VonBaeyer
            } else {
                ParentHydrideKind::LegacyRing
            };
            (hydride_kind, descriptor)
        } else {
            (ParentHydrideKind::GeneratedMonocycle, None)
        };
        Self {
            hydride_kind,
            descriptor,
        }
    }

    pub fn requires_fixed_numbering(&self) -> bool {
        self.hydride_kind != ParentHydrideKind::GeneratedMonocycle
    }
}
