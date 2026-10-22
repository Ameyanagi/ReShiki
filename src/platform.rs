//! The keyboard conventions of the system the editor runs on. The desktop app
//! knows its system when it is built; in a browser the page's navigator says.
//!
//! In a browser on a Mac, the page swaps ⌘ and Control before the editor sees
//! a key or pointer event (see web.rs), so ⌘ arrives as Control and Control
//! as the logo key. iced's `Modifiers::COMMAND` is Control in every browser,
//! so its Command shortcuts and text fields then follow ⌘ as on the desktop.
use iced::keyboard::{
    Key, Modifiers,
    key::{Code, Physical},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Os {
    Mac,
    Windows,
    Other,
}

impl Os {
    /// The system a browser names in `navigator.userAgentData.platform` or
    /// `navigator.platform`: "macOS", "MacIntel", "iPad", "Win32", "Windows",
    /// "Linux x86_64".
    #[cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]
    pub(crate) fn from_name(name: &str) -> Self {
        if ["Mac", "iPhone", "iPad", "iPod"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            Self::Mac
        } else if name.starts_with("Win") {
            Self::Windows
        } else {
            Self::Other
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn os() -> Os {
    if cfg!(target_os = "macos") {
        Os::Mac
    } else if cfg!(windows) {
        Os::Windows
    } else {
        Os::Other
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn os() -> Os {
    thread_local! {
        static OS: Os = Os::from_name(&crate::web::platform_name());
    }
    OS.with(|os| *os)
}

pub(crate) fn mac() -> bool {
    os() == Os::Mac
}

pub(crate) fn windows() -> bool {
    os() == Os::Windows
}

/// The modifier the Control key reports: the logo key in a browser on a Mac,
/// where it swaps places with ⌘.
pub(crate) fn control_modifier() -> Modifiers {
    if cfg!(target_arch = "wasm32") && mac() {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    }
}

/// Whether the Control key itself is held, as for Control-drag gestures.
pub(crate) fn control(modifiers: Modifiers) -> bool {
    modifiers.contains(control_modifier())
}

/// The key a shortcut means, before Shift or Option changed its character.
///
/// The desktop app gets this from the system, but in a browser iced reports
/// the typed character: Shift+6 arrives as "^" and Option+C as "ç" on a Mac.
/// Digits and letters that are not already a Latin letter come from the
/// physical key; punctuation does too while Shift is held or the character is
/// not ASCII. Other layouts keep their own letters and punctuation.
pub(crate) fn shortcut_key(key: &Key, physical: Physical, modifiers: Modifiers) -> Key {
    let Key::Character(typed) = key else {
        return key.clone();
    };
    let Physical::Code(code) = physical else {
        return key.clone();
    };
    let ascii = typed.len() == 1 && typed.is_ascii();
    let base = match code {
        Code::Digit0 => '0',
        Code::Digit1 => '1',
        Code::Digit2 => '2',
        Code::Digit3 => '3',
        Code::Digit4 => '4',
        Code::Digit5 => '5',
        Code::Digit6 => '6',
        Code::Digit7 => '7',
        Code::Digit8 => '8',
        Code::Digit9 => '9',
        _ => {
            if let Some(letter) = letter(code) {
                if ascii && typed.chars().all(|c| c.is_ascii_alphabetic()) {
                    return Key::Character(typed.to_ascii_lowercase().into());
                }
                letter
            } else if let Some(punctuation) = punctuation(code)
                && (!ascii || modifiers.shift())
            {
                punctuation
            } else {
                return key.clone();
            }
        }
    };
    Key::Character(base.to_string().into())
}

fn letter(code: Code) -> Option<char> {
    Some(match code {
        Code::KeyA => 'a',
        Code::KeyB => 'b',
        Code::KeyC => 'c',
        Code::KeyD => 'd',
        Code::KeyE => 'e',
        Code::KeyF => 'f',
        Code::KeyG => 'g',
        Code::KeyH => 'h',
        Code::KeyI => 'i',
        Code::KeyJ => 'j',
        Code::KeyK => 'k',
        Code::KeyL => 'l',
        Code::KeyM => 'm',
        Code::KeyN => 'n',
        Code::KeyO => 'o',
        Code::KeyP => 'p',
        Code::KeyQ => 'q',
        Code::KeyR => 'r',
        Code::KeyS => 's',
        Code::KeyT => 't',
        Code::KeyU => 'u',
        Code::KeyV => 'v',
        Code::KeyW => 'w',
        Code::KeyX => 'x',
        Code::KeyY => 'y',
        Code::KeyZ => 'z',
        _ => return None,
    })
}

/// The unshifted character of a punctuation key on a US keyboard.
fn punctuation(code: Code) -> Option<char> {
    Some(match code {
        Code::Minus => '-',
        Code::Equal => '=',
        Code::BracketLeft => '[',
        Code::BracketRight => ']',
        Code::Backslash => '\\',
        Code::Semicolon => ';',
        Code::Quote => '\'',
        Code::Comma => ',',
        Code::Period => '.',
        Code::Slash => '/',
        Code::Backquote => '`',
        _ => return None,
    })
}

/// A browser key event with its key as `shortcut_key` reads it, so shortcuts
/// match as on the desktop. The typed text is unchanged. None elsewhere, or
/// when nothing changes.
pub(crate) fn shortcut_event(event: &iced::Event) -> Option<iced::Event> {
    use iced::keyboard::Event::{KeyPressed, KeyReleased};
    if !cfg!(target_arch = "wasm32") {
        return None;
    }
    let iced::Event::Keyboard(keyboard) = event else {
        return None;
    };
    let mut keyboard = keyboard.clone();
    let (KeyPressed {
        key,
        physical_key,
        modifiers,
        ..
    }
    | KeyReleased {
        key,
        physical_key,
        modifiers,
        ..
    }) = &mut keyboard
    else {
        return None;
    };
    let base = shortcut_key(key, *physical_key, *modifiers);
    if base == *key {
        return None;
    }
    *key = base;
    Some(iced::Event::Keyboard(keyboard))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn char(c: &str) -> Key {
        Key::Character(c.into())
    }

    #[test]
    fn browsers_name_their_system() {
        for (name, os) in [
            ("macOS", Os::Mac),
            ("MacIntel", Os::Mac),
            ("iPad", Os::Mac),
            ("iPhone", Os::Mac),
            ("Windows", Os::Windows),
            ("Win32", Os::Windows),
            ("Linux x86_64", Os::Other),
            ("Linux", Os::Other),
            ("", Os::Other),
        ] {
            assert_eq!(Os::from_name(name), os, "{name}");
        }
    }

    #[test]
    fn shift_digits_read_as_the_digit() {
        for (typed, code, digit) in [
            ("#", Code::Digit3, "3"),
            ("^", Code::Digit6, "6"),
            ("*", Code::Digit8, "8"),
            ("6", Code::Digit6, "6"),
        ] {
            assert_eq!(
                shortcut_key(&char(typed), Physical::Code(code), Modifiers::SHIFT),
                char(digit)
            );
        }
    }

    #[test]
    fn option_letters_read_as_the_letter() {
        let option = Modifiers::CTRL | Modifiers::ALT;
        for (typed, code, letter) in [
            ("ç", Code::KeyC, "c"),
            ("ø", Code::KeyO, "o"),
            ("˚", Code::KeyK, "k"),
            ("≈", Code::KeyX, "x"),
            ("Ç", Code::KeyC, "c"),
        ] {
            assert_eq!(
                shortcut_key(&char(typed), Physical::Code(code), option),
                char(letter)
            );
        }
    }

    #[test]
    fn layouts_keep_their_own_letters() {
        // AZERTY: the key in QWERTY's Q position types a.
        assert_eq!(
            shortcut_key(&char("a"), Physical::Code(Code::KeyQ), Modifiers::CTRL),
            char("a")
        );
        assert_eq!(
            shortcut_key(&char("Z"), Physical::Code(Code::KeyZ), Modifiers::SHIFT),
            char("z")
        );
    }

    #[test]
    fn punctuation_reads_unshifted() {
        let shift = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(
            shortcut_key(&char("?"), Physical::Code(Code::Slash), shift),
            char("/")
        );
        assert_eq!(
            shortcut_key(
                &char("“"),
                Physical::Code(Code::BracketLeft),
                Modifiers::ALT
            ),
            char("[")
        );
        // Unshifted ASCII stays as the layout types it.
        assert_eq!(
            shortcut_key(&char("-"), Physical::Code(Code::Slash), Modifiers::CTRL),
            char("-")
        );
    }

    #[test]
    fn named_keys_are_unchanged() {
        use iced::keyboard::key::Named;
        let enter = Key::Named(Named::Enter);
        assert_eq!(
            shortcut_key(&enter, Physical::Code(Code::Enter), Modifiers::CTRL),
            enter
        );
    }
}
