//! Apply a method's capabilities to the options supplied by clients.

#[cfg(any(not(target_arch = "wasm32"), test))]
use reshiki_qcdata::SolventCorrection;
use reshiki_qcdata::{Solvent, SolventModel, SolventSupport, XtbMethod};

pub fn accepts_spin(method: XtbMethod, unpaired: u32) -> bool {
    unpaired == 0 || method.capabilities().open_shell
}

pub fn supports_scf(method: XtbMethod) -> bool {
    method.capabilities().orbitals
}

pub fn supports_solvent(method: XtbMethod) -> bool {
    method.capabilities().solvent != SolventSupport::None
}

/// Only read SCF overrides when the method uses them.
pub fn scf_options<T>(method: XtbMethod, read: impl FnOnce() -> T) -> Option<T> {
    supports_scf(method).then(read)
}

pub fn solvent(model: SolventModel, name: &str) -> Solvent {
    Solvent {
        model,
        name: name.to_lowercase(),
        state: Default::default(),
        correction: None,
    }
}

#[cfg(any(not(target_arch = "wasm32"), test))]
pub(crate) fn solvent_correction(method: XtbMethod) -> Option<SolventCorrection> {
    (method.capabilities().solvent == SolventSupport::Gfn2Correction)
        .then_some(SolventCorrection::Gfn2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_field_is_closed_shell_without_scf_or_solvent() {
        assert!(accepts_spin(XtbMethod::GfnFf, 0));
        assert!(!accepts_spin(XtbMethod::GfnFf, 1));
        assert_eq!(scf_options::<()>(XtbMethod::GfnFf, || unreachable!()), None);
        assert!(!supports_solvent(XtbMethod::GfnFf));
    }

    #[test]
    fn gxtb_uses_only_the_gfn2_solvent_correction() {
        assert!(accepts_spin(XtbMethod::GXtb, 1));
        assert_eq!(scf_options(XtbMethod::GXtb, || 500.0), Some(500.0));
        assert!(supports_solvent(XtbMethod::GXtb));
        assert_eq!(
            solvent_correction(XtbMethod::GXtb),
            Some(SolventCorrection::Gfn2)
        );
        assert_eq!(solvent_correction(XtbMethod::Gfn2Xtb), None);
    }
}
