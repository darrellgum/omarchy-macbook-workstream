//! ~/.config/touchbar/config.toml
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Same sources as Omarchy's bar weather widget: omarchy-weather-icon + wttr.in (IP auto-location
/// unless ~/.local/state/omarchy/settings/weather.json names a place).
pub const DEFAULT_WEATHER_CMD: &str = r#"PATH=/usr/share/omarchy/bin:$PATH; q=""; if [ -s "$HOME/.local/state/omarchy/settings/weather.json" ]; then q=$(omarchy-weather-location 2>/dev/null | jq -rR @uri); fi; curl -fsS --max-time 10 "https://wttr.in/${q}?format=j1""#;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Left-to-right widget order. Known: esc, workspaces, bots, spacer, pulse, battery, clock.
    pub layout: Vec<String>,
    pub font: Option<PathBuf>,
    pub fps: u32,
    /// frame rate for the idle bot loop (blink/breathe)
    pub idle_fps: u32,
    /// /proc/interrupts names whose counts changing count as keyboard/trackpad activity
    pub activity_irqs: Vec<String>,
    /// shell command printing wttr.in `format=j1` JSON (or `ICON|TEMP`); run every weather_refresh_secs (empty = off)
    pub weather_cmd: String,
    pub weather_refresh_secs: u64,
    pub theme: ThemeCfg,
    pub workspaces: WorkspacesCfg,
    pub bots: BotsCfg,
    pub pulse: PulseCfg,
    pub clock: ClockCfg,
    pub ambient: AmbientCfg,
    pub oled: OledCfg,
    pub controls: ControlsCfg,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(default)]
pub struct ThemeCfg { pub path: Option<PathBuf> }

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct WorkspacesCfg { pub min_count: u32, pub max_count: u32 }

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct BotsCfg {
    /// Grok Bot integration: unset = auto (on when the app's roster is found), true/false to force.
    pub enabled: Option<bool>,
    pub status_dir: Option<PathBuf>,
    pub roster: Option<PathBuf>,
    /// Bot names (case-insensitive) or ids to show, in display order (empty = all bots in roster order).
    #[serde(alias = "include")]
    pub show: Vec<String>,
    /// Bot names or ids to hide.
    #[serde(alias = "exclude")]
    pub hide: Vec<String>,
    /// Names or ids drawn first, in this order; the rest follow in roster order.
    pub order: Vec<String>,
    /// A 'working' file older than this is treated as idle (0 = never).
    pub stale_after_secs: u64,
    pub mark_size: u32,
    /// read live state (unread / awaiting / working heuristic) from the app's local persistence
    pub app_state: bool,
    /// working heuristic: transcript replica rewritten within this many seconds
    pub working_recent_secs: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct PulseCfg { pub cpu: bool, pub mem: bool, pub net: bool, pub width: u32 }

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct ClockCfg { pub hour24: bool, pub seconds: bool, pub date: bool }

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct AmbientCfg { pub enabled: bool, pub idle_secs: u64, pub on_screensaver: bool }

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct ControlsCfg {
    /// Double-tap Fn opens the controls layer; Esc or `timeout_secs` without a tap closes it.
    pub enabled: bool,
    pub timeout_secs: u64,
    /// One shell command per button, run with sh -c. Empty string hides the button.
    pub brightness_down: String, pub brightness_up: String,
    pub kbd_down: String, pub kbd_up: String,
    pub mute: String, pub volume_down: String, pub volume_up: String,
}

impl Default for ControlsCfg {
    fn default() -> Self {
        let osd = |a: &str, b: &str| format!("command -v swayosd-client >/dev/null && swayosd-client {a} || {b}");
        ControlsCfg {
            enabled: true, timeout_secs: 5,
            brightness_down: osd("--brightness lower", "brightnessctl -q -c backlight set 5%-"),
            brightness_up: osd("--brightness raise", "brightnessctl -q -c backlight set +5%"),
            kbd_down: "brightnessctl -q -d '*kbd_backlight*' set 10%-".into(),
            kbd_up: "brightnessctl -q -d '*kbd_backlight*' set +10%".into(),
            mute: osd("--output-volume mute-toggle", "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"),
            volume_down: osd("--output-volume lower", "wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-"),
            volume_up: osd("--output-volume raise", "wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ 5%+"),
        }
    }
}

impl ControlsCfg {
    /// (label, command) for each visible button, in order.
    pub fn buttons(&self) -> Vec<(&'static str, &str)> {
        [("BRIGHT -", &self.brightness_down), ("BRIGHT +", &self.brightness_up), ("KEYS -", &self.kbd_down), ("KEYS +", &self.kbd_up),
         ("MUTE", &self.mute), ("VOL -", &self.volume_down), ("VOL +", &self.volume_up)]
            .into_iter().filter(|(_, c)| !c.trim().is_empty()).map(|(l, c)| (l, c.as_str())).collect()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct OledCfg { pub shift_secs: u64, pub shift_px: u32, pub dim_after_secs: u64, pub dim_level: f32 }

impl Default for Config {
    fn default() -> Self {
        Config {
            layout: ["esc", "workspaces", "bots", "activity", "spacer", "pulse", "weather", "battery", "clock"]
                .iter().map(|s| s.to_string()).collect(),
            font: None,
            fps: 30,
            idle_fps: 15,
            weather_cmd: DEFAULT_WEATHER_CMD.into(),
            weather_refresh_secs: 900,
            activity_irqs: vec!["pxa2xx-spi".into(), "i8042".into(), "applespi".into()],
            theme: ThemeCfg::default(),
            workspaces: WorkspacesCfg::default(),
            bots: BotsCfg::default(),
            pulse: PulseCfg::default(),
            clock: ClockCfg::default(),
            ambient: AmbientCfg::default(),
            oled: OledCfg::default(),
            controls: ControlsCfg::default(),
        }
    }
}
impl Default for WorkspacesCfg { fn default() -> Self { Self { min_count: 5, max_count: 10 } } }
impl Default for BotsCfg {
    fn default() -> Self { Self { enabled: None, status_dir: None, roster: None, show: vec![], hide: vec![], order: vec![], stale_after_secs: 900, mark_size: 52, app_state: true, working_recent_secs: 45 } }
}
impl Default for PulseCfg { fn default() -> Self { Self { cpu: true, mem: true, net: true, width: 130 } } }
impl Default for ClockCfg { fn default() -> Self { Self { hour24: true, seconds: false, date: true } } }
impl Default for AmbientCfg { fn default() -> Self { Self { enabled: true, idle_secs: 300, on_screensaver: true } } }
impl Default for OledCfg { fn default() -> Self { Self { shift_secs: 120, shift_px: 2, dim_after_secs: 600, dim_level: 0.8 } } }

impl Config {
    pub fn parse(text: &str) -> Result<Config, String> { toml::from_str(text).map_err(|e| e.to_string()) }
    pub fn load(path: &Path) -> Config {
        match std::fs::read_to_string(path) {
            Ok(t) => Config::parse(&t).unwrap_or_else(|e| {
                eprintln!("t1-dash: bad config {}: {e}; using defaults", path.display());
                Config::default()
            }),
            Err(_) => Config::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn example_config_parses() {
        let c = Config::parse(include_str!("../config/config.toml")).unwrap();
        assert!(c.layout.contains(&"clock".to_string()));
        assert_eq!(c.fps, 30);
    }
    #[test]
    fn partial_config_keeps_defaults() {
        let c = Config::parse("layout=[\"esc\",\"clock\"]\n[oled]\nshift_px=1\n").unwrap();
        assert_eq!(c.layout.len(), 2);
        assert_eq!(c.oled.shift_px, 1);
        assert_eq!(c.oled.shift_secs, 120);
        assert_eq!(c.bots.mark_size, 52);
        assert_eq!(c.bots.enabled, None);
        assert_eq!(Config::parse("[bots]\nenabled=false\n").unwrap().bots.enabled, Some(false));
    }
}

/// Rewrites `show` / `hide` under [bots] in config text, keeping everything else (comments included).
pub fn set_bots_lists(text: &str, show: &[String], hide: &[String]) -> String {
    let fmt = |k: &str, l: &[String]| format!("{k} = [{}]", l.iter().map(|x| format!("{:?}", x)).collect::<Vec<_>>().join(", "));
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let start = lines.iter().position(|l| l.trim() == "[bots]");
    let start = match start { Some(i) => i, None => { if !lines.is_empty() { lines.push(String::new()); } lines.push("[bots]".into()); lines.len() - 1 } };
    let end = lines.iter().enumerate().skip(start + 1).find(|(_, l)| l.trim_start().starts_with('[')).map(|(i, _)| i).unwrap_or(lines.len());
    for (keys, val) in [(["show", "include"], fmt("show", show)), (["hide", "exclude"], fmt("hide", hide))] {
        let found = (start + 1..end).find(|&i| { let k = lines[i].split('=').next().unwrap_or("").trim(); !lines[i].trim_start().starts_with('#') && keys.contains(&k) && lines[i].contains('=') });
        match found {
            Some(i) => {
                let comment = lines[i].find(" #").filter(|&c| !lines[i][..c].contains('"') || lines[i][..c].matches('"').count() % 2 == 0).map(|c| lines[i][c..].to_string()).unwrap_or_default();
                lines[i] = format!("{val}{}", if comment.is_empty() { String::new() } else { format!("  {}", comment.trim_start()) });
            }
            None => lines.insert(start + 1, val),
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod set_lists_tests {
    #[test]
    fn rewrites_and_parses() {
        let t = "[clock]\nhour24 = true\n\n[bots]\ninclude = []   # c\nexclude = [\"x\"]\nmark_size = 40\n";
        let o = super::set_bots_lists(t, &[], &["Scout".into()]);
        assert!(o.contains("hide = [\"Scout\"]") && o.contains("mark_size = 40") && o.contains("# c"));
        let c = super::Config::parse(&o).unwrap();
        assert_eq!(c.bots.hide, vec!["Scout".to_string()]);
        let o2 = super::set_bots_lists("", &[], &["A".into()]);
        assert_eq!(super::Config::parse(&o2).unwrap().bots.hide, vec!["A".to_string()]);
    }
}
