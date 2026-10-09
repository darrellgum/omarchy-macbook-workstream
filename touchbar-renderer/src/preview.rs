//! Preview PNGs from the real drawing code with fixture data.
use crate::bots::parse_roster;
use crate::config::Config;
use crate::draw::Text;
use crate::hypr::Workspace;
use crate::marks::Marks;
use crate::scene::{LocalTime, Scene, TouchId};
use crate::sysmon::Battery;
use crate::theme::Theme;
use std::path::Path;
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

pub fn demo_scene() -> Scene {
    let bots = parse_roster(include_str!("../tests/fixtures/roster.json"));
    Scene {
        width: 2170, height: 60, cfg: Config::default(),
        theme: Theme::parse(include_str!("../tests/fixtures/colors.toml")),
        bots: bots.into_iter().map(|b| (b, false)).collect(),
        activities: vec![],
        workspaces: vec![Workspace { id: 1, windows: 3 }, Workspace { id: 2, windows: 1 }, Workspace { id: 4, windows: 2 }],
        active_ws: 2,
        cpu: (0..60).map(|i| 0.18 + 0.2 * ((i as f32 * 0.37).sin() * (i as f32 * 0.11).cos()).abs()).collect(),
        mem: (0..60).map(|i| 0.42 + 0.02 * (i as f32 * 0.2).sin()).collect(),
        net: (0..60).map(|i| 0.25 + 0.35 * ((i as f32 * 0.9).sin().max(0.0))).collect(),
        net_rate: 2.4 * 1048576.0, mem_gib: 4.2, mem_total_gib: 16.0, cpu_temp: Some(64.0), weather: Some(("\u{e30d}".into(), "62°F".into())),
        battery: Battery { percent: 74, charging: true, present: true, on_ac: true },
        time: LocalTime { hour: 14, min: 12, sec: 5, wday: 5, mday: 9, mon: 9 },
        fn_pressed: false, controls: false, ambient: false, idle_secs: 0.0, t: 3.0, unix_secs: 0, touch_id: None, pressed: None,
    }
}

fn save(s: &Scene, marks: &Marks, text: &mut Text, path: &Path) -> Pixmap {
    let mut pm = Pixmap::new(s.width, s.height).unwrap();
    s.render(&mut pm, marks, text);
    pm.save_png(path).unwrap();
    println!("wrote {}", path.display());
    pm
}

pub fn render_all(dir: &Path) {
    let generic = !cfg!(feature = "grok-bot");
    std::fs::create_dir_all(dir).unwrap();
    let marks = Marks::builtin();
    let mut text = Text::load(None);
    let mut s = demo_scene();
    if generic { s.bots.clear(); s.activities = demo_activities(); }
    save(&s, &marks, &mut text, &dir.join("normal.png"));
    if !generic { for i in [0usize, 4] { s.bots[i].1 = true; } // two bots working
    s.bots[0].0.unread = 2; s.bots[3].0.blocked = true; }
    let working = save(&s, &marks, &mut text, &dir.join("working-bot.png"));
    if !generic {
    // animation strip: 8 frames of the bots region, 4x zoom, 1/8 s apart
    let (bx, bw) = (bots_x(&s), 6.0 * (s.cfg.bots.mark_size as f32 + 14.0) + 22.0);
    let frames = 8;
    let z = 4.0;
    let mut strip = Pixmap::new((bw * z) as u32, (60.0 * z) as u32 * frames).unwrap();
    for f in 0..frames {
        s.t = 3.0 + f as f32 * 0.125;
        let mut pm = Pixmap::new(s.width, s.height).unwrap();
        s.render(&mut pm, &marks, &mut text);
        let paint = PixmapPaint { quality: FilterQuality::Nearest, ..Default::default() };
        strip.draw_pixmap(0, 0, pm.as_ref(), &paint, Transform::from_translate(-bx * z, f as f32 * 60.0 * z).pre_scale(z, z), None);
    }
    strip.save_png(dir.join("working-bot-anim-zoom.png")).unwrap();
    println!("wrote {}", dir.join("working-bot-anim-zoom.png").display());
    }
    drop(working);
    s.t = 3.0;
    let mut f = demo_scene(); f.fn_pressed = true; f.pressed = Some(crate::scene::Hit::FKey(5));
    save(&f, &marks, &mut text, &dir.join("fn-layer.png"));
    let mut k = demo_scene(); k.controls = true; k.pressed = Some(crate::scene::Hit::Ctl(6));
    save(&k, &marks, &mut text, &dir.join("controls.png"));
    let mut a = demo_scene(); a.ambient = true; a.t = 40.0;
    save(&a, &marks, &mut text, &dir.join("ambient.png"));
    let mut d = demo_scene(); d.idle_secs = 120.0; d.unix_secs = 240;
    save(&d, &marks, &mut text, &dir.join("dimmed-idle.png"));
    let mut g = demo_scene(); g.bots.clear();
    g.activities = demo_activities();
    save(&g, &marks, &mut text, &dir.join("generic-activity.png"));
    let mut t = demo_scene(); t.touch_id = Some(TouchId::Authenticate);
    save(&t, &marks, &mut text, &dir.join("touch-id.png"));
    // arrow sweep: 6 frames of the right end, stacked
    let mut strip = Pixmap::new(400, 60 * 6).unwrap();
    for f in 0..6 {
        t.t = f as f32 * 0.1;
        let mut pm = Pixmap::new(t.width, t.height).unwrap();
        t.render(&mut pm, &marks, &mut text);
        strip.draw_pixmap(0, 0, pm.as_ref(), &PixmapPaint::default(), Transform::from_translate(400.0 - t.width as f32, f as f32 * 60.0), None);
    }
    strip.save_png(dir.join("touch-id-arrow-anim.png")).unwrap();
}

fn demo_activities() -> Vec<crate::bots::Activity> {
    vec![crate::bots::Activity { name: "claude".into(), label: "Claude Code".into() }, crate::bots::Activity { name: "codex".into(), label: "Codex".into() }]
}

fn bots_x(s: &Scene) -> f32 {
    // esc (92) + workspaces (5*52+18) + left margin 8
    8.0 + 112.0 + s.workspace_ids().len() as f32 * 66.0 + 18.0
}
