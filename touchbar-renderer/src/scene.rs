//! Pure scene description + renderer + hit testing (no I/O; used live and for previews).
use crate::bots::Bot;
use crate::config::Config;
use crate::draw::{self, Text};
use crate::hypr::Workspace;
use crate::marks::{self, Marks};
use crate::sysmon::{self, Battery};
use crate::theme::{Rgb, Theme};
use tiny_skia::{PathBuilder, Pixmap};

pub const FKEYS: [&str; 13] = ["esc", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit { Esc, FKey(usize), Ctl(usize), CloseControls, Workspace(i64), Bot(usize), WakeAmbient, CancelTouchId }

#[derive(Clone, Debug, Default)]
pub struct LocalTime { pub hour: u32, pub min: u32, pub sec: u32, pub wday: u32, pub mday: u32, pub mon: u32 }

impl LocalTime {
    pub fn now() -> LocalTime {
        unsafe {
            let t = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            libc::localtime_r(&t, &mut tm);
            LocalTime { hour: tm.tm_hour as u32, min: tm.tm_min as u32, sec: tm.tm_sec as u32, wday: tm.tm_wday as u32, mday: tm.tm_mday as u32, mon: tm.tm_mon as u32 }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TouchId { Authenticate, Approve, Retry, Success, Enrollment(u8) }

pub struct Scene {
    pub width: u32,
    pub height: u32,
    pub cfg: Config,
    pub theme: Theme,
    pub bots: Vec<(Bot, bool)>,
    /// generic status-dir activity (any tool)
    pub activities: Vec<crate::bots::Activity>,
    pub workspaces: Vec<Workspace>,
    pub active_ws: i64,
    pub cpu: Vec<f32>, pub mem: Vec<f32>, pub net: Vec<f32>,
    pub net_rate: f64, pub mem_gib: f32, pub mem_total_gib: f32,
    pub cpu_temp: Option<f32>,
    /// (nerd-font icon, "62°F")
    pub weather: Option<(String, String)>,
    pub battery: Battery,
    pub time: LocalTime,
    pub fn_pressed: bool,
    /// hidden controls layer (double-tap Fn)
    pub controls: bool,
    pub ambient: bool,
    /// seconds since last touch / desktop activity
    pub idle_secs: f32,
    /// animation clock, seconds
    pub t: f32,
    /// wall clock seconds (OLED shift schedule)
    pub unix_secs: u64,
    pub touch_id: Option<TouchId>,
    pub pressed: Option<Hit>,
}

/// `animating`: needs fast frames (working/ambient); `idle_anim`: idle loop (blink/breathe).
pub struct Frame { pub hits: Vec<([f32; 4], Hit)>, pub animating: bool, pub idle_anim: bool }

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// OLED pixel shift: slow walk over a small square, one step per `shift_secs`.
pub fn oled_offset(unix_secs: u64, shift_secs: u64, px: u32) -> (f32, f32) {
    if shift_secs == 0 || px == 0 { return (0.0, 0.0); }
    let p = px as i64;
    let ring: Vec<(i64, i64)> = (0..=p).map(|x| (x, 0)).chain((1..=p).map(|y| (p, y))).chain((0..p).rev().map(|x| (x, p))).chain((1..p).rev().map(|y| (0, y))).collect();
    let (x, y) = ring[((unix_secs / shift_secs) as usize) % ring.len()];
    ((x - p / 2) as f32, (y - p / 2) as f32)
}

impl Scene {
    pub fn workspace_ids(&self) -> Vec<(i64, u32)> {
        let mut ids: Vec<(i64, u32)> = (1..=self.cfg.workspaces.min_count as i64).map(|i| (i, 0)).collect();
        for w in &self.workspaces {
            match ids.iter_mut().find(|(i, _)| *i == w.id) { Some(e) => e.1 = w.windows, None => ids.push((w.id, w.windows)) }
        }
        if !ids.iter().any(|(i, _)| *i == self.active_ws) && self.active_ws > 0 { ids.push((self.active_ws, 0)); }
        ids.sort();
        ids.truncate(self.cfg.workspaces.max_count.max(1) as usize);
        ids
    }

    fn widget_width(&self, name: &str) -> f32 {
        let ms = self.cfg.bots.mark_size as f32;
        match name {
            "esc" => 116.0,
            "workspaces" => self.workspace_ids().len() as f32 * 66.0 + 18.0,
            "activity" => if self.activities.is_empty() { 0.0 } else { self.activities.iter().map(|a| 70.0 + a.label.chars().count() as f32 * 13.0).sum::<f32>() + 14.0 },
            "bots" => if self.bots.is_empty() { 0.0 } else { self.bots.len() as f32 * (ms + 14.0) + 22.0 },
            "pulse" => { let p = &self.cfg.pulse; let pw = p.width as f32; 14.0 + if p.cpu { 136.0 + pw } else { 0.0 } + if p.mem { 132.0 } else { 0.0 } + if p.net { 104.0 + pw + 6.0 } else { 0.0 } }
            "weather" => if self.weather.is_some() { 128.0 } else { 0.0 },
            "battery" => if self.battery.present { 132.0 } else { 0.0 },
            "clock" => 262.0,
            _ => 0.0,
        }
    }

    pub fn render(&self, pm: &mut Pixmap, marks: &Marks, text: &mut Text) -> Frame {
        pm.fill(tiny_skia::Color::BLACK);
        let mut hits = Vec::new();
        if let Some(tid) = &self.touch_id { self.render_touch_id(pm, text, tid, &mut hits); return Frame { hits, animating: true, idle_anim: false }; }
        if self.controls { self.render_controls(pm, text, &mut hits); return Frame { hits, animating: false, idle_anim: false }; }
        if self.fn_pressed { self.render_fn(pm, text, &mut hits); return Frame { hits, animating: false, idle_anim: false }; }
        if self.ambient { self.render_ambient(pm, text, &mut hits); return Frame { hits, animating: true, idle_anim: false }; }

        let th0 = self.theme.clone();
        let th = &brighten(&th0);
        let (ox, oy) = oled_offset(self.unix_secs, self.cfg.oled.shift_secs, self.cfg.oled.shift_px);
        let dimmed = self.cfg.oled.dim_after_secs > 0 && self.idle_secs >= self.cfg.oled.dim_after_secs as f32;
        let dim = if dimmed { self.cfg.oled.dim_level.clamp(0.1, 1.0) } else { 1.0 };
        let h = self.height as f32;
        let cy = h / 2.0 + oy;
        let mut animating = false;

        let layout: Vec<&str> = self.cfg.layout.iter().map(String::as_str).collect();
        let fixed: f32 = layout.iter().map(|w| self.widget_width(w)).sum();
        let spacers = layout.iter().filter(|w| **w == "spacer").count().max(1) as f32;
        let spacer_w = ((self.width as f32 - 16.0 - fixed) / spacers).max(0.0);
        let mut x = 8.0 + ox;
        let mut prev_drawn = false;
        for name in layout {
            let w = if name == "spacer" { spacer_w } else { self.widget_width(name) };
            if w <= 0.0 { continue; }
            if name != "spacer" && prev_drawn && matches!(name, "bots" | "activity" | "pulse") {
                draw::rect(pm, x, cy - 20.0, 1.5, 40.0, th.muted, dim);
            }
            match name {
                "esc" => {
                    let pressed = self.pressed == Some(Hit::Esc);
                    draw::rrect(pm, x, cy - 26.0, 100.0, 52.0, 10.0, th.selection, if pressed { 1.0 } else { dim });
                    if pressed { draw::rrect_stroke(pm, x + 0.5, cy - 25.5, 99.0, 51.0, 10.0, th.accent, 1.0, 2.0); }
                    text.draw_centered(pm, "esc", x + 50.0, cy, 27.0, th.bright_foreground, dim);
                    hits.push(([x, 0.0, 106.0, h], Hit::Esc));
                }
                "workspaces" => {
                    let mut px = x + 10.0;
                    for (id, windows) in self.workspace_ids() {
                        let active = id == self.active_ws;
                        let (pw, ph) = (58.0, 50.0);
                        if active {
                            for (g, a) in [(7.0, 0.07), (5.0, 0.12), (3.0, 0.2), (1.5, 0.3)] {
                                draw::rrect(pm, px - g, cy - ph / 2.0 - g, pw + 2.0 * g, ph + 2.0 * g, 9.0 + g, th.accent, a);
                            }
                            draw::rrect(pm, px, cy - ph / 2.0, pw, ph, 9.0, th.accent, 1.0);
                            text.draw_centered(pm, &id.to_string(), px + pw / 2.0, cy, 29.0, th.background, 1.0);
                        } else {
                            let occupied = windows > 0;
                            draw::rrect(pm, px, cy - ph / 2.0, pw, ph, 10.0, th.selection, dim);
                            if self.pressed == Some(Hit::Workspace(id)) { draw::rrect_stroke(pm, px, cy - ph / 2.0, pw, ph, 9.0, th.accent, 1.0, 1.5); }
                            let c = if occupied { th.bright_foreground } else { th.foreground };
                            text.draw_centered(pm, &id.to_string(), px + pw / 2.0, cy - 1.0, 28.0, c, dim);
                            if occupied { draw::rrect(pm, px + pw / 2.0 - 7.0, cy + ph / 2.0 - 7.0, 14.0, 3.0, 1.5, th.bright_foreground, dim); }
                        }
                        hits.push(([px - 4.0, 0.0, pw + 8.0, h], Hit::Workspace(id)));
                        px += 66.0;
                    }
                }
                "activity" => {
                    let mut ax = x + 12.0;
                    for a in &self.activities {
                        let pw = 58.0 + a.label.chars().count() as f32 * 13.0;
                        draw::rrect(pm, ax, cy - 22.0, pw, 44.0, 22.0, th.selection, 1.0);
                        // three dots chasing (the app's 3-dot working badge, generic form)
                        for k in 0..3 {
                            let ph = ((self.t * 2.2 - k as f32 * 0.22).rem_euclid(1.0) * std::f32::consts::PI).sin();
                            draw::rrect(pm, ax + 14.0 + k as f32 * 9.0, cy - 3.5 - ph * 4.0, 7.0, 7.0, 3.5, marks::SUCCESS_GREEN, 0.5 + 0.5 * ph);
                        }
                        text.draw(pm, &a.label, ax + 46.0, cy, 21.0, th.bright_foreground, 1.0);
                        ax += pw + 12.0;
                    }
                    animating = true;
                }
                "bots" => {
                    let ms = self.cfg.bots.mark_size as f32;
                    let mut bx = x + 14.0 + ms / 2.0;
                    for (i, (bot, working)) in self.bots.iter().enumerate() {
                        let (pose, active) = marks::animate(&bot.id, *working, self.t, ms);
                        animating |= active;
                        let fill = marks.color(bot.color.as_deref(), &bot.id);
                        let shape = marks.shape_name(bot.shape.as_deref(), &bot.id);
                        let a = if *working { 1.0 } else { dim.max(0.9) };
                        marks.draw(pm, shape, fill, Rgb(0, 0, 0), bx, cy, ms, &pose, a);
                        // app sidebar markers: "blocked" (awaiting your response) wins over "unread"
                        if bot.blocked || bot.unread > 0 {
                            let (bxp, byp) = (bx + ms / 2.0 - 4.0, cy - ms / 2.0 + 5.0);
                            let label = if bot.blocked { "!".to_string() } else if bot.unread > 9 { "9+".into() } else { bot.unread.to_string() };
                            let bw = text.width(&label, 15.0).max(9.0) + 9.0;
                            let col = if bot.blocked { th.yellow } else { th.accent };
                            draw::rrect(pm, bxp - bw / 2.0 - 2.0, byp - 11.0, bw + 4.0, 22.0, 11.0, Rgb(0, 0, 0), 1.0);
                            draw::rrect(pm, bxp - bw / 2.0, byp - 9.0, bw, 18.0, 9.0, col, 1.0);
                            text.draw_centered(pm, &label, bxp, byp, 15.0, Rgb(0, 0, 0), 1.0);
                        }
                        hits.push(([bx - ms / 2.0 - 7.0, 0.0, ms + 14.0, h], Hit::Bot(i)));
                        bx += ms + 14.0;
                    }
                }
                "pulse" => {
                    let pw = self.cfg.pulse.width as f32;
                    let mut sx = x + 14.0;
                    let (lp, vp) = (18.0, 26.0);
                    if self.cfg.pulse.cpu {
                        text.draw(pm, "CPU", sx, cy - 14.0, lp, th.cyan, dim);
                        let v = format!("{:.0}%", self.cpu.last().copied().unwrap_or(0.0) * 100.0);
                        let vw = text.draw(pm, &v, sx, cy + 12.0, vp, th.bright_foreground, dim);
                        if let Some(tc) = self.cpu_temp {
                            let c = if tc >= 85.0 { th.red } else if tc >= 70.0 { th.yellow } else { th.bright_foreground };
                            text.draw(pm, &format!("{tc:.0}°"), sx + vw.max(52.0) + 8.0, cy + 12.0, vp, c, dim);
                        }
                        draw::sparkline(pm, &self.cpu, sysmon::HISTORY, sx + 124.0, cy - 25.0, pw, 50.0, th.cyan, dim.max(0.9));
                        sx += 128.0 + pw + 8.0;
                    }
                    if self.cfg.pulse.mem {
                        text.draw(pm, "MEM", sx, cy - 14.0, lp, th.magenta, dim);
                        let used = format!("{:.1}", self.mem_gib);
                        let uw = text.draw(pm, &used, sx, cy + 12.0, vp, th.bright_foreground, dim);
                        text.draw(pm, &format!("/{:.0}G", self.mem_total_gib.round()), sx + uw, cy + 12.0, vp * 0.75, th.foreground, dim);
                        sx += 132.0;
                    }
                    if self.cfg.pulse.net {
                        text.draw(pm, "NET", sx, cy - 14.0, lp, th.green, dim);
                        text.draw(pm, &sysmon::human_rate(self.net_rate), sx, cy + 12.0, vp, th.bright_foreground, dim);
                        draw::sparkline(pm, &self.net, sysmon::HISTORY, sx + 98.0, cy - 25.0, pw, 50.0, th.green, dim.max(0.9));
                    }
                }
                "weather" => {
                    if let Some((icon, temp)) = &self.weather {
                        let iw = text.draw(pm, icon, x + 6.0, cy, 34.0, th.yellow, dim);
                        text.draw(pm, temp, x + 6.0 + iw + 8.0, cy, 26.0, th.bright_foreground, dim);
                    }
                }
                "battery" => {
                    let b = &self.battery;
                    let c = if b.charging { th.green } else if b.percent <= 15 { th.red } else if b.percent <= 30 { th.yellow } else { th.green };
                    let bx = x + 6.0;
                    let c = if b.on_ac { th.green } else { c };
                    draw::rrect_stroke(pm, bx, cy - 13.0, 48.0, 26.0, 5.0, th.bright_foreground, dim, 2.0);
                    draw::rrect(pm, bx + 49.5, cy - 6.0, 4.0, 12.0, 1.5, th.bright_foreground, dim);
                    draw::rrect(pm, bx + 4.0, cy - 9.0, 40.0 * b.percent as f32 / 100.0, 18.0, 3.0, c, dim.max(0.9));
                    if b.on_ac || b.charging { draw::bolt(pm, bx + 24.0, cy, 22.0, Rgb(0, 0, 0), th.yellow); }
                    text.draw(pm, &format!("{}%", b.percent), bx + 62.0, cy, 26.0, th.bright_foreground, dim);
                }
                "clock" => {
                    let tm = &self.time;
                    let (hr, suffix) = if self.cfg.clock.hour24 { (tm.hour, "") } else { (((tm.hour + 11) % 12) + 1, if tm.hour < 12 { "a" } else { "p" }) };
                    let s = if self.cfg.clock.seconds { format!("{hr}:{:02}:{:02}{suffix}", tm.min, tm.sec) } else { format!("{hr}:{:02}{suffix}", tm.min) };
                    let bright = th.bright_foreground.mix(Rgb(255, 255, 255), 0.7);
                    let tw = text.width(&s, 52.0);
                    let right = x + w - 4.0;
                    text.draw(pm, &s, right - tw, cy, 52.0, bright, dim.max(0.95));
                    if self.cfg.clock.date {
                        let d1 = DAYS[tm.wday as usize % 7];
                        let d2 = format!("{} {}", MONTHS[tm.mon as usize % 12], tm.mday);
                        let dx = right - tw - 12.0;
                        let w1 = text.width(d1, 20.0); let w2 = text.width(&d2, 20.0);
                        text.draw(pm, d1, dx - w1, cy - 12.0, 20.0, th.accent, dim);
                        text.draw(pm, &d2, dx - w2, cy + 12.0, 20.0, th.bright_foreground, dim);
                    }
                }
                _ => {}
            }
            x += w;
            prev_drawn = name != "spacer";
        }
        Frame { hits, animating, idle_anim: !self.bots.is_empty() || !self.activities.is_empty() }
    }

    fn render_fn(&self, pm: &mut Pixmap, text: &mut Text, hits: &mut Vec<([f32; 4], Hit)>) {
        let th = &brighten(&self.theme);
        let (w, h) = (self.width as f32, self.height as f32);
        let gap = 6.0;
        let esc_w = 100.0;
        let key_w = (w - 16.0 - esc_w - gap * 12.0) / 12.0;
        let mut x = 8.0;
        for (i, label) in FKEYS.iter().enumerate() {
            let kw = if i == 0 { esc_w } else { key_w };
            let pressed = self.pressed == Some(Hit::FKey(i));
            draw::rrect(pm, x, h / 2.0 - 27.0, kw, 54.0, 9.0, if pressed { th.accent } else { th.selection }, 1.0);
            if !pressed { draw::rrect_stroke(pm, x + 0.5, h / 2.0 - 26.5, kw - 1.0, 53.0, 9.0, th.muted, 1.0, 1.5); }
            text.draw_centered(pm, label, x + kw / 2.0, h / 2.0, 28.0, if pressed { th.background } else { th.bright_foreground }, 1.0);
            hits.push(([x - gap / 2.0, 0.0, kw + gap, h], Hit::FKey(i)));
            x += kw + gap;
        }
    }

    fn render_controls(&self, pm: &mut Pixmap, text: &mut Text, hits: &mut Vec<([f32; 4], Hit)>) {
        let th = &brighten(&self.theme);
        let h = self.height as f32;
        let (gap, esc_w, key_w) = (8.0, 100.0, 150.0);
        let mut x = 8.0;
        let pressed = self.pressed == Some(Hit::CloseControls);
        draw::rrect(pm, x, h / 2.0 - 27.0, esc_w, 54.0, 9.0, if pressed { th.accent } else { th.selection }, 1.0);
        text.draw_centered(pm, "esc", x + esc_w / 2.0, h / 2.0, 28.0, if pressed { th.background } else { th.bright_foreground }, 1.0);
        hits.push(([0.0, 0.0, x + esc_w + gap / 2.0, h], Hit::CloseControls));
        x += esc_w + gap * 3.0;
        let mut group = "";
        for (i, (label, _)) in self.cfg.controls.buttons().iter().enumerate() {
            let g = label.split(' ').next().unwrap_or("");
            if !group.is_empty() && g != group { x += gap * 3.0; }
            group = g;
            let p = self.pressed == Some(Hit::Ctl(i));
            draw::rrect(pm, x, h / 2.0 - 27.0, key_w, 54.0, 9.0, if p { th.accent } else { th.selection }, 1.0);
            if !p { draw::rrect_stroke(pm, x + 0.5, h / 2.0 - 26.5, key_w - 1.0, 53.0, 9.0, th.muted, 1.0, 1.5); }
            text.draw_centered(pm, label, x + key_w / 2.0, h / 2.0, 24.0, if p { th.background } else { th.bright_foreground }, 1.0);
            hits.push(([x - gap / 2.0, 0.0, key_w + gap, h], Hit::Ctl(i)));
            x += key_w + gap;
        }
    }

    fn render_ambient(&self, pm: &mut Pixmap, text: &mut Text, hits: &mut Vec<([f32; 4], Hit)>) {
        let th = &self.theme;
        let (w, h) = (self.width as usize, self.height as usize);
        let t = self.t;
        let layers = [(th.accent, 0.9f32, 0.0021f32, 0.13f32, 0.0f32), (th.magenta, 0.7, 0.0033, -0.09, 2.1), (th.cyan, 0.6, 0.0017, 0.07, 4.2)];
        let data = pm.data_mut();
        for x in 0..w {
            let xf = x as f32;
            let mut centers = [(0.0f32, 0.0f32); 3];
            for (k, (_, amp, f, sp, ph)) in layers.iter().enumerate() {
                let yc = h as f32 / 2.0 + (xf * f + t * sp + ph).sin() * h as f32 * 0.32 * amp;
                let bright = 0.5 + 0.5 * (xf * f * 0.37 - t * sp * 0.6 + ph * 1.7).sin();
                centers[k] = (yc, bright);
            }
            for y in 0..h {
                let yf = y as f32;
                let (mut r, mut g, mut b) = (0.0f32, 0.0f32, 0.0f32);
                for (k, (c, ..)) in layers.iter().enumerate() {
                    let (yc, br) = centers[k];
                    let d = (yf - yc) / 9.0;
                    let a = (-d * d).exp() * br * 0.42;
                    r += c.0 as f32 * a; g += c.1 as f32 * a; b += c.2 as f32 * a;
                }
                let i = (y * w + x) * 4;
                data[i] = r.min(255.0) as u8; data[i + 1] = g.min(255.0) as u8; data[i + 2] = b.min(255.0) as u8; data[i + 3] = 255;
            }
        }
        // dim clock drifting slowly across the strip (OLED)
        let tm = &self.time;
        let s = format!("{}:{:02}", tm.hour, tm.min);
        let span = self.width as f32 * 0.6;
        let cx = self.width as f32 / 2.0 + (t * 0.01).sin() * span / 2.0;
        text.draw_centered(pm, &s, cx, h as f32 / 2.0 + (t * 0.05).sin() * 2.0, 26.0, th.bright_foreground, 0.55);
        hits.push(([0.0, 0.0, self.width as f32, self.height as f32], Hit::WakeAmbient));
    }

    fn render_touch_id(&self, pm: &mut Pixmap, text: &mut Text, tid: &TouchId, hits: &mut Vec<([f32; 4], Hit)>) {
        let th = &self.theme;
        let (w, h) = (self.width as f32, self.height as f32);
        draw::rrect(pm, 8.0, h / 2.0 - 21.0, 110.0, 42.0, 8.0, th.selection, 1.0);
        text.draw_centered(pm, "Cancel", 63.0, h / 2.0, 16.0, th.bright_foreground, 1.0);
        hits.push(([0.0, 0.0, 126.0, h], Hit::CancelTouchId));
        let (msg, c) = match tid {
            TouchId::Authenticate => ("Touch ID: place your finger on the sensor".to_string(), th.accent),
            TouchId::Approve => ("Touch ID to approve".to_string(), th.accent),
            TouchId::Retry => ("Not recognized, try again".to_string(), th.red),
            TouchId::Success => ("Touch ID accepted".to_string(), th.green),
            TouchId::Enrollment(p) => (format!("Enrolling fingerprint {p}%"), th.accent),
        };
        let pulse = 0.75 + 0.25 * (self.t * 3.0).sin();
        draw::rrect_stroke(pm, w / 2.0 - 380.0, h / 2.0 - 20.0, 760.0, 40.0, 20.0, c, pulse, 2.0);
        if let TouchId::Enrollment(p) = tid { draw::rrect(pm, w / 2.0 - 378.0, h / 2.0 + 14.0, 756.0 * *p as f32 / 100.0, 4.0, 2.0, c, 1.0); }
        text.draw_centered(pm, &msg, w / 2.0, h / 2.0, 18.0, th.bright_foreground, 1.0);
        if !matches!(tid, TouchId::Success) { draw_sensor_arrow(pm, self.t, w, h); }
    }
}

/// Bright green chevrons sweeping right toward the Touch ID sensor (just right of the bar).
pub fn draw_sensor_arrow(pm: &mut Pixmap, t: f32, w: f32, h: f32) {
    let g = Rgb(0x2b, 0xff, 0x7a);
    let n = 5;
    let spacing = 44.0;
    let x0 = w - 18.0 - spacing * (n as f32 - 1.0);
    let phase = (t * 1.6).fract(); // one sweep every ~0.6 s
    for i in 0..n {
        let x = x0 + spacing * i as f32;
        // brightness wave travelling rightward; nearest chevron always fairly bright
        let d = ((i as f32 / (n - 1) as f32) - phase).rem_euclid(1.0);
        let a = (0.25 + 0.75 * (1.0 - d).powi(3)).max(if i == n - 1 { 0.6 } else { 0.0 });
        let (cw, ch) = (16.0 + 3.0 * i as f32, h * 0.36 + 2.0 * i as f32);
        let mut pb = PathBuilder::new();
        pb.move_to(x - cw, h / 2.0 - ch);
        pb.line_to(x, h / 2.0);
        pb.line_to(x - cw, h / 2.0 + ch);
        if let Some(path) = pb.finish() {
            let st = tiny_skia::Stroke { width: 7.0, line_cap: tiny_skia::LineCap::Round, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
            pm.stroke_path(&path, &draw::paint(g, a * 0.35), &tiny_skia::Stroke { width: 13.0, ..st.clone() }, tiny_skia::Transform::identity(), None);
            pm.stroke_path(&path, &draw::paint(g, a), &st, tiny_skia::Transform::identity(), None);
        }
    }
}

/// Brighter, higher-contrast variant of the theme for the OLED strip.
pub fn brighten(t: &Theme) -> Theme {
    let w = Rgb(255, 255, 255);
    let mut b = t.clone();
    b.bright_foreground = t.bright_foreground.mix(w, 0.45);
    b.foreground = t.foreground.mix(w, 0.3);
    b.selection = t.selection.mix(t.foreground, 0.22);
    b.muted = t.muted.mix(t.foreground, 0.3);
    b.accent = t.accent.mix(w, 0.12);
    b
}

pub fn hit_test(hits: &[([f32; 4], Hit)], x: f32, y: f32) -> Option<Hit> {
    hits.iter().find(|(r, _)| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]).map(|(_, h)| *h)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::bots::parse_roster;

    pub fn sample_scene() -> Scene {
        let bots = parse_roster(include_str!("../tests/fixtures/roster.json"));
        Scene {
            width: 2170, height: 60, cfg: Config::default(),
            theme: Theme::parse(include_str!("../tests/fixtures/colors.toml")),
            bots: bots.into_iter().map(|b| (b, false)).collect(),
            activities: vec![],
            workspaces: vec![Workspace { id: 1, windows: 3 }, Workspace { id: 2, windows: 1 }, Workspace { id: 4, windows: 2 }],
            active_ws: 2,
            cpu: (0..60).map(|i| 0.2 + 0.15 * (i as f32 * 0.4).sin().abs()).collect(),
            mem: vec![0.4; 60], net: (0..60).map(|i| (i % 7) as f32 / 10.0).collect(),
            net_rate: 2.4 * 1048576.0, mem_gib: 5.1, mem_total_gib: 16.0, cpu_temp: Some(64.0), weather: Some(("\u{e30d}".into(), "62°F".into())),
            battery: Battery { percent: 74, charging: false, present: true, on_ac: false },
            time: LocalTime { hour: 14, min: 12, sec: 5, wday: 5, mday: 9, mon: 9 },
            fn_pressed: false, controls: false, ambient: false, idle_secs: 0.0, t: 3.0, unix_secs: 0, touch_id: None, pressed: None,
        }
    }

    #[cfg(feature = "grok-bot")]
    #[test]
    fn layout_hits_cover_controls() {
        let s = sample_scene();
        let mut pm = Pixmap::new(s.width, s.height).unwrap();
        let mut text = Text::load(None);
        let f = s.render(&mut pm, &Marks::builtin(), &mut text);
        assert_eq!(hit_test(&f.hits, 30.0, 30.0), Some(Hit::Esc));
        let ws: Vec<i64> = f.hits.iter().filter_map(|(_, h)| if let Hit::Workspace(i) = h { Some(*i) } else { None }).collect();
        assert_eq!(ws, vec![1, 2, 3, 4, 5]);
        let bot_hits = f.hits.iter().filter(|(_, h)| matches!(h, Hit::Bot(_))).count();
        assert_eq!(bot_hits, 6);
        // everything fits within the panel
        assert!(f.hits.iter().all(|(r, _)| r[0] + r[2] <= 2170.0 + 8.0));
    }

    #[test]
    fn fn_layer_has_13_keys_spanning_panel() {
        let mut s = sample_scene();
        s.fn_pressed = true;
        let mut pm = Pixmap::new(s.width, s.height).unwrap();
        let f = s.render(&mut pm, &Marks::builtin(), &mut Text::load(None));
        assert_eq!(f.hits.len(), 13);
        assert_eq!(hit_test(&f.hits, 2160.0, 30.0), Some(Hit::FKey(12)));
        assert_eq!(hit_test(&f.hits, 20.0, 30.0), Some(Hit::FKey(0)));
    }

    #[test]
    fn generic_mode_without_bots_lays_out_cleanly() {
        let mut s = sample_scene();
        s.bots.clear();
        s.activities = vec![crate::bots::Activity { name: "claude".into(), label: "Claude".into() }];
        let mut pm = Pixmap::new(s.width, s.height).unwrap();
        let f = s.render(&mut pm, &Marks { center: 114.27, colors: Default::default(), shapes: Default::default() }, &mut Text::load(None));
        assert!(!f.hits.iter().any(|(_, h)| matches!(h, Hit::Bot(_))));
        assert_eq!(hit_test(&f.hits, 30.0, 30.0), Some(Hit::Esc));
        assert!(f.animating);
    }

    #[test]
    fn controls_layer_has_buttons_and_close() {
        let mut s = sample_scene();
        s.controls = true;
        let mut pm = Pixmap::new(s.width, s.height).unwrap();
        let f = s.render(&mut pm, &Marks { center: 114.27, colors: Default::default(), shapes: Default::default() }, &mut Text::load(None));
        assert_eq!(hit_test(&f.hits, 30.0, 30.0), Some(Hit::CloseControls));
        assert_eq!(f.hits.iter().filter(|(_, h)| matches!(h, Hit::Ctl(_))).count(), 7);
    }

    #[test]
    fn oled_offsets_bounded_and_moving() {
        let offs: Vec<_> = (0..16).map(|i| oled_offset(i * 120, 120, 2)).collect();
        assert!(offs.iter().all(|(x, y)| x.abs() <= 2.0 && y.abs() <= 2.0));
        assert!(offs.windows(2).any(|w| w[0] != w[1]));
        assert_eq!(oled_offset(999, 0, 2), (0.0, 0.0));
    }

    #[test]
    fn touch_id_prompt_has_right_arrow() {
        let mut s = sample_scene();
        s.touch_id = Some(TouchId::Authenticate);
        let mut pm = Pixmap::new(s.width, s.height).unwrap();
        s.render(&mut pm, &Marks::builtin(), &mut Text::load(None));
        let w = s.width as usize;
        let green_right = pm.pixels().iter().enumerate().filter(|(i, p)| i % w > 1900 && p.green() > 60 && p.red() < 40).count();
        assert!(green_right > 100, "{green_right}");
        s.touch_id = Some(TouchId::Success);
        s.render(&mut pm, &Marks::builtin(), &mut Text::load(None));
        assert_eq!(pm.pixels().iter().enumerate().filter(|(i, p)| i % w > 1900 && p.green() > 60 && p.red() < 40).count(), 0);
    }
    #[cfg(feature = "grok-bot")]
    #[test]
    fn working_bot_draws_green_dot() {
        let mut s = sample_scene();
        s.bots[0].1 = true;
        let mut pm = Pixmap::new(s.width, s.height).unwrap();
        let f = s.render(&mut pm, &Marks::builtin(), &mut Text::load(None));
        assert!(f.animating);
        let green = pm.pixels().iter().filter(|p| p.red() == 0 && p.green() == 0xc9 && p.blue() == 0x72).count();
        assert!(green > 10, "green pixels {green}");
    }
}
