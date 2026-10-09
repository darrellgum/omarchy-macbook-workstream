//! Omarchy theme colors (colors.toml).
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn parse(s: &str) -> Option<Rgb> {
        let h = s.trim().trim_start_matches('#');
        if h.len() < 6 { return None; }
        let v = u32::from_str_radix(&h[..6], 16).ok()?;
        Some(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
    pub fn mix(self, o: Rgb, t: f32) -> Rgb {
        let f = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)).round() as u8;
        Rgb(f(self.0, o.0), f(self.1, o.1), f(self.2, o.2))
    }
    #[allow(dead_code)]
    pub fn scale(self, k: f32) -> Rgb { Rgb::mix(Rgb(0, 0, 0), self, k) }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub accent: Rgb,
    pub background: Rgb,
    pub foreground: Rgb,
    pub bright_foreground: Rgb,
    pub dim_foreground: Rgb,
    pub muted: Rgb,
    pub selection: Rgb,
    pub red: Rgb,
    pub green: Rgb,
    pub yellow: Rgb,
    pub cyan: Rgb,
    pub magenta: Rgb,
    pub blue: Rgb,
}

impl Default for Theme {
    fn default() -> Self { Theme::parse("") }
}

impl Theme {
    /// Parses colors.toml; missing keys fall back to Tokyo Night.
    pub fn parse(text: &str) -> Theme {
        let table: toml::Table = text.parse().unwrap_or_default();
        let get = |keys: &[&str], def: &str| {
            keys.iter()
                .find_map(|k| table.get(*k).and_then(|v| v.as_str()).and_then(Rgb::parse))
                .unwrap_or_else(|| Rgb::parse(def).unwrap())
        };
        Theme {
            accent: get(&["accent", "blue"], "#7aa2f7"),
            background: get(&["background"], "#1a1b26"),
            foreground: get(&["foreground"], "#a9b1d6"),
            bright_foreground: get(&["bright_foreground", "light_foreground", "foreground"], "#c0caf5"),
            dim_foreground: get(&["dark_foreground", "muted"], "#565f89"),
            muted: get(&["muted", "dark_foreground"], "#414868"),
            selection: get(&["selection", "lighter_background"], "#292e42"),
            red: get(&["red"], "#f7768e"),
            green: get(&["green"], "#9ece6a"),
            yellow: get(&["yellow"], "#e0af68"),
            cyan: get(&["bright_cyan", "cyan"], "#0db9d7"),
            magenta: get(&["bright_magenta", "magenta"], "#bb9af7"),
            blue: get(&["blue", "accent"], "#7aa2f7"),
        }
    }

    #[allow(dead_code)]
    pub fn load(path: &Path) -> Option<Theme> {
        std::fs::read_to_string(path).ok().map(|t| Theme::parse(&t))
    }
}

/// Omarchy 4 keeps the theme in ~/.local/state; 3.x used ~/.config/omarchy/current.
pub fn default_theme_paths(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".local/state/omarchy/current/theme/colors.toml"),
        home.join(".config/omarchy/current/theme/colors.toml"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_fixture_and_defaults() {
        let t = Theme::parse(include_str!("../tests/fixtures/colors.toml"));
        assert_eq!(t.accent, Rgb(0x7a, 0xa2, 0xf7));
        assert_eq!(t.cyan, Rgb(0x0d, 0xb9, 0xd7));
        let t2 = Theme::parse("accent = \"#ff0000\"\n");
        assert_eq!(t2.accent, Rgb(255, 0, 0));
        assert_eq!(t2.green, Rgb(0x9e, 0xce, 0x6a));
        assert_eq!(Theme::parse("garbage = = ="), Theme::default());
    }
}
