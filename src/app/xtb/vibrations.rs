use super::{palette::Shown, state::signed};
use crate::app::{App, Message};
use iced::Element;
use iced::widget::{column, text};

impl App {
    pub(super) fn xtb_vibrations<'a>(
        &'a self,
        shown: &Shown<'a>,
        _inspector: bool,
    ) -> Element<'a, Message> {
        let Some(vibrations) = &shown.stored.result.vibrations else {
            return column![].into();
        };
        column(
            vibrations
                .frequencies_cm1
                .iter()
                .enumerate()
                .map(|(i, &f)| {
                    let row = if f < 0.0 {
                        format!("{}i cm⁻¹ · imaginary", signed(f, 1))
                    } else if let Some(ir) = vibrations.ir_intensities_km_mol.get(i) {
                        format!("{f:.2} cm⁻¹ · IR {ir:.1}")
                    } else {
                        format!("{f:.2} cm⁻¹")
                    };
                    text(row).size(12).into()
                }),
        )
        .spacing(4)
        .into()
    }
}
