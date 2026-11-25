use std::fmt::Write;

use crate::qcdata::Trajectory;

use super::{Frame, clean_title, single_line, write_atoms};

pub(super) fn frame(frame: &Frame, title: &str) -> String {
    let mut text = String::new();
    write_frame(&mut text, frame, title);
    text
}

pub fn write_xyz_trajectory(path: &Trajectory, title: &str) -> String {
    let mut text = String::new();
    if path.positions_angstrom.is_empty() {
        return text;
    }
    let title = clean_title(title);
    for step in 0..path.steps() {
        let Some(frame) = path.frame(step) else { break };
        let mut comment = path.labels.get(step).map_or_else(
            || format!("{title} step {}", step + 1),
            |label| single_line(label),
        );
        if let Some(energy) = path.energies_hartree.get(step) {
            let _ = write!(comment, "  E = {energy:.10} Eh");
        }
        write_frame(&mut text, &frame, &comment);
    }
    text
}

fn write_frame(text: &mut String, frame: &Frame, comment: &str) {
    let _ = writeln!(text, "{}\n{comment}", frame.numbers.len());
    write_atoms(text, frame, "");
}
