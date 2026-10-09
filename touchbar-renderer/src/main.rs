//! t1-dash: custom T1Bridge Touch Bar renderer (v1).
#![allow(clippy::too_many_arguments)]
#![cfg_attr(not(feature = "grok-bot"), allow(dead_code))]
mod bots;
mod config;
mod draw;
mod hypr;
mod marks;
mod preview;
mod scene;
mod sysmon;
#[allow(dead_code, unused_imports)]
mod t1bridge;
mod theme;

use scene::{Hit, LocalTime, Scene, TouchId};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant, SystemTime};
use t1bridge::{Client, ClientError, Event, Key};
use tiny_skia::Pixmap;

static STOP: AtomicBool = AtomicBool::new(false);
static RELOAD: AtomicBool = AtomicBool::new(false);
const TOUCH_ID_STATE: &str = "/run/t1bridge/touch-id-state.json";
const FKEY_CODES: [Key; 13] = [Key::Escape, Key::F1, Key::F2, Key::F3, Key::F4, Key::F5, Key::F6, Key::F7, Key::F8, Key::F9, Key::F10, Key::F11, Key::F12];

fn home() -> PathBuf { std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/")) }
fn config_home() -> PathBuf { std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config")) }
fn mtime(p: &Path) -> Option<SystemTime> { std::fs::metadata(p).and_then(|m| m.modified()).ok() }

/// All desktop-side state; survives reconnects to the hardware service.
pub struct Live {
    cfg_path: PathBuf,
    cfg_mtime: Option<SystemTime>,
    theme_path: Option<PathBuf>,
    theme_mtime: Option<SystemTime>,
    roster_path: Option<PathBuf>,
    roster_mtime: Option<SystemTime>,
    status_dir: PathBuf,
    all_bots: Vec<bots::Bot>,
    marks: marks::Marks,
    text: draw::Text,
    sys: sysmon::SysMon,
    scene: Scene,
    hypr_rx: Receiver<hypr::HyprEvent>,
    screensaver: bool,
    last_activity: Instant,
    wake_until: Option<Instant>,
    started: Instant,
    last_sample: Instant,
    irq_total: u64,
    theme_text: String,
    replicas: std::collections::HashMap<String, PathBuf>,
    /// id -> (mtime seen, pending user message ms)
    replica_state: std::collections::HashMap<String, (Option<SystemTime>, Option<u64>)>,
    persistence_dir: PathBuf,
    weather: std::sync::Arc<std::sync::Mutex<Option<(String, String)>>>,
}

impl Live {
    pub fn new(width: u32, height: u32, watch_hypr: bool) -> Live {
        let cfg_path = std::env::var_os("T1_DASH_CONFIG").map(PathBuf::from).unwrap_or_else(|| config_home().join("touchbar/config.toml"));
        let cfg = config::Config::load(&cfg_path);
        let (tx, rx) = channel();
        if watch_hypr { hypr::spawn_watcher(tx); }
        let text = draw::Text::load(cfg.font.as_deref());
        let now = Instant::now();
        let mut live = Live {
            cfg_mtime: mtime(&cfg_path), cfg_path,
            theme_path: None, theme_mtime: None, roster_path: None, roster_mtime: None,
            status_dir: PathBuf::new(), all_bots: vec![],
            marks: marks::Marks::builtin(), text,
            sys: sysmon::SysMon::new(PathBuf::from("/")),
            scene: Scene {
                width, height, cfg, theme: theme::Theme::default(), bots: vec![], workspaces: vec![], active_ws: 1,
                cpu: vec![], mem: vec![], net: vec![], activities: vec![], net_rate: 0.0, mem_gib: 0.0, mem_total_gib: 0.0, cpu_temp: None, weather: None, battery: Default::default(),
                time: LocalTime::now(), fn_pressed: false, controls: false, ambient: false, idle_secs: 0.0, t: 0.0, unix_secs: 0, touch_id: None, pressed: None,
            },
            hypr_rx: rx, screensaver: false, last_activity: now, wake_until: None, started: now, last_sample: now - Duration::from_secs(2), irq_total: 0, theme_text: String::new(), replicas: Default::default(), replica_state: Default::default(), persistence_dir: config_home().join("Grok Bot/sand-client-persistence"), weather: Default::default(),
        };
        if watch_hypr { spawn_weather(live.weather.clone(), live.scene.cfg.weather_cmd.clone(), live.scene.cfg.weather_refresh_secs); }
        live.apply_config();
        live.sys.sample(1.0);
        live
    }

    fn apply_config(&mut self) {
        let cfg = &self.scene.cfg;
        self.theme_path = cfg.theme.path.clone().or_else(|| theme::default_theme_paths(&home()).into_iter().find(|p| p.exists()));
        self.theme_mtime = None;
        self.roster_path = if grok_enabled(cfg) { cfg.bots.roster.clone().or_else(|| bots::find_roster(&self.persistence_dir)) } else { None };
        if self.roster_path.is_none() { self.all_bots.clear(); self.replicas.clear(); }
        self.roster_mtime = None;
        self.status_dir = cfg.bots.status_dir.clone().unwrap_or_else(|| home().join(".local/state/touchbar/bots"));
        self.text = draw::Text::load(cfg.font.as_deref());
    }

    /// Cheap periodic refresh (call every loop; does real work about once per second).
    pub fn poll_sources(&mut self) -> bool {
        let mut changed = false;
        let reload = RELOAD.swap(false, Ordering::SeqCst);
        while let Ok(ev) = self.hypr_rx.try_recv() {
            match ev {
                hypr::HyprEvent::Workspaces { list, active } => { self.scene.workspaces = list; self.scene.active_ws = active; changed = true; }
                hypr::HyprEvent::Screensaver(on) => { self.screensaver = on; if !on { self.last_activity = Instant::now(); } changed = true; }
                hypr::HyprEvent::Activity => { self.last_activity = Instant::now(); }
            }
        }
        self.scene.touch_id = read_touch_id(Path::new(TOUCH_ID_STATE));
        // keyboard/trackpad activity via interrupt counters (no input-device access needed)
        let irq = irq_count(&std::fs::read_to_string("/proc/interrupts").unwrap_or_default(), &self.scene.cfg.activity_irqs);
        if irq != self.irq_total { if self.irq_total != 0 { self.last_activity = Instant::now(); } self.irq_total = irq; }
        if self.last_sample.elapsed() < Duration::from_secs(1) && !reload { return changed || self.update_modes(); }
        let dt = self.last_sample.elapsed().as_secs_f64();
        self.last_sample = Instant::now();
        if reload || mtime(&self.cfg_path) != self.cfg_mtime {
            self.cfg_mtime = mtime(&self.cfg_path);
            self.scene.cfg = config::Config::load(&self.cfg_path);
            self.apply_config();
        }
        if self.theme_path.is_none() { self.theme_path = theme::default_theme_paths(&home()).into_iter().find(|p| p.exists()); }
        if let Some(p) = &self.theme_path {
            // compare content, not mtime: omarchy swaps the whole theme dir and package files share mtimes
            let text = std::fs::read_to_string(p).unwrap_or_default();
            if reload || (!text.is_empty() && text != self.theme_text) {
                self.theme_mtime = mtime(p);
                self.scene.theme = theme::Theme::parse(&text);
                self.theme_text = text;
            }
        }
        if let Some(p) = &self.roster_path {
            let m = mtime(p);
            if m != self.roster_mtime {
                self.roster_mtime = m;
                if let Ok(t) = std::fs::read_to_string(p) { self.all_bots = bots::parse_roster(&t); }
                self.replicas = bots::find_replicas(&self.persistence_dir);
            }
        }
        let now = SystemTime::now();
        let c = &self.scene.cfg.bots;
        self.scene.bots = bots::select(self.all_bots.clone(), &c.include, &c.exclude)
            .into_iter().map(|b| {
                let mut w = bots::is_working(&self.status_dir, &b, c.stale_after_secs, now);
                if c.app_state && !w {
                    if let Some(p) = self.replicas.get(&b.id) {
                        let m = mtime(p);
                        let st = self.replica_state.entry(b.id.clone()).or_insert((None, None));
                        if st.0 != m { st.0 = m; st.1 = std::fs::read_to_string(p).ok().and_then(|t| bots::pending_user_message(&t)); }
                        let recent = m.and_then(|m| now.duration_since(m).ok()).is_some_and(|d| d.as_secs() < c.working_recent_secs);
                        let now_ms = now.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
                        let pending = st.1.is_some_and(|ts| now_ms.saturating_sub(ts) < 15 * 60 * 1000);
                        w = recent || pending;
                    }
                }
                let mut b = b;
                if !c.app_state { b.unread = 0; b.blocked = false; }
                (b, w)
            }).collect();
        let skip: Vec<String> = self.scene.bots.iter().flat_map(|(b, _)| [b.id.clone(), bots::slug(&b.name)]).collect();
        self.scene.activities = bots::read_activities(&self.status_dir, &skip, self.scene.cfg.bots.stale_after_secs, now);
        self.sys.sample(dt);
        self.scene.cpu = self.sys.cpu.iter().copied().collect();
        self.scene.mem = self.sys.mem.iter().copied().collect();
        self.scene.net = self.sys.net.iter().copied().collect();
        self.scene.net_rate = self.sys.net_rate;
        self.scene.mem_gib = self.sys.mem_used_gib;
        self.scene.mem_total_gib = self.sys.mem_total_gib;
        self.scene.cpu_temp = self.sys.cpu_temp_c;
        self.scene.weather = self.weather.lock().ok().and_then(|w| w.clone());
        self.scene.battery = self.sys.battery.clone();
        self.scene.time = LocalTime::now();
        self.update_modes();
        true
    }

    fn update_modes(&mut self) -> bool {
        let s = &mut self.scene;
        s.t = self.started.elapsed().as_secs_f32();
        s.unix_secs = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        s.idle_secs = self.last_activity.elapsed().as_secs_f32();
        let a = &s.cfg.ambient;
        let woke = self.wake_until.is_some_and(|t| Instant::now() < t);
        let ambient = a.enabled && !woke && ((a.on_screensaver && self.screensaver) || (a.idle_secs > 0 && s.idle_secs >= a.idle_secs as f32));
        let changed = ambient != s.ambient;
        s.ambient = ambient;
        changed
    }

    pub fn touch(&mut self) { self.last_activity = Instant::now(); }
}

/// Sums per-CPU counts of /proc/interrupts lines whose device names match any of `names`.
pub fn irq_count(text: &str, names: &[String]) -> u64 {
    text.lines().filter(|l| names.iter().any(|n| !n.is_empty() && l.contains(n.as_str())))
        .map(|l| l.split_whitespace().skip(1).map_while(|x| x.parse::<u64>().ok()).sum::<u64>()).sum()
}

/// Accepts wttr.in `format=j1` JSON (as the Omarchy bar uses) or a plain `ICON|TEMP` line.
pub fn parse_weather(out: &str) -> Option<(String, String)> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(out.trim()) {
        return parse_j1(&v, LocalTime::now());
    }
    let (icon, temp) = out.trim().split_once('|')?;
    let temp = temp.trim().trim_start_matches('+').to_string();
    if temp.is_empty() || temp.len() > 12 || !temp.chars().any(|c| c.is_ascii_digit()) { return None; }
    Some((icon.trim().to_string(), temp))
}

fn minutes_12h(s: &str) -> Option<u32> {
    let (hm, ap) = s.trim().split_once(' ')?;
    let (h, m) = hm.split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    Some((h % 12 + if ap.eq_ignore_ascii_case("PM") { 12 } else { 0 }) * 60 + m)
}

/// Same code->glyph table as omarchy-weather-icon; imperial for US like the bar widget.
pub fn parse_j1(v: &serde_json::Value, now: LocalTime) -> Option<(String, String)> {
    let cur = v.pointer("/current_condition/0")?;
    let code: u32 = cur.get("weatherCode")?.as_str()?.parse().ok()?;
    let astro = v.pointer("/weather/0/astronomy/0");
    let rise = astro.and_then(|a| minutes_12h(a.get("sunrise")?.as_str()?));
    let set = astro.and_then(|a| minutes_12h(a.get("sunset")?.as_str()?));
    let m = now.hour * 60 + now.min;
    let night = matches!((rise, set), (Some(r), Some(s)) if m < r || m >= s);
    let icon = match code {
        113 => if night { '\u{e32b}' } else { '\u{e30d}' },
        116 => if night { '\u{e32e}' } else { '\u{e302}' },
        119 | 122 => '\u{e33d}',
        143 | 248 | 260 => '\u{e313}',
        176 | 263 | 353 => if night { '\u{e333}' } else { '\u{e308}' },
        179 | 227 | 230 | 323 | 326 | 368 => if night { '\u{e327}' } else { '\u{e30a}' },
        182 | 185 | 281 | 284 | 311 | 314 | 317 | 320 | 350 | 362 | 365 | 374 | 377 => '\u{e3ad}',
        200 | 386 | 389 | 392 | 395 => '\u{e31d}',
        266 | 293 | 296 | 299 | 302 | 305 | 308 | 356 | 359 => '\u{e318}',
        329 | 332 | 335 | 338 | 371 => '\u{e31a}',
        _ => '\u{e33d}',
    };
    let country = v.pointer("/nearest_area/0/country/0/value").and_then(|c| c.as_str()).unwrap_or("").to_lowercase();
    let imperial = matches!(country.as_str(), "us" | "usa" | "united states" | "united states of america");
    let (key, unit) = if imperial { ("temp_F", "°F") } else { ("temp_C", "°C") };
    let t = cur.get(key)?.as_str()?;
    Some((icon.to_string(), format!("{t}{unit}")))
}

/// Background weather refresh with an on-disk cache (~/.cache/t1-dash/weather.txt).
fn spawn_weather(slot: std::sync::Arc<std::sync::Mutex<Option<(String, String)>>>, cmd: String, every: u64) {
    if cmd.trim().is_empty() { return; }
    let cache = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".cache")).join("t1-dash/weather.txt");
    std::thread::spawn(move || {
        let mut wait = Duration::ZERO;
        if let Ok(t) = std::fs::read_to_string(&cache) {
            if let Some(w) = parse_weather(&t) { *slot.lock().unwrap() = Some(w); }
            let age = mtime(&cache).and_then(|m| m.elapsed().ok()).unwrap_or(Duration::MAX);
            wait = Duration::from_secs(every).saturating_sub(age);
        }
        loop {
            std::thread::sleep(wait);
            let out = std::process::Command::new("sh").arg("-c").arg(&cmd).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output();
            let text = out.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
            if let Some(w) = parse_weather(&text) {
                *slot.lock().unwrap() = Some(w);
                if let Some(d) = cache.parent() { let _ = std::fs::create_dir_all(d); }
                let _ = std::fs::write(&cache, &text);
                wait = Duration::from_secs(every.max(60));
            } else {
                wait = Duration::from_secs(120); // retry sooner after a failed fetch
            }
        }
    });
}

/// Grok Bot integration on? Needs the build feature; config `bots.enabled` forces, unset = auto-detect.
fn grok_enabled(cfg: &config::Config) -> bool {
    cfg!(feature = "grok-bot") && cfg.bots.enabled.unwrap_or_else(|| config_home().join("Grok Bot/sand-client-persistence").is_dir())
}

fn read_touch_id(path: &Path) -> Option<TouchId> {
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let pid = v.get("pid")?.as_u64()?;
    if pid == 0 || !Path::new(&format!("/proc/{pid}")).exists() { return None; }
    Some(match v.get("state")?.as_str()? {
        "authenticate" => TouchId::Authenticate,
        "approve" => TouchId::Approve,
        "retry" => TouchId::Retry,
        "success" => TouchId::Success,
        "enrollment" => TouchId::Enrollment(v.get("progress").and_then(|p| p.as_u64()).unwrap_or(0).min(100) as u8),
        _ => return None,
    })
}

extern "C" fn on_stop(_: libc::c_int) { STOP.store(true, Ordering::SeqCst); }
extern "C" fn on_reload(_: libc::c_int) { RELOAD.store(true, Ordering::SeqCst); }

fn run_live() {
    if unsafe { libc::geteuid() } == 0 { eprintln!("t1-dash: refusing to run as root"); std::process::exit(2); }
    unsafe {
        libc::signal(libc::SIGTERM, on_stop as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_stop as *const () as libc::sighandler_t);
        libc::signal(libc::SIGUSR1, on_reload as *const () as libc::sighandler_t);
    }
    let mut live: Option<Live> = None;
    let mut delay = Duration::from_millis(250);
    while !STOP.load(Ordering::SeqCst) {
        match Client::connect() {
            Ok(mut client) => {
                delay = Duration::from_millis(250);
                let d = client.dimensions();
                eprintln!("t1-dash: connected, panel {}x{}", d.width, d.height);
                let l = live.get_or_insert_with(|| Live::new(d.width, d.height, true));
                l.scene.width = d.width; l.scene.height = d.height;
                match session(&mut client, l) {
                    Ok(()) => return,
                    Err(e) => eprintln!("t1-dash: connection lost ({e}); reconnecting"),
                }
            }
            Err(e) => {
                if matches!(e, ClientError::Protocol | ClientError::Frame) { eprintln!("t1-dash: connect failed: {e}"); }
                std::thread::sleep(delay);
                delay = (delay * 2).min(Duration::from_secs(5));
            }
        }
    }
}

fn session(client: &mut Client, live: &mut Live) -> Result<(), ClientError> {
    let (w, h) = (live.scene.width, live.scene.height);
    let mut pm = Pixmap::new(w, h).ok_or(ClientError::Frame)?;
    let mut hits = vec![];
    let mut down: HashSet<u8> = HashSet::new();
    let mut dirty = true;
    let mut last_hash = 0u64;
    let mut last_render = Instant::now() - Duration::from_secs(10);
    let mut animating = false;
    let mut idle_anim = false;
    let mut last_sec = 0u64;
    let mut fn_raw = false;
    let mut last_fn_tap: Option<Instant> = None;
    let mut controls_since = Instant::now();
    let mut shown_controls = false;
    loop {
        if STOP.load(Ordering::SeqCst) { return Ok(()); }
        if live.scene.controls && controls_since.elapsed() >= Duration::from_secs(live.scene.cfg.controls.timeout_secs.max(1)) {
            live.scene.controls = false;
        }
        if live.scene.controls != shown_controls {
            shown_controls = live.scene.controls;
            live.scene.fn_pressed = fn_raw && !live.scene.controls;
            hits = live.scene.render(&mut pm, &live.marks, &mut live.text).hits;
            dirty = true;
        }
        dirty |= live.poll_sources();
        let fps = live.scene.cfg.fps.clamp(1, 60);
        let frame_dt = Duration::from_secs_f32(1.0 / if live.scene.ambient { (fps / 2).max(1) } else { fps } as f32);
        if live.scene.unix_secs != last_sec { last_sec = live.scene.unix_secs; dirty = true; }
        let idle_dt = Duration::from_secs_f32(1.0 / live.scene.cfg.idle_fps.clamp(1, 60) as f32);
        let due = (animating && last_render.elapsed() >= frame_dt) || (idle_anim && last_render.elapsed() >= idle_dt);
        if (dirty || due) && client.frame_available() {
            let f = live.scene.render(&mut pm, &live.marks, &mut live.text);
            hits = f.hits; animating = f.animating; idle_anim = f.idle_anim; last_render = Instant::now();
            let hsh = hash(pm.data());
            if hsh != last_hash && client.submit_rgba(pm.data(), w, h)? { last_hash = hsh; }
            dirty = false;
        }
        let timeout = if animating { frame_dt.saturating_sub(last_render.elapsed()) } else if idle_anim { idle_dt.saturating_sub(last_render.elapsed()) } else { Duration::from_millis(200) };
        let mut pfd = libc::pollfd { fd: client.raw_fd(), events: libc::POLLIN, revents: 0 };
        unsafe { libc::poll(&mut pfd, 1, timeout.as_millis().clamp(1, 250) as i32) };
        if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 { return Err(ClientError::Transport); }
        while let Some(ev) = client.receive()? {
            let Event::Input(input) = ev else { continue };
            live.touch();
            if input.fn_pressed != fn_raw {
                fn_raw = input.fn_pressed;
                if fn_raw {
                    // double-tap Fn (two presses within 400 ms) toggles the hidden controls layer
                    let now = Instant::now();
                    if live.scene.cfg.controls.enabled && last_fn_tap.is_some_and(|t| now.duration_since(t) < Duration::from_millis(400)) {
                        live.scene.controls = !live.scene.controls;
                        controls_since = now;
                        last_fn_tap = None;
                    } else { last_fn_tap = Some(now); }
                }
            }
            let want_fn = fn_raw && !live.scene.controls;
            if want_fn != live.scene.fn_pressed || live.scene.controls != shown_controls {
                shown_controls = live.scene.controls;
                live.scene.fn_pressed = want_fn;
                // refresh hit regions for the new layer before handling touches in this frame
                hits = live.scene.render(&mut pm, &live.marks, &mut live.text).hits;
                dirty = true;
            }
            let now_down: HashSet<u8> = input.contacts.iter().filter(|c| c.tip).map(|c| c.id).collect();
            for c in input.contacts.iter().filter(|c| c.tip && !down.contains(&c.id)) {
                let Some(hit) = scene::hit_test(&hits, c.x as f32, c.y as f32) else { continue };
                live.scene.pressed = Some(hit);
                dirty = true;
                match hit {
                    Hit::Esc => client.tap_keys(&[Key::Escape])?,
                    Hit::FKey(i) => { last_fn_tap = None; client.tap_keys(&[FKEY_CODES[i]])? }
                    Hit::CloseControls => { live.scene.controls = false; }
                    Hit::Ctl(i) => {
                        controls_since = Instant::now();
                        if let Some((_, cmd)) = live.scene.cfg.controls.buttons().get(i) { run_cmd(cmd); }
                    }
                    Hit::Workspace(n) => hypr::switch_workspace(n),
                    Hit::CancelTouchId => client.cancel_touch_id()?,
                    Hit::WakeAmbient => { live.wake_until = Some(Instant::now() + Duration::from_secs(15)); live.scene.ambient = false; }
                    Hit::Bot(i) => {
                        let name = live.scene.bots.get(i).map(|b| b.0.name.clone()).unwrap_or_default();
                        hypr::open_bot(name, false);
                    }
                }
            }
            if now_down.is_empty() && live.scene.pressed.is_some() { live.scene.pressed = None; dirty = true; }
            down = now_down;
        }
    }
}

/// Run a control command in the background; output is discarded, the child is reaped by a thread.
fn run_cmd(cmd: &str) {
    if let Ok(mut c) = std::process::Command::new("sh").arg("-c").arg(cmd)
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
        std::thread::spawn(move || { let _ = c.wait(); });
    }
}

fn hash(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for c in b.chunks(8) { let mut v = [0u8; 8]; v[..c.len()].copy_from_slice(c); h = (h ^ u64::from_le_bytes(v)).wrapping_mul(0x0100_0000_01b3); }
    h
}

/// Headless dump of one frame from the real local sources (theme, roster, status, /proc).
fn dump(out: &Path, w: u32, h: u32) {
    let mut live = Live::new(w, h, true);
    std::thread::sleep(Duration::from_millis(1100));
    live.poll_sources();
    let mut pm = Pixmap::new(w, h).unwrap();
    live.scene.render(&mut pm, &live.marks, &mut live.text);
    pm.save_png(out).expect("write png");
    let s = &live.scene;
    println!("dumped {} ({}x{}) theme={:?} roster={:?} bots={} working={} workspaces={:?} active={} ambient={}",
        out.display(), w, h, live.theme_path, live.roster_path, s.bots.len(), s.bots.iter().filter(|b| b.1).count(),
        s.workspaces.iter().map(|w| w.id).collect::<Vec<_>>(), s.active_ws, s.ambient);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        None | Some("run") => run_live(),
        Some("preview") => preview::render_all(Path::new(args.get(2).map(String::as_str).unwrap_or("previews"))),
        Some("dump") => {
            let out = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "frame.png".into()));
            let w = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(2170);
            let h = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(60);
            dump(&out, w, h)
        }
        Some("--version") => println!("t1-dash {}", env!("CARGO_PKG_VERSION")),
        Some(_) => { eprintln!("usage: t1-dash [run | preview DIR | dump OUT.png [W H] | --version]"); std::process::exit(2) }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn weather_parse() {
        assert_eq!(super::parse_weather("\u{e30d}|+62°F\n"), Some(("\u{e30d}".into(), "62°F".into())));
        assert_eq!(super::parse_weather("|"), None);
        let j: serde_json::Value = serde_json::from_str(r#"{"current_condition":[{"weatherCode":"113","temp_F":"54","temp_C":"12"}],"weather":[{"astronomy":[{"sunrise":"07:10 AM","sunset":"06:40 PM"}]}],"nearest_area":[{"country":[{"value":"United States of America"}]}]}"#).unwrap();
        let day = super::LocalTime { hour: 14, ..Default::default() };
        assert_eq!(super::parse_j1(&j, day), Some(("\u{e30d}".into(), "54°F".into())));
        let night = super::LocalTime { hour: 22, ..Default::default() };
        assert_eq!(super::parse_j1(&j, night).unwrap().0, "\u{e32b}");
        assert_eq!(super::parse_weather("x|Unknown location; please try"), None);
    }
    #[test]
    fn irq_counts() {
        let t = "           CPU0       CPU1\n 22:   5   6  IR-IO-APIC  22-fasteoi   pxa2xx-spi.4, idma64.4\n 23:  10  20  IR-IO-APIC  23-fasteoi   pxa2xx-spi.5, idma64.5\n  1:  99 99 IO-APIC i8042\n";
        assert_eq!(super::irq_count(t, &["pxa2xx-spi".into()]), 41);
        assert_eq!(super::irq_count(t, &["i8042".into()]), 198);
    }
}
