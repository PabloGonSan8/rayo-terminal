use egui::Color32;

pub struct Theme {
    pub name: &'static str,
    pub light: bool,
    pub bg: Color32,
    pub fg: Color32,
    pub cursor: Color32,
    pub selection: Color32,
    pub accent: Color32,
    /// Hand-picked colours per theme for each kind of text.
    pub command: Color32,
    pub path: Color32,
    pub branch: Color32,
    pub meta: Color32,
    pub ansi: [Color32; 16],
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

const fn pal(v: [u32; 16]) -> [Color32; 16] {
    let mut out = [Color32::BLACK; 16];
    let mut i = 0;
    while i < 16 {
        out[i] = hex(v[i]);
        i += 1;
    }
    out
}

/// UI colors derived from the terminal palette so every theme gets a coherent chrome.
impl Theme {
    fn mix(&self, amt: f32) -> Color32 {
        self.bg.lerp_to_gamma(
            if self.light {
                Color32::BLACK
            } else {
                Color32::WHITE
            },
            amt,
        )
    }
    /// Secondary surface (#111111 on the default theme).
    pub fn bg2(&self) -> Color32 {
        self.mix(0.032)
    }
    /// Hover surface.
    pub fn bg3(&self) -> Color32 {
        self.mix(0.075)
    }
    /// Hairline separators (#242424 on the default theme).
    pub fn border(&self) -> Color32 {
        self.mix(0.11)
    }
    /// Secondary text (#858585 on the default theme).
    pub fn muted(&self) -> Color32 {
        self.fg.lerp_to_gamma(self.bg, 0.36)
    }
    pub fn faint(&self) -> Color32 {
        self.fg.lerp_to_gamma(self.bg, 0.68)
    }
    /// Full-strength palette colours: status, git and paths should pop.
    pub fn success(&self) -> Color32 {
        self.ansi[2]
    }
    pub fn error(&self) -> Color32 {
        self.ansi[1]
    }
    /// Folder paths (chips, block headers).
    pub fn path(&self) -> Color32 {
        self.path
    }
    /// Git branch.
    pub fn branch(&self) -> Color32 {
        self.branch
    }
}

pub const THEMES: &[Theme] = &[
    Theme {
        name: "Rayo",
        light: false,
        bg: hex(0x080808),
        fg: hex(0xe5e5e5),
        cursor: hex(0x52c7ff),
        selection: hex(0x1d3d5c),
        accent: hex(0x52c7ff),
        command: hex(0xffffff),
        path: hex(0x52c7ff),
        branch: hex(0x5ee38f),
        meta: hex(0x8b95a7),
        ansi: pal([
            0x1e1e1e, 0xff6b7d, 0x5ee38f, 0xffcf5c, 0x5aa9ff, 0xd88cff, 0x3fdcd6, 0xd0d0d0,
            0x6a6a6a, 0xff8a98, 0x7cf0a8, 0xffdd85, 0x82bdff, 0xe4a8ff, 0x6ae9e3, 0xffffff,
        ]),
    },
    Theme {
        name: "Grafito",
        light: false,
        bg: hex(0x16181d),
        fg: hex(0xd8dee9),
        cursor: hex(0x5ec4ff),
        selection: hex(0x2f4a66),
        accent: hex(0x5ec4ff),
        command: hex(0xf0f4fa),
        path: hex(0xc792ea),
        branch: hex(0x8fd18b),
        meta: hex(0x7f8ba0),
        ansi: pal([
            0x1d2027, 0xf0717a, 0x8fd18b, 0xf2c97d, 0x6cb6ff, 0xc792ea, 0x5fd7d7, 0xc8ccd4,
            0x5c6370, 0xff8a92, 0xa6e3a1, 0xffd88f, 0x8ccaff, 0xd8a8f5, 0x7ee8e8, 0xffffff,
        ]),
    },
    Theme {
        name: "Solarized Dark",
        light: false,
        bg: hex(0x002b36),
        fg: hex(0x93a1a1),
        cursor: hex(0x93a1a1),
        selection: hex(0x073642),
        accent: hex(0x268bd2),
        command: hex(0xfdf6e3),
        path: hex(0x268bd2),
        branch: hex(0x859900),
        meta: hex(0x657b83),
        ansi: pal([
            0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5,
            0x002b36, 0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
        ]),
    },
    Theme {
        name: "Gruvbox Dark",
        light: false,
        bg: hex(0x1d2021),
        fg: hex(0xebdbb2),
        cursor: hex(0xfe8019),
        selection: hex(0x504945),
        accent: hex(0xfe8019),
        command: hex(0xfbf1c7),
        path: hex(0x83a598),
        branch: hex(0xb8bb26),
        meta: hex(0xa89984),
        ansi: pal([
            0x282828, 0xcc241d, 0x98971a, 0xd79921, 0x458588, 0xb16286, 0x689d6a, 0xa89984,
            0x928374, 0xfb4934, 0xb8bb26, 0xfabd2f, 0x83a598, 0xd3869b, 0x8ec07c, 0xebdbb2,
        ]),
    },
    Theme {
        name: "Nord",
        light: false,
        bg: hex(0x2e3440),
        fg: hex(0xd8dee9),
        cursor: hex(0x88c0d0),
        selection: hex(0x434c5e),
        accent: hex(0x88c0d0),
        command: hex(0xeceff4),
        path: hex(0x88c0d0),
        branch: hex(0xa3be8c),
        meta: hex(0x7b88a1),
        ansi: pal([
            0x3b4252, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x88c0d0, 0xe5e9f0,
            0x4c566a, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x8fbcbb, 0xeceff4,
        ]),
    },
    Theme {
        name: "Claro",
        light: true,
        bg: hex(0xfafafa),
        fg: hex(0x383a42),
        cursor: hex(0x526fff),
        selection: hex(0xa9c6f7),
        accent: hex(0x526fff),
        command: hex(0x1f2329),
        path: hex(0x0b6bcb),
        branch: hex(0x2f8f3a),
        meta: hex(0x6b7280),
        ansi: pal([
            // Programs print "white" assuming a dark background: keep 7 / 15 readable on light.
            0x24292f, 0xcf222e, 0x116329, 0x7d4e00, 0x0969da, 0x8250df, 0x1b7c83, 0x6e7781,
            0x57606a, 0xa40e26, 0x1a7f37, 0x633c01, 0x0550ae, 0x6639ba, 0x15646b, 0x424a53,
        ]),
    },
];

/// `base` with a custom text colour, built once per (theme, colour) and reused.
// ponytail: themes are `&'static` everywhere, so variants are leaked (~150 bytes per
// distinct colour picked in settings). Move to Rc<Theme> if this ever grows.
pub fn with_fg(base: &'static Theme, rgb: [u8; 3]) -> &'static Theme {
    static CACHE: std::sync::Mutex<Vec<&'static Theme>> = std::sync::Mutex::new(Vec::new());
    let fg = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let mut cache = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(t) = cache.iter().find(|t| t.name == base.name && t.fg == fg) {
        return t;
    }
    let t: &'static Theme = Box::leak(Box::new(Theme { fg, ..*base }));
    cache.push(t);
    t
}

pub fn by_name(name: &str) -> &'static Theme {
    THEMES.iter().find(|t| t.name == name).unwrap_or(&THEMES[0])
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_theme_matches_spec() {
        let t = &super::THEMES[0];
        let near = |c: egui::Color32, v: u8| (c.r() as i32 - v as i32).abs() <= 3;
        assert!(near(t.bg2(), 0x11), "bg2 {:?}", t.bg2());
        assert!(near(t.border(), 0x24), "border {:?}", t.border());
        assert!(near(t.muted(), 0x96), "muted {:?}", t.muted());
    }
}
