//! Small drawing helpers on top of tiny-skia + fontdue.
use crate::theme::Rgb;
use fontdue::{Font, FontSettings, Metrics};
use std::collections::HashMap;
use std::path::Path;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

static FALLBACK_FONT: &[u8] = include_bytes!("../assets/DejaVuSans-Bold.ttf");
pub const FONT_CANDIDATES: [&str; 3] = [
    "/usr/share/fonts/TTF/JetBrainsMonoNerdFont-Bold.ttf",
    "/usr/share/fonts/TTF/JetBrainsMonoNerdFont-SemiBold.ttf",
    "/usr/share/fonts/TTF/JetBrainsMono-Bold.ttf",
];

pub struct Text { font: Font, cache: HashMap<(char, u32), (Metrics, Vec<u8>)> }

impl Text {
    pub fn load(custom: Option<&Path>) -> Text {
        let mut paths: Vec<&Path> = custom.into_iter().collect();
        paths.extend(FONT_CANDIDATES.iter().map(Path::new));
        for p in paths {
            if let Ok(bytes) = std::fs::read(p) {
                if let Ok(font) = Font::from_bytes(bytes, FontSettings::default()) { return Text { font, cache: HashMap::new() }; }
            }
        }
        Text { font: Font::from_bytes(FALLBACK_FONT, FontSettings::default()).expect("fallback font"), cache: HashMap::new() }
    }
    fn glyph(&mut self, c: char, px: f32) -> &(Metrics, Vec<u8>) {
        let key = (c, (px * 4.0) as u32);
        let font = &self.font;
        self.cache.entry(key).or_insert_with(|| font.rasterize(c, px))
    }
    pub fn width(&mut self, s: &str, px: f32) -> f32 { s.chars().map(|c| self.glyph(c, px).0.advance_width).sum() }
    /// Draws `s` with its vertical center at `cy` (cap-height centered); returns advance.
    pub fn draw(&mut self, pm: &mut Pixmap, s: &str, x: f32, cy: f32, px: f32, color: Rgb, alpha: f32) -> f32 {
        let baseline = (cy + px * 0.36).round();
        let mut pen = x;
        let (w, h) = (pm.width() as i32, pm.height() as i32);
        for ch in s.chars() {
            let (m, bmp) = self.glyph(ch, px).clone();
            let gx = (pen + m.xmin as f32).round() as i32;
            let gy = baseline as i32 - m.height as i32 - m.ymin;
            let data = pm.data_mut();
            for row in 0..m.height as i32 {
                let y = gy + row;
                if y < 0 || y >= h { continue; }
                for col in 0..m.width as i32 {
                    let xx = gx + col;
                    if xx < 0 || xx >= w { continue; }
                    let a = bmp[(row * m.width as i32 + col) as usize] as f32 / 255.0 * alpha;
                    if a <= 0.0 { continue; }
                    let i = ((y * w + xx) * 4) as usize;
                    for (k, c) in [color.0, color.1, color.2].iter().enumerate() {
                        data[i + k] = (data[i + k] as f32 * (1.0 - a) + *c as f32 * a).round() as u8;
                    }
                    data[i + 3] = 255;
                }
            }
            pen += m.advance_width;
        }
        pen - x
    }
    pub fn draw_centered(&mut self, pm: &mut Pixmap, s: &str, cx: f32, cy: f32, px: f32, color: Rgb, alpha: f32) {
        let w = self.width(s, px);
        self.draw(pm, s, cx - w / 2.0, cy, px, color, alpha);
    }
}

pub fn paint(c: Rgb, a: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.anti_alias = true;
    p.set_color_rgba8(c.0, c.1, c.2, (a.clamp(0.0, 1.0) * 255.0) as u8);
    p
}

pub fn rrect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgb, a: f32) {
    if let Some(p) = rrect_path(x, y, w, h, r) { pm.fill_path(&p, &paint(c, a), FillRule::Winding, Transform::identity(), None); }
}
pub fn rrect_stroke(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgb, a: f32, width: f32) {
    if let Some(p) = rrect_path(x, y, w, h, r) {
        let s = Stroke { width, ..Stroke::default() };
        pm.stroke_path(&p, &paint(c, a), &s, Transform::identity(), None);
    }
}
pub fn rrect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w / 2.0).min(h / 2.0);
    let k = 0.5523 * r;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}
/// Lightning bolt (charging) centered at (cx, cy), height h, with an outline for contrast.
pub fn bolt(pm: &mut Pixmap, cx: f32, cy: f32, h: f32, outline: Rgb, fill: Rgb) {
    let pts = [(0.15, -0.5), (-0.3, 0.08), (-0.02, 0.08), (-0.15, 0.5), (0.3, -0.1), (0.02, -0.1)];
    let mut pb = PathBuilder::new();
    for (i, (x, y)) in pts.iter().enumerate() {
        let (px, py) = (cx + x * h, cy + y * h);
        if i == 0 { pb.move_to(px, py) } else { pb.line_to(px, py) }
    }
    pb.close();
    if let Some(p) = pb.finish() {
        let s = Stroke { width: 2.5, line_join: tiny_skia::LineJoin::Round, ..Stroke::default() };
        pm.stroke_path(&p, &paint(outline, 1.0), &s, Transform::identity(), None);
        pm.fill_path(&p, &paint(fill, 1.0), FillRule::Winding, Transform::identity(), None);
    }
}

pub fn rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: Rgb, a: f32) {
    if let Some(r) = Rect::from_xywh(x, y, w, h) { pm.fill_rect(r, &paint(c, a), Transform::identity(), None); }
}

/// Filled sparkline with a bright top edge. `values` in 0..1, newest last.
pub fn sparkline(pm: &mut Pixmap, values: &[f32], cap: usize, x: f32, y: f32, w: f32, h: f32, c: Rgb, a: f32) {
    if values.len() < 2 { return; }
    let step = w / (cap.max(2) - 1) as f32;
    let x0 = x + w - step * (values.len() - 1) as f32;
    let pt = |i: usize, v: f32| (x0 + step * i as f32, y + h - v.clamp(0.0, 1.0) * h);
    let mut line = PathBuilder::new();
    let mut area = PathBuilder::new();
    area.move_to(x0, y + h);
    for (i, v) in values.iter().enumerate() {
        let (px, py) = pt(i, *v);
        if i == 0 { line.move_to(px, py) } else { line.line_to(px, py) }
        area.line_to(px, py);
    }
    area.line_to(x + w, y + h);
    area.close();
    if let Some(p) = area.finish() { pm.fill_path(&p, &paint(c, 0.33 * a), FillRule::Winding, Transform::identity(), None); }
    if let Some(p) = line.finish() {
        let s = Stroke { width: 1.6, ..Stroke::default() };
        pm.stroke_path(&p, &paint(c, a), &s, Transform::identity(), None);
    }
}
