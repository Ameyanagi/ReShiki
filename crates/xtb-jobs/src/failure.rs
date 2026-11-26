use reshiki_qcdata::XtbMethod;
use std::fmt;

/// Why a run failed, as reported by the job.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// The SCF did not converge; the worker's message.
    Scf(String),
    /// `by` (the method that said so, if known) has no parameters for `z`.
    Element {
        z: u8,
        by: Option<XtbMethod>,
    },
    TooMany {
        count: usize,
        limit: usize,
    },
    /// The worker stopped: exited, timed out or broke the protocol.
    Crashed {
        summary: String,
        /// Diagnostic from the worker client, which can include worker stderr.
        details: Option<String>,
    },
    /// The method's valence model needs `valence` unpaired electrons.
    Spin {
        valence: u32,
    },
    Other(String),
}

impl Failure {
    pub fn crashed(summary: impl Into<String>) -> Self {
        Self::Crashed {
            summary: summary.into(),
            details: None,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scf(message) | Self::Other(message) => f.write_str(message),
            Self::Crashed { summary, .. } => f.write_str(summary),
            Self::Element { z, by } => match by {
                Some(method) => write!(f, "{} has no parameters for Z={z}", method.label()),
                None => write!(f, "No xTB parameters for Z={z}"),
            },
            Self::TooMany { count, limit } => write!(
                f,
                "Conformer search re-ranks with xTB up to {limit} atoms; this molecule has {count}."
            ),
            Self::Spin { .. } => f.write_str(
                "Unpaired electrons must have the same parity as the valence electron count",
            ),
        }
    }
}
