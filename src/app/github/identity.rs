//! Which repository a drawing is in, at a glance: each repository keeps one
//! of six hues, chosen on first sight and kept by repository id, and a one-
//! or two-letter monogram. Tabs and the status chip show a solid badge; lists
//! show a tinted one. The hues avoid the app's teal, red, amber and peach.
use iced::widget::{container, text};
use iced::{Border, Color, Element, Theme};
use reshiki::github::RepoKey;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

pub(super) const HUES: usize = 6;
pub(super) const NAMES: [&str; HUES] = ["Blue", "Violet", "Pink", "Olive", "Cyan", "Slate"];
const LIGHT: [[u8; 3]; HUES] = [
    [0x2F, 0x6F, 0xDE],
    [0x7A, 0x52, 0xCC],
    [0xB8, 0x40, 0x8F],
    [0x5E, 0x7A, 0x1E],
    [0x0F, 0x7C, 0x99],
    [0x5B, 0x66, 0x76],
];
const DARK: [[u8; 3]; HUES] = [
    [0x8A, 0xB4, 0xFF],
    [0xB9, 0xA0, 0xF5],
    [0xEE, 0x9A, 0xD0],
    [0xB4, 0xCC, 0x7A],
    [0x7F, 0xD0, 0xE6],
    [0xB5, 0xBE, 0xCB],
];
/// The monogram on a solid badge.
const LIGHT_INK: [u8; 3] = [0xFF, 0xFF, 0xFF];
const DARK_INK: [u8; 3] = [0x11, 0x14, 0x18];

/// Kept in `<data>/github/prefs.json`.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Prefs {
    /// Hue by repository id.
    #[serde(default)]
    pub(super) colors: BTreeMap<u64, usize>,
    /// The repository a drawing was last opened from or sent to.
    #[serde(default)]
    pub(super) last: Option<u64>,
}

fn prefs_path(root: &Path) -> std::path::PathBuf {
    root.join("prefs.json")
}

pub(super) fn load(root: &Path) -> Prefs {
    std::fs::read(prefs_path(root))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes may finish out of order, so each carries the caller's rising
/// `seq`; one older than the last written is dropped.
pub(super) fn save(root: &Path, prefs: &Prefs, seq: u64) -> Result<(), String> {
    static WRITTEN: std::sync::Mutex<u64> = std::sync::Mutex::new(0);
    let mut written = WRITTEN.lock().map_err(|error| error.to_string())?;
    if *written > seq {
        return Ok(());
    }
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec_pretty(prefs).map_err(|error| error.to_string())?;
    reshiki::storage::write_atomic(&prefs_path(root), &bytes)?;
    *written = seq;
    Ok(())
}

/// FNV-1a: the same on every platform and in every run.
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The hue of repository `id`: the one kept for it, or else the one its name
/// hashes to, moved on to the next hue another kept repository does not use.
pub(super) fn hue(prefs: &Prefs, id: u64, key: &RepoKey) -> usize {
    if let Some(hue) = prefs.colors.get(&id) {
        return hue % HUES;
    }
    let start = (fnv(&format!("{}/{}", key.owner, key.repo).to_lowercase()) % HUES as u64) as usize;
    (0..HUES)
        .map(|step| (start + step) % HUES)
        .find(|hue| {
            !prefs
                .colors
                .iter()
                .any(|(other, taken)| *other != id && taken % HUES == *hue)
        })
        .unwrap_or(start)
}

/// Keeps the hue of a repository seen for the first time. Whether it is new.
pub(super) fn assign(prefs: &mut Prefs, id: u64, key: &RepoKey) -> bool {
    if prefs.colors.contains_key(&id) {
        return false;
    }
    let hue = hue(prefs, id, key);
    prefs.colors.insert(id, hue);
    true
}

/// `lab-notes` → `LN`, `spectra` → `SP`.
pub(super) fn monogram(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let letters: String = match words.as_slice() {
        [] => "?".into(),
        [one] => one.chars().take(2).collect(),
        [first, second, ..] => first.chars().take(1).chain(second.chars().take(1)).collect(),
    };
    letters.to_uppercase()
}

/// The monogram of each repository in `open`; two that would look the same
/// show the owner's and the repository's initials instead.
pub(super) fn monograms(open: &[RepoKey]) -> HashMap<RepoKey, String> {
    let plain: Vec<String> = open.iter().map(|key| monogram(&key.repo)).collect();
    open.iter()
        .zip(&plain)
        .map(|(key, letters)| {
            let shared = plain.iter().filter(|other| *other == letters).count() > 1;
            let shown = if shared {
                let initial = |name: &str| name.chars().next().unwrap_or('?');
                format!("{}{}", initial(&key.owner), initial(&key.repo)).to_uppercase()
            } else {
                letters.clone()
            };
            (key.clone(), shown)
        })
        .collect()
}

pub(super) fn color(theme: &Theme, hue: usize) -> Color {
    let hues = if crate::appearance::is_dark(theme) {
        DARK
    } else {
        LIGHT
    };
    let [r, g, b] = hues.get(hue % HUES).copied().unwrap_or_default();
    Color::from_rgb8(r, g, b)
}

fn ink(theme: &Theme) -> Color {
    let [r, g, b] = if crate::appearance::is_dark(theme) {
        DARK_INK
    } else {
        LIGHT_INK
    };
    Color::from_rgb8(r, g, b)
}

/// The solid badge of tabs and the chip: the monogram on the repository's hue.
pub(super) fn badge<'a, Message: 'a>(hue: usize, letters: String) -> Element<'a, Message> {
    container(
        text(letters)
            .size(8.5)
            .font(bold())
            .style(move |theme: &Theme| text::Style {
                color: Some(ink(theme)),
            }),
    )
    .center(16)
    .style(move |theme: &Theme| container::Style {
        background: Some(color(theme, hue).into()),
        border: Border {
            radius: 4.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// The tinted tile of repository lists: the monogram in the hue on a light
/// wash of it.
pub(super) fn tile<'a, Message: 'a>(hue: usize, letters: String) -> Element<'a, Message> {
    container(
        text(letters)
            .size(10)
            .font(bold())
            .style(move |theme: &Theme| text::Style {
                color: Some(color(theme, hue)),
            }),
    )
    .center(22)
    .style(move |theme: &Theme| {
        let hue = color(theme, hue);
        container::Style {
            background: Some(Color { a: 0.16, ..hue }.into()),
            border: Border {
                radius: 5.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        }
    })
    .into()
}

/// A round dot in the repository's hue, as in the status chip.
pub(super) fn dot<'a, Message: 'a>(color: impl Fn(&Theme) -> Color + 'a) -> Element<'a, Message> {
    container(iced::widget::Space::new().width(8).height(8))
        .style(move |theme: &Theme| container::Style {
            background: Some(color(theme).into()),
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn bold() -> iced::Font {
    iced::Font {
        weight: iced::font::Weight::Bold,
        ..iced::Font::with_name(reshiki::style::ui_font_family())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(owner: &str, repo: &str) -> RepoKey {
        RepoKey {
            owner: owner.into(),
            repo: repo.into(),
        }
    }

    #[test]
    fn a_hue_is_stable_kept_and_avoids_hues_taken() {
        let mut prefs = Prefs::default();
        let lab = key("octo-test", "lab-notes");
        let first = hue(&prefs, 1, &lab);
        assert_eq!(first, hue(&prefs, 1, &key("Octo-Test", "Lab-Notes")));
        assert!(assign(&mut prefs, 1, &lab));
        assert!(!assign(&mut prefs, 1, &lab));
        // Another repository whose name hashes to the same hue gets the next free one.
        let other = (0..)
            .map(|n| key("octo-test", &format!("repo-{n}")))
            .find(|other| hue(&Prefs::default(), 2, other) == first)
            .unwrap();
        assert_eq!(hue(&prefs, 2, &other), (first + 1) % HUES);
        // A chosen color wins, and survives a round trip through the file.
        prefs.colors.insert(1, 4);
        prefs.last = Some(1);
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &prefs, 2).unwrap();
        assert_eq!(load(dir.path()), prefs);
        // An older write that finishes late changes nothing.
        save(dir.path(), &Prefs::default(), 1).unwrap();
        assert_eq!(load(dir.path()), prefs);
        assert_eq!(hue(&load(dir.path()), 1, &lab), 4);
    }

    #[test]
    fn monograms_turn_to_owner_initials_only_when_open_repositories_collide() {
        assert_eq!(monogram("lab-notes"), "LN");
        assert_eq!(monogram("spectra"), "SP");
        assert_eq!(monogram("reaction_notes"), "RN");
        let open = [
            key("octo-test", "lab-notes"),
            key("pchem", "lab-nmr"),
            key("octo-test", "spectra"),
        ];
        let shown = monograms(&open);
        assert_eq!(shown[&open[0]], "OL");
        assert_eq!(shown[&open[1]], "PL");
        assert_eq!(shown[&open[2]], "SP");
        assert_eq!(monograms(&open[..1])[&open[0]], "LN");
    }

    #[test]
    fn badges_are_readable_and_stand_out_from_the_tabs() {
        use reshiki::color_contrast::contrast;
        // The tab strip's lightest and darkest backgrounds in each theme.
        type Theme<'a> = (&'a [[u8; 3]; HUES], [u8; 3], [[u8; 3]; 2]);
        let backgrounds: [Theme<'_>; 2] = [
            (&LIGHT, LIGHT_INK, [[255, 255, 255], [239, 241, 244]]),
            (&DARK, DARK_INK, [[20, 23, 28], [30, 34, 40]]),
        ];
        for (hues, ink, tabs) in backgrounds {
            for hue in hues {
                assert!(contrast(ink, *hue) >= 4.5, "{hue:?}");
                for tab in tabs {
                    assert!(contrast(*hue, tab) >= 3., "{hue:?} on {tab:?}");
                }
            }
        }
    }
}
