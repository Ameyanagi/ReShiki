//! Where a molecule's 3D coordinates came from (an xTB conformer, an
//! optimized geometry or step, the file or the force field) and the other
//! geometries its current xTB results hold, for the input-file dialog and
//! the Undo status.
use super::{conformers, model_shows, results};
use crate::app::App;
use reshiki::models3d::ForceField;
use reshiki::qcdata::{CalcResult, Frame, Settings, Task};
use reshiki::science::Freshness;

/// One geometry the input-file dialog can write.
pub(in crate::app) struct Source {
    /// The dialog's line, e.g. "xTB conformer 1 of 4 · GFN2-xTB · 26 Nov 12:46".
    pub label: String,
    /// For the file's title line, e.g. "GFN2-xTB conformer 1".
    pub short: String,
    /// `None` uses the 3D model as it is. `Some` supplies coordinates in frame order (Å).
    pub frame: Option<Frame>,
    /// Electronic state of the xTB result shown by the current 3D model.
    model_state: Option<(i32, u32)>,
}

impl Source {
    pub(in crate::app) fn saved(frame: Frame, label: String) -> Self {
        Self {
            label,
            short: String::new(),
            frame: Some(frame),
            model_state: None,
        }
    }

    pub fn electronic_state(&self) -> Option<(i32, u32)> {
        self.frame
            .as_ref()
            .map(|frame| (frame.charge, frame.multiplicity))
            .or(self.model_state)
    }
}

impl App {
    /// The model's geometry first, then other geometries from current xTB results.
    pub(in crate::app) fn xtb_geometry_sources(&self, atoms: &[u64]) -> Vec<Source> {
        let doc = &self.tab.doc;
        let science = &doc.science;
        let mut geometries = science.results.iter().rev().filter_map(|stored| {
            let result = &*stored.result;
            let Settings::Xtb(settings) = &result.settings else {
                return None;
            };
            if !matches!(result.task, Task::Optimize | Task::Conformers) {
                return None;
            }
            let link = science.attachment(stored.attachment)?;
            link.atoms
                .iter()
                .flatten()
                .any(|id| atoms.contains(id))
                .then_some((
                    result,
                    link,
                    method_label(result).unwrap_or_else(|| settings.method.label()),
                ))
        });
        let model = geometries.clone().find_map(|(result, link, method)| {
            let shows = |xyz: &[[f64; 3]]| model_shows(doc, &link.atoms, xyz);
            let (mut source, state) = if conformers::is_search(result) {
                let path = result.trajectory.as_ref()?;
                let count = path.steps();
                let step = (0..count).find(|&i| {
                    path.frame(i)
                        .is_some_and(|frame| shows(&frame.positions_angstrom))
                })?;
                (
                    conformer(method, step, count),
                    (path.charge, path.multiplicity),
                )
            } else {
                let output = science.frame(&result.frame)?;
                let source = if shows(&output.positions_angstrom) {
                    optimized(method)
                } else {
                    let input = science.frame(&link.frame)?;
                    let count = results::energies(result).len();
                    let step = (0..count.saturating_sub(1)).rev().find(|&i| {
                        results::step_positions(result, i, input, output)
                            .is_some_and(|xyz| shows(&xyz))
                    })?;
                    Source {
                        label: format!("xTB optimization step {step} · {method}"),
                        short: format!("{method} step {step}"),
                        frame: None,
                        model_state: None,
                    }
                };
                (source, (output.charge, output.multiplicity))
            };
            source.model_state = Some(state);
            if let Some(time) = results::finished_local(result) {
                source
                    .label
                    .push_str(&format!(" · {}", time.format("%-d %b %H:%M")));
            }
            Some(source)
        });
        let model = model.unwrap_or_else(|| {
            let (label, short) = match doc
                .models
                .iter()
                .find(|m| atoms.iter().any(|&id| m.contains(id)))
            {
                Some(model) if model.is_file_derived() => {
                    ("From the file".into(), "geometry from the file".into())
                }
                Some(model) => {
                    let ff = match model.force_field {
                        ForceField::Mmff94 => "MMFF94",
                        ForceField::Mmff94s => "MMFF94s",
                        ForceField::Uff => "UFF",
                    };
                    (
                        format!("Force-field model ({ff})"),
                        format!("{ff} force-field model"),
                    )
                }
                None => ("No 3D model yet.".into(), String::new()),
            };
            Source {
                label,
                short,
                frame: None,
                model_state: None,
            }
        });
        let mut sources = vec![model];
        let current = |link: &reshiki::science::Attachment| {
            let mut ids = link.atoms.iter().flatten();
            ids.clone().count() == atoms.len()
                && ids.all(|id| atoms.contains(id))
                && link.freshness(doc) == Freshness::Current
        };
        if let Some((result, link, method)) = geometries
            .clone()
            .find(|(result, link, _)| conformers::is_search(result) && current(link))
            && let Some(path) = &result.trajectory
        {
            let count = path.steps();
            let energies = results::energies(result);
            for i in 0..count {
                if let Some(mut frame) = path.frame(i)
                    && !model_shows(doc, &link.atoms, &frame.positions_angstrom)
                {
                    let mut source = conformer(method, i, count);
                    if let Some((energy, lowest)) = energies.get(i).zip(energies.first()) {
                        let delta = (energy - lowest) * conformers::KCAL_PER_HARTREE;
                        source
                            .label
                            .push_str(&format!(" · ΔE {delta:+.2} kcal/mol"));
                    }
                    frame.drawing_atoms.clone_from(&link.atoms);
                    source.frame = Some(frame);
                    sources.push(source);
                }
            }
            // The geometry the search started from, as the model had it then.
            if let Some(start) = science.frame(&link.frame)
                && !model_shows(doc, &link.atoms, &start.positions_angstrom)
            {
                sources.push(Source {
                    label: "Start geometry before the conformer search".into(),
                    short: "start geometry before the conformer search".into(),
                    frame: Some(Frame {
                        drawing_atoms: link.atoms.clone(),
                        ..(**start).clone()
                    }),
                    model_state: None,
                });
            }
        }
        if let Some((result, link, method)) =
            geometries.find(|(result, link, _)| result.task == Task::Optimize && current(link))
            && let (Some(output), Some(input)) =
                (science.frame(&result.frame), science.frame(&link.frame))
        {
            let with_atoms = |frame: &Frame| Frame {
                drawing_atoms: link.atoms.clone(),
                ..frame.clone()
            };
            if !model_shows(doc, &link.atoms, &output.positions_angstrom) {
                sources.push(Source {
                    frame: Some(with_atoms(output)),
                    ..optimized(method)
                });
            }
            if input.positions_angstrom != output.positions_angstrom
                && !model_shows(doc, &link.atoms, &input.positions_angstrom)
            {
                sources.push(Source {
                    label: "Start geometry before xTB".into(),
                    short: "start geometry before xTB".into(),
                    frame: Some(with_atoms(input)),
                    model_state: None,
                });
            }
        }
        sources
    }
}

fn method_label(result: &CalcResult) -> Option<&str> {
    let method = result.provenance.method.as_deref()?;
    // A search records its recipe. Name the method that relaxed the kept geometries.
    if conformers::is_search(result) {
        if let Some((_, rerank)) = method.rsplit_once(" · ")
            && let Some(method) = rerank.strip_suffix(" re-rank")
        {
            return Some(method);
        }
        if let Some((minimizer, _)) = method.split_once(" ETKDG ") {
            return Some(minimizer);
        }
    }
    Some(method)
}

fn conformer(method: &str, step: usize, count: usize) -> Source {
    let k = step + 1;
    Source {
        label: format!("xTB conformer {k} of {count} · {method}"),
        short: format!("{method} conformer {k}"),
        frame: None,
        model_state: None,
    }
}

fn optimized(method: &str) -> Source {
    Source {
        label: format!("xTB optimized geometry · {method}"),
        short: format!("{method} optimized"),
        frame: None,
        model_state: None,
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::super::tests::{MOVED, START, event, optimized, water};
    use super::*;
    use crate::app::xtb::CalcTask;

    #[test]
    fn an_optimization_names_the_model_and_offers_its_start() {
        let (mut app, oxygen) = water(CalcTask::Optimize);
        let sources = app.xtb_geometry_sources(&[oxygen]);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].label, "Force-field model (MMFF94s)");

        event(&mut app, optimized(&START, &MOVED));
        let sources = app.xtb_geometry_sources(&[oxygen]);
        assert_eq!(sources.len(), 2);
        assert!(
            sources[0]
                .label
                .starts_with("xTB optimized geometry · GFN2-xTB · ")
        );
        assert_eq!(sources[0].short, "GFN2-xTB optimized");
        assert!(sources[0].frame.is_none());
        assert_eq!(sources[1].label, "Start geometry before xTB");
        assert_eq!(sources[1].short, "start geometry before xTB");
        let start = sources[1].frame.as_ref().unwrap();
        assert_eq!(
            start.positions_angstrom,
            START.map(|xyz| xyz.map(|x| f64::from(x as f32)))
        );
        assert_eq!(start.numbers, [8, 1, 1]);
        assert_eq!(start.drawing_atoms, [Some(oxygen), None, None]);

        app.tab.doc.atoms[0].charge = -1;
        let stale = app.xtb_geometry_sources(&[oxygen]);
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].label, sources[0].label);
    }

    #[test]
    fn a_search_names_the_applied_conformer_and_offers_other_ranks() {
        let (app, oxygen) = App::xtb_searched_water();
        let sources = app.xtb_geometry_sources(&[oxygen]);
        assert_eq!(sources.len(), 4);
        assert!(
            sources[0]
                .label
                .starts_with("xTB conformer 1 of 3 · GFN2-xTB · ")
        );
        assert!(sources[0].frame.is_none());
        assert_eq!(
            sources[1].label,
            "xTB conformer 2 of 3 · GFN2-xTB · ΔE +0.63 kcal/mol"
        );
        assert_eq!(sources[1].short, "GFN2-xTB conformer 2");
        let second = sources[1].frame.as_ref().unwrap();
        assert_eq!(second.numbers, [8, 1, 1]);
        assert_eq!(second.drawing_atoms, [Some(oxygen), None, None]);
        let start = sources.last().unwrap();
        assert_eq!(start.label, "Start geometry before the conformer search");
        let start = start.frame.as_ref().unwrap();
        assert_eq!(
            start.positions_angstrom,
            START.map(|xyz| xyz.map(|x| f64::from(x as f32)))
        );
        assert_eq!(start.drawing_atoms, [Some(oxygen), None, None]);
    }
}
