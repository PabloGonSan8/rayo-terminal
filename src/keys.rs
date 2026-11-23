//! Rebindable application shortcuts. Stored in config.toml as `action = "Ctrl+Shift+D"`.

use std::collections::BTreeMap;

use egui::{Key, Modifiers};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Combo {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: Key,
}

impl Combo {
    pub fn new(key: Key, m: Modifiers) -> Self {
        Self {
            ctrl: m.ctrl,
            shift: m.shift,
            alt: m.alt,
            key,
        }
    }

    /// Storage form: "Ctrl+Shift+Comma".
    pub fn to_config(self) -> String {
        let mut s = String::new();
        for (on, name) in [
            (self.ctrl, "Ctrl+"),
            (self.shift, "Shift+"),
            (self.alt, "Alt+"),
        ] {
            if on {
                s += name;
            }
        }
        s + self.key.name()
    }

    pub fn parse(s: &str) -> Option<Self> {
        let mut c = Self {
            ctrl: false,
            shift: false,
            alt: false,
            key: Key::A,
        };
        let mut key = None;
        for part in s.split('+').map(str::trim) {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => c.ctrl = true,
                "shift" | "mayús" | "mayus" => c.shift = true,
                "alt" => c.alt = true,
                _ => key = Some(Key::from_name(part)?),
            }
        }
        c.key = key?;
        Some(c)
    }

    /// Display form: "Ctrl Shift ,".
    pub fn label(self) -> String {
        let mut v = vec![];
        if self.ctrl {
            v.push("Ctrl");
        }
        if self.shift {
            v.push("Shift");
        }
        if self.alt {
            v.push("Alt");
        }
        let k = match self.key {
            Key::ArrowLeft => "←",
            Key::ArrowRight => "→",
            Key::ArrowUp => "↑",
            Key::ArrowDown => "↓",
            Key::PageUp => crate::i18n::t("RePág"),
            Key::PageDown => crate::i18n::t("AvPág"),
            k => k.symbol_or_name(),
        };
        v.push(k);
        v.join(" ")
    }

    pub fn matches(self, key: Key, m: Modifiers) -> bool {
        // "=" and "+" share a key on many layouts (Ctrl + to zoom).
        let same_key = self.key == key
            || matches!(
                (self.key, key),
                (Key::Equals, Key::Plus) | (Key::Plus, Key::Equals)
            );
        same_key && self.ctrl == m.ctrl && self.shift == m.shift && self.alt == m.alt
    }

    /// egui turns these into Copy/Cut/Paste events, so they never arrive as keys.
    pub fn reserved(self) -> bool {
        self.ctrl && !self.shift && !self.alt && matches!(self.key, Key::C | Key::V | Key::X)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Settings,
    NewTab,
    ClosePane,
    SplitRight,
    SplitDown,
    Zoom,
    NextTab,
    PrevTab,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    Copy,
    Paste,
    FontBigger,
    FontSmaller,
    FontReset,
    ClearBlocks,
    PageUp,
    PageDown,
}

/// (action, config id, label, default)
pub const ACTIONS: &[(Action, &str, &str, &str)] = &[
    (Action::Settings, "settings", "Abrir ajustes", "Ctrl+Comma"),
    (Action::NewTab, "new_tab", "Nueva pestaña", "Ctrl+Shift+T"),
    (
        Action::ClosePane,
        "close_pane",
        "Cerrar panel / pestaña",
        "Ctrl+Shift+W",
    ),
    (
        Action::SplitRight,
        "split_right",
        "Dividir a la derecha",
        "Ctrl+Shift+D",
    ),
    (
        Action::SplitDown,
        "split_down",
        "Dividir hacia abajo",
        "Ctrl+Shift+E",
    ),
    (
        Action::Zoom,
        "zoom",
        "Maximizar / restaurar panel",
        "Ctrl+Shift+Enter",
    ),
    (Action::NextTab, "next_tab", "Siguiente pestaña", "Ctrl+Tab"),
    (
        Action::PrevTab,
        "prev_tab",
        "Pestaña anterior",
        "Ctrl+Shift+Tab",
    ),
    (
        Action::FocusLeft,
        "focus_left",
        "Foco al panel izquierdo",
        "Ctrl+Alt+Left",
    ),
    (
        Action::FocusRight,
        "focus_right",
        "Foco al panel derecho",
        "Ctrl+Alt+Right",
    ),
    (
        Action::FocusUp,
        "focus_up",
        "Foco al panel superior",
        "Ctrl+Alt+Up",
    ),
    (
        Action::FocusDown,
        "focus_down",
        "Foco al panel inferior",
        "Ctrl+Alt+Down",
    ),
    (Action::Copy, "copy", "Copiar selección", "Ctrl+Shift+C"),
    (Action::Paste, "paste", "Pegar", "Ctrl+Shift+V"),
    (
        Action::FontBigger,
        "font_bigger",
        "Aumentar letra",
        "Ctrl+Equals",
    ),
    (
        Action::FontSmaller,
        "font_smaller",
        "Reducir letra",
        "Ctrl+Minus",
    ),
    (
        Action::FontReset,
        "font_reset",
        "Tamaño de letra original",
        "Ctrl+0",
    ),
    (
        Action::ClearBlocks,
        "clear_blocks",
        "Limpiar bloques",
        "Ctrl+L",
    ),
    (
        Action::PageUp,
        "page_up",
        "Subir una página del historial",
        "Shift+PageUp",
    ),
    (
        Action::PageDown,
        "page_down",
        "Bajar una página del historial",
        "Shift+PageDown",
    ),
];

/// Upgrade bindings whose default changed, unless the user customised them.
pub fn migrate(b: &mut BTreeMap<String, String>) {
    for (id, old) in [
        ("focus_left", "Alt+Left"),
        ("focus_right", "Alt+Right"),
        ("focus_up", "Alt+Up"),
        ("focus_down", "Alt+Down"),
    ] {
        if b.get(id).map(String::as_str) == Some(old) {
            let new = ACTIONS
                .iter()
                .find(|a| a.1 == id)
                .map(|a| a.3)
                .unwrap_or(old);
            b.insert(id.to_string(), new.to_string());
        }
    }
}

pub fn defaults() -> BTreeMap<String, String> {
    ACTIONS
        .iter()
        .map(|(_, id, _, d)| (id.to_string(), d.to_string()))
        .collect()
}

/// Action bound to this key press, if any. Empty / invalid bindings are unassigned.
pub fn lookup(bindings: &BTreeMap<String, String>, key: Key, m: Modifiers) -> Option<Action> {
    ACTIONS.iter().find_map(|(a, id, _, d)| {
        let s = bindings.get(*id).map(String::as_str).unwrap_or(d);
        Combo::parse(s).filter(|c| c.matches(key, m)).map(|_| *a)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_lookup() {
        for (_, _, _, d) in ACTIONS {
            let c = Combo::parse(d).unwrap_or_else(|| panic!("bad default {d}"));
            assert_eq!(c.to_config(), *d);
        }
        let ctrl_shift = Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        let mut b = defaults();
        assert_eq!(lookup(&b, Key::D, ctrl_shift), Some(Action::SplitRight));
        b.insert("split_right".into(), "F2".into());
        assert_eq!(lookup(&b, Key::D, ctrl_shift), None);
        assert_eq!(
            lookup(&b, Key::F2, Modifiers::NONE),
            Some(Action::SplitRight)
        );
        b.insert("split_down".into(), String::new());
        assert_eq!(lookup(&b, Key::E, ctrl_shift), None);
        // Ctrl + "+" key also zooms on layouts without "=".
        assert_eq!(
            lookup(&b, Key::Plus, Modifiers::CTRL),
            Some(Action::FontBigger)
        );
        assert_eq!(Combo::parse("Ctrl+Comma").unwrap().label(), "Ctrl ,");
        assert!(Combo::parse("Ctrl+C").unwrap().reserved());
    }
}
