//! Grok Bot marks: geometry extracted at runtime from the locally installed app
//! (tools/extract-marks.cjs, cached in ~/.cache/t1-dash/marks.json) and a 2D port of its
//! idle/working animation. No app artwork is compiled in.
use crate::theme::Rgb;
use serde::Deserialize;
use std::collections::HashMap;
use std::f32::consts::PI;
use tiny_skia::{FillRule, Paint, Path, PathBuilder, Pixmap, Transform};

pub const SUCCESS_GREEN: Rgb = Rgb(0x00, 0xc9, 0x72);
const POSES: [&str; 5] = ["neutral", "tilt_l", "tilt_r", "glance_l", "glance_r"];
const DEFAULT_SHAPES: [&str; 8] = ["blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop"];
const HASH_COLORS: [&str; 10] = ["brown", "red", "orange", "yellow", "green", "cyan", "blue", "violet", "magenta", "gray"];

#[derive(Deserialize)]
struct RawColor { #[allow(dead_code)] light: String, dark: String }
#[derive(Deserialize)]
struct RawShape { scale: f32, body: String, eyes: HashMap<String, [String; 2]> }
#[derive(Deserialize)]
struct RawMarks { center: f32, colors: HashMap<String, RawColor>, shapes: HashMap<String, RawShape> }

pub struct Shape {
    pub scale: f32,
    pub body: Path,
    /// eyes[pose] = (left, right, centroid_left, centroid_right)
    pub eyes: Vec<(Path, Path, (f32, f32), (f32, f32))>,
}

/// Newer app versions: frames sampled from the app's own motion player (format 2).
pub struct Motion {
    pub fps: f32,
    pub size: f32,
    /// (body rings, eye rings) per unique frame
    pub frames: Vec<(Path, Vec<Path>)>,
    /// shape -> (idle frame indices, working frame indices, extent radius in frame units)
    pub shapes: HashMap<String, (Vec<u32>, Vec<u32>, f32)>,
}

#[derive(Deserialize)]
struct RawFrame { b: Vec<Vec<i32>>, e: Vec<Vec<i32>> }
#[derive(Deserialize)]
struct RawMotionShape { idle: Vec<u32>, working: Vec<u32> }
#[derive(Deserialize)]
struct RawMotion { size: f32, fps: f32, scale: f32, colors: HashMap<String, RawColor>, shapes: HashMap<String, RawMotionShape>, frames: Vec<RawFrame> }

fn ring(v: &[i32], k: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    for (i, c) in v.chunks_exact(2).enumerate() {
        let (x, y) = (c[0] as f32 / k, c[1] as f32 / k);
        if i == 0 { pb.move_to(x, y) } else { pb.line_to(x, y) }
    }
    pb.close();
    pb.finish()
}

pub struct Marks {
    pub motion: Option<Motion>,
    pub center: f32,
    pub colors: HashMap<String, Rgb>,
    pub shapes: HashMap<String, Shape>,
}

impl Marks {
    pub fn empty() -> Marks {
        Marks { motion: None, center: 114.2705, colors: HashMap::new(), shapes: HashMap::new() }
    }
    pub fn is_empty(&self) -> bool { self.shapes.is_empty() && self.motion.is_none() }

    /// Geometry from the cache, re-extracted from the installed app when the cache is missing or
    /// older than the app archive. Empty when the app is absent or extraction fails.
    #[cfg(feature = "grok-bot")]
    pub fn load() -> Marks {
        let asar = std::path::PathBuf::from(std::env::var_os("T1_DASH_GROK_ASAR").unwrap_or_else(|| "/opt/Grok Bot/resources/app.asar".into()));
        let cache = std::env::var_os("XDG_CACHE_HOME").map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache")).join("t1-dash");
        let out = cache.join("marks.json");
        let mtime = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let Some(app_time) = mtime(&asar) else { return Marks::empty() };
        if mtime(&out).is_none_or(|t| t < app_time) {
            let script = cache.join("extract-marks.cjs");
            let _ = std::fs::create_dir_all(&cache);
            let _ = std::fs::write(&script, include_str!("../tools/extract-marks.cjs"));
            let electron = asar.parent().and_then(|p| p.parent()).map(|p| p.join("grok-bot"));
            // success = the script exited 0 AND left a fresh cache (an Electron build with the
            // run-as-node fuse off exits 0 without running the script)
            let ran = |cmd: &mut std::process::Command| {
                cmd.arg(&script).arg(&asar).arg(&out).stdout(std::process::Stdio::null()).status().is_ok_and(|s| s.success())
                    && mtime(&out).is_some_and(|t| t >= app_time)
            };
            let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
            let ok = ran(&mut std::process::Command::new("node"))
                || ran(&mut std::process::Command::new(home.join(".local/share/mise/shims/node")))
                || electron.is_some_and(|e| ran(std::process::Command::new(e).env("ELECTRON_RUN_AS_NODE", "1")));
            if !ok { eprintln!("t1-dash: could not extract Grok Bot marks from {}", asar.display()); }
        }
        match std::fs::read_to_string(&out).map_err(|e| e.to_string()).and_then(|t| Marks::from_json(&t)) {
            Ok(m) => m,
            Err(e) => { eprintln!("t1-dash: Grok Bot marks unavailable: {e}"); Marks::empty() }
        }
    }
    #[cfg(not(feature = "grok-bot"))]
    pub fn load() -> Marks { Marks::empty() }

    /// Synthetic test geometry (plain polygons), not app artwork.
    #[cfg(test)]
    pub fn synthetic() -> Marks { Marks::from_json(include_str!("../tests/fixtures/marks-synthetic.json")).unwrap() }
    pub fn from_json(text: &str) -> Result<Marks, String> {
        #[derive(Deserialize)]
        struct Format { format: Option<u32> }
        if serde_json::from_str::<Format>(text).ok().and_then(|f| f.format) == Some(2) { return Marks::from_motion_json(text); }
        let raw: RawMarks = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut shapes = HashMap::new();
        for (name, s) in raw.shapes {
            let body = parse_svg_path(&s.body).ok_or(format!("bad body path {name}"))?;
            let mut eyes = Vec::new();
            for pose in POSES {
                let e = s.eyes.get(pose).ok_or(format!("{name}: missing pose {pose}"))?;
                let l = parse_svg_path(&e[0]).ok_or("bad eye")?;
                let r = parse_svg_path(&e[1]).ok_or("bad eye")?;
                let cl = centroid(&l);
                let cr = centroid(&r);
                eyes.push((l, r, cl, cr));
            }
            shapes.insert(name, Shape { scale: s.scale, body, eyes });
        }
        let colors = raw.colors.into_iter().filter_map(|(k, v)| Some((k, Rgb::parse(&v.dark)?))).collect();
        Ok(Marks { motion: None, center: raw.center, colors, shapes })
    }

    fn from_motion_json(text: &str) -> Result<Marks, String> {
        let raw: RawMotion = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let k = raw.scale.max(1.0);
        let frames: Vec<(Path, Vec<Path>)> = raw.frames.iter().map(|f| {
            let mut pb = PathBuilder::new();
            for r in &f.b { if let Some(p) = ring(r, k) { pb.push_path(&p); } }
            let body = pb.finish().unwrap_or_else(|| PathBuilder::from_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, 0.1, 0.1).unwrap()));
            (body, f.e.iter().filter_map(|r| ring(r, k)).collect())
        }).collect();
        let c = raw.size / 2.0;
        let mut shapes = HashMap::new();
        for (name, s) in raw.shapes {
            if s.idle.is_empty() || s.working.is_empty() || s.idle.iter().chain(&s.working).any(|&i| i as usize >= frames.len()) {
                return Err(format!("bad frames for {name}"));
            }
            // extent at rest (first idle frame) so every shape fills the same box like the app's still marks
            let b = frames[s.idle[0] as usize].0.bounds();
            let r = [c - b.left(), b.right() - c, c - b.top(), b.bottom() - c].into_iter().fold(1.0f32, f32::max);
            shapes.insert(name, (s.idle, s.working, r));
        }
        let colors = raw.colors.into_iter().filter_map(|(k, v)| Some((k, Rgb::parse(&v.dark)?))).collect();
        Ok(Marks { motion: Some(Motion { fps: raw.fps, size: raw.size, frames, shapes }), center: c, colors, shapes: HashMap::new() })
    }

    /// Dark-mode fill like the app (black renders white in dark mode).
    pub fn color(&self, id: Option<&str>, bot_id: &str) -> Rgb {
        id.and_then(|c| self.colors.get(c)).copied().unwrap_or_else(|| {
            let c = HASH_COLORS[(fnv1a(bot_id) as usize) % HASH_COLORS.len()];
            self.colors.get(c).copied().unwrap_or(Rgb(0x95, 0x95, 0x95))
        })
    }
    pub fn shape_name<'a>(&'a self, id: Option<&'a str>, bot_id: &str) -> &'a str {
        let known = |s: &str| self.shapes.contains_key(s) || self.motion.as_ref().is_some_and(|m| m.shapes.contains_key(s));
        match id { Some(s) if known(s) => s, _ => DEFAULT_SHAPES[(fnv1a(bot_id) as usize) % DEFAULT_SHAPES.len()] }
    }

    /// Format 2: draws the app's own sampled frame for this bot at time `t`. Returns false if unavailable.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_motion(&self, pm: &mut Pixmap, shape: &str, bot_id: &str, working: bool, t: f32, fill: Rgb, eye: Rgb, cx: f32, cy: f32, size: f32, alpha: f32) -> bool {
        let Some(m) = &self.motion else { return false };
        let Some((idle, work, r)) = m.shapes.get(shape) else { return false };
        let seq = if working { work } else { idle };
        let seed = fnv1a(bot_id);
        let t = t + rand01(seed, 7) * 20.0; // de-sync bots like separate app instances
        let i = seq[((t * m.fps) as usize) % seq.len()] as usize;
        let (body, eyes) = &m.frames[i];
        let k = size * 0.5 / r;
        let c = m.size / 2.0;
        let tf = Transform::from_translate(cx, cy).pre_scale(k, k).pre_translate(-c, -c);
        let mut paint = Paint::default();
        paint.anti_alias = true;
        paint.set_color_rgba8(fill.0, fill.1, fill.2, (alpha * 255.0) as u8);
        pm.fill_path(body, &paint, FillRule::Winding, tf, None);
        paint.set_color_rgba8(eye.0, eye.1, eye.2, 255);
        for e in eyes { pm.fill_path(e, &paint, FillRule::Winding, tf, None); }
        if working { working_dot(pm, cx, cy, size); }
        true
    }

    /// Draws one mark centered at (cx, cy) with box size `size` px.
    pub fn draw(&self, pm: &mut Pixmap, shape: &str, fill: Rgb, eye: Rgb, cx: f32, cy: f32, size: f32, pose: &Pose, alpha: f32) {
        let Some(s) = self.shapes.get(shape) else { return };
        let k = size / 259.0;
        let c = self.center;
        // local mark space -> screen: center, app scale, pose (squash, rotation, bob)
        let base = Transform::from_translate(cx + pose.dx, cy + pose.dy)
            .pre_rotate(pose.rotate_deg)
            .pre_scale(k * s.scale * pose.sx, k * s.scale * pose.sy)
            .pre_translate(-c, -c);
        let mut paint = Paint::default();
        paint.anti_alias = true;
        paint.set_color_rgba8(fill.0, fill.1, fill.2, (alpha * 255.0) as u8);
        pm.fill_path(&s.body, &paint, FillRule::EvenOdd, base, None);
        let (l, r, cl, cr) = &s.eyes[pose.eyes.min(s.eyes.len() - 1)];
        paint.set_color_rgba8(eye.0, eye.1, eye.2, 255);
        for (p, ctr) in [(l, cl), (r, cr)] {
            let t = base
                .pre_translate(ctr.0 + pose.gaze.0, ctr.1 + pose.gaze.1)
                .pre_scale(pose.eye_scale.0, pose.eye_scale.1 * pose.blink)
                .pre_translate(-ctr.0, -ctr.1);
            if pose.blink > 0.02 { pm.fill_path(p, &paint, FillRule::EvenOdd, t, None); }
        }
        if pose.working { working_dot(pm, cx, cy, size); }
    }
}

fn working_dot(pm: &mut Pixmap, cx: f32, cy: f32, size: f32) {
    let mut paint = Paint::default();
    paint.anti_alias = true;
    {
        {
            let d = (size * 0.2).max(6.0);
            let (x, y) = (cx + size / 2.0 - d / 2.0 - 1.0, cy + size / 2.0 - d / 2.0 - 1.0);
            if let Some(ring) = PathBuilder::from_circle(x, y, d / 2.0 + 1.5) {
                paint.set_color_rgba8(0, 0, 0, 255);
                pm.fill_path(&ring, &paint, FillRule::Winding, Transform::identity(), None);
            }
            if let Some(dot) = PathBuilder::from_circle(x, y, d / 2.0) {
                let g = SUCCESS_GREEN;
                paint.set_color_rgba8(g.0, g.1, g.2, 255);
                pm.fill_path(&dot, &paint, FillRule::Winding, Transform::identity(), None);
            }
        }
    }
}

/// Animated pose for one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub dx: f32, pub dy: f32, pub rotate_deg: f32, pub sx: f32, pub sy: f32,
    pub eyes: usize, pub gaze: (f32, f32), pub eye_scale: (f32, f32), pub blink: f32, pub working: bool,
}

impl Pose {
    pub fn rest() -> Pose {
        Pose { dx: 0.0, dy: 0.0, rotate_deg: 0.0, sx: 1.0, sy: 1.0, eyes: 0, gaze: (0.0, 0.0), eye_scale: (1.0, 1.0), blink: 1.0, working: false }
    }
}

/// Deterministic per-bot pseudo-random in [0,1).
fn rand01(seed: u32, n: u64) -> f32 {
    let mut x = (seed as u64) ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33; x = x.wrapping_mul(0xff51_afd7_ed55_8ccd); x ^= x >> 33;
    (x % 10_000) as f32 / 10_000.0
}

/// Finds which jittered interval [lo,hi] seconds `t` falls in; returns (index, time since its start).
fn schedule(seed: u32, t: f32, lo: f32, hi: f32) -> (u64, f32) {
    let mean = (lo + hi) / 2.0;
    let mut i = (t / mean).floor().max(0.0) as u64;
    i = i.saturating_sub(2);
    let mut start = i as f32 * mean + (rand01(seed, i) - 0.5) * (hi - lo) * 0.5;
    loop {
        let next = (i + 1) as f32 * mean + (rand01(seed, i + 1) - 0.5) * (hi - lo) * 0.5;
        if t < next || i > 1_000_000 { return (i, t - start); }
        i += 1; start = next;
    }
}

/// App blink keyframes: 0→70ms 0.05, →150ms 1.08, →300ms 1.
pub fn blink_curve(dt: f32) -> f32 {
    let ms = dt * 1000.0;
    if ms < 0.0 || ms >= 300.0 { 1.0 }
    else if ms < 70.0 { 1.0 + (0.05 - 1.0) * (ms / 70.0) }
    else if ms < 150.0 { 0.05 + (1.08 - 0.05) * ((ms - 70.0) / 80.0) }
    else { 1.08 + (1.0 - 1.08) * ((ms - 150.0) / 150.0) }
}

pub fn fnv1a(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.bytes() { h ^= b as u32; h = h.wrapping_mul(0x0100_0193); }
    h
}

/// `t` = seconds since renderer start; `size` = mark px (gaze/bob scale with it).
pub fn animate(bot_id: &str, working: bool, t: f32, size: f32) -> (Pose, bool) {
    let seed = fnv1a(bot_id);
    let t = t + rand01(seed, 7) * 20.0; // de-sync bots
    let mut p = Pose::rest();
    let px = 259.0 / size; // screen px -> mark units
    let (blo, bhi) = if working { (2.8, 5.5) } else { (6.0, 14.0) };
    let (bi, bdt) = schedule(seed ^ 0xb1, t, blo, bhi);
    p.blink = blink_curve(bdt);
    if rand01(seed ^ 0xd0, bi) < 0.14 { p.blink = p.blink.min(blink_curve(bdt - 0.32)); }
    let mut active = bdt < 0.7;
    if working {
        let ct = (t * 2.0 * PI * 1.6).sin();
        p.working = true;
        p.rotate_deg = 4.0 + 2.5 * ct - 4.0; // tilt sway around rest
        p.sy = 1.0 - ct.max(0.0) * 0.02;
        p.dy = -ct.max(0.0) * 1.2;
        p.eye_scale = (1.05 * 1.12, 1.05 * 1.02);
        // gaze shifts every 1.2–2.4 s: x ∈ ±0.4·15, y ∈ 0.4–1·9 (mark units)
        let (gi, _) = schedule(seed ^ 0x6a, t, 1.2, 2.4);
        p.gaze = ((rand01(seed, gi) * 2.0 - 1.0) * 0.4 * 15.0, (0.4 + 0.6 * rand01(seed ^ 3, gi)) * 9.0 - 6.0);
        p.eyes = [1, 2, 3, 4, 0][(gi % 5) as usize];
        // hop every 6–9 s, 380 ms parabola
        let (_, hdt) = schedule(seed ^ 0x40, t, 6.0, 9.0);
        if hdt < 0.38 { let u = hdt / 0.38; p.dy -= (u * PI).sin() * size * 0.16; p.sy *= 1.0 + 0.04 * (u * PI).sin(); }
        let _ = px;
        active = true;
    } else {
        // idle expression dwell 9–16 s: mostly neutral, sometimes a glance
        let (ei, _) = schedule(seed ^ 0xe1, t, 9.0, 16.0);
        p.eyes = match (rand01(seed ^ 0xe2, ei) * 4.0) as u32 { 0 => 3, 1 => 4, _ => 0 };
        // visible breathe + gentle sway so idle marks are clearly alive
        let br = (t * 2.0 * PI * 0.3).sin();
        p.sy = 1.0 + 0.035 * br;
        p.sx = 1.0 - 0.015 * br;
        p.dy = -br * size * 0.03;
        p.rotate_deg = (t * 2.0 * PI * 0.11).sin() * 3.0;
    }
    (p, active)
}

fn centroid(p: &Path) -> (f32, f32) {
    let b = p.bounds();
    ((b.left() + b.right()) / 2.0, (b.top() + b.bottom()) / 2.0)
}

/// Minimal absolute-command SVG path parser (M L C Q Z, as emitted by the app).
pub fn parse_svg_path(d: &str) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let mut nums: Vec<f32> = Vec::new();
    let mut cmd = ' ';
    let flush = |cmd: char, nums: &mut Vec<f32>, pb: &mut PathBuilder| -> Option<()> {
        let n = match cmd { 'M' | 'L' => 2, 'C' => 6, 'Q' => 4, _ => 0 };
        if n == 0 { if !nums.is_empty() { return None; } return Some(()); }
        if nums.len() % n != 0 { return None; }
        for (i, c) in nums.chunks(n).enumerate() {
            match (cmd, i) {
                ('M', 0) => pb.move_to(c[0], c[1]),
                ('M', _) | ('L', _) => pb.line_to(c[0], c[1]),
                ('C', _) => pb.cubic_to(c[0], c[1], c[2], c[3], c[4], c[5]),
                ('Q', _) => pb.quad_to(c[0], c[1], c[2], c[3]),
                _ => {}
            }
        }
        nums.clear();
        Some(())
    };
    let mut tok = String::new();
    let push = |tok: &mut String, nums: &mut Vec<f32>| -> Option<()> {
        if !tok.is_empty() { nums.push(tok.parse().ok()?); tok.clear(); }
        Some(())
    };
    for ch in d.chars() {
        match ch {
            'M' | 'L' | 'C' | 'Q' | 'Z' | 'z' => {
                push(&mut tok, &mut nums)?;
                flush(cmd, &mut nums, &mut pb)?;
                if ch == 'Z' || ch == 'z' { pb.close(); cmd = ' '; } else { cmd = ch; }
            }
            '-' if !tok.is_empty() && !tok.ends_with('e') => { push(&mut tok, &mut nums)?; tok.push('-'); }
            ' ' | ',' | '\n' | '\t' => push(&mut tok, &mut nums)?,
            c if c.is_ascii_digit() || c == '.' || c == 'e' || c == '-' => tok.push(c),
            _ => return None,
        }
    }
    push(&mut tok, &mut nums)?;
    flush(cmd, &mut nums, &mut pb)?;
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_18_shapes_load() {
        let m = Marks::synthetic();
        assert_eq!(m.shapes.len(), 18);
        assert_eq!(m.colors["orange"], Rgb(0xFF, 0x67, 0x00));
        assert_eq!(m.colors["black"], Rgb(0xFF, 0xFF, 0xFF));
        for s in m.shapes.values() { assert_eq!(s.eyes.len(), 5); }
    }
    #[test]
    fn svg_parser() {
        let p = parse_svg_path("M0 0L10 0L10 10Z M2 2C3 3 4 4 5 5Z").unwrap();
        assert_eq!(p.bounds().right(), 10.0);
        assert!(parse_svg_path("M0 0 X").is_none());
        assert!(parse_svg_path("M1-2L3-4").is_some());
    }
    #[test]
    fn blink_keyframes() {
        assert!((blink_curve(0.07) - 0.05).abs() < 1e-3);
        assert!((blink_curve(0.15) - 1.08).abs() < 1e-3);
        assert_eq!(blink_curve(0.5), 1.0);
    }
    #[test]
    fn working_pose_moves_and_has_dot() {
        let a: Vec<Pose> = (0..30).map(|i| animate("atlas", true, i as f32 * 0.033, 40.0).0).collect();
        assert!(a.iter().all(|p| p.working));
        assert!(a.iter().any(|p| p.rotate_deg != a[0].rotate_deg));
        let (idle, _) = animate("atlas", false, 1.0, 40.0);
        assert!(!idle.working);
        let (idle2, _) = animate("atlas", false, 2.0, 40.0);
        assert_ne!(idle.sy, idle2.sy, "idle breathe must move");
    }
    #[test]
    fn idle_blinks_within_14s() {
        let blinked = (0..14 * 60).any(|i| animate("cobalt", false, i as f32 / 60.0, 40.0).0.blink < 0.5);
        assert!(blinked);
    }
}

#[cfg(test)]
mod motion_tests {
    use super::*;
    #[test]
    fn motion_format_draws_sampled_frames() {
        let m = Marks::from_json(include_str!("../tests/fixtures/marks-motion-synthetic.json")).unwrap();
        assert!(!m.is_empty());
        assert_eq!(m.shape_name(Some("ring"), "x"), "ring");
        let mut a = Pixmap::new(80, 80).unwrap();
        assert!(m.draw_motion(&mut a, "ring", "bot", false, 0.0, Rgb(255, 255, 255), Rgb(0, 0, 0), 40.0, 40.0, 60.0, 1.0));
        assert!(a.pixel(40, 30).unwrap().red() > 200, "body filled");
        assert!(!m.draw_motion(&mut a, "missing", "bot", false, 0.0, Rgb(255, 255, 255), Rgb(0, 0, 0), 40.0, 40.0, 60.0, 1.0));
    }
}
