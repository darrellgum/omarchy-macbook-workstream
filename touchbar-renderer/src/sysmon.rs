//! CPU / memory / network / battery sampling from /proc and /sys.
use std::collections::VecDeque;
use std::path::PathBuf;

pub const HISTORY: usize = 60;

#[derive(Clone, Debug, Default)]
pub struct Battery { pub percent: u8, pub charging: bool, pub present: bool, pub on_ac: bool }

pub struct SysMon {
    root: PathBuf,
    last_cpu: Option<(u64, u64)>,
    last_net: Option<u64>,
    pub cpu: VecDeque<f32>,
    pub mem: VecDeque<f32>,
    pub net: VecDeque<f32>,
    pub net_rate: f64,
    pub mem_used_gib: f32,
    pub mem_total_gib: f32,
    pub cpu_temp_c: Option<f32>,
    pub battery: Battery,
}

pub fn parse_cpu(stat: &str) -> Option<(u64, u64)> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    if v.len() < 4 { return None; }
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    Some((v.iter().sum(), idle))
}

pub fn parse_mem(meminfo: &str) -> Option<(u64, u64)> {
    let get = |k: &str| meminfo.lines().find(|l| l.starts_with(k))?.split_whitespace().nth(1)?.parse::<u64>().ok();
    Some((get("MemTotal:")?, get("MemAvailable:")?))
}

pub fn parse_net(dev: &str) -> u64 {
    dev.lines().skip(2).filter_map(|l| {
        let (name, rest) = l.split_once(':')?;
        if name.trim() == "lo" { return None; }
        let v: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        Some(v.first()? + v.get(8)?)
    }).sum()
}

fn push(q: &mut VecDeque<f32>, v: f32) { if q.len() >= HISTORY { q.pop_front(); } q.push_back(v); }

impl SysMon {
    pub fn new(root: PathBuf) -> SysMon {
        SysMon { root, last_cpu: None, last_net: None, cpu: VecDeque::new(), mem: VecDeque::new(), net: VecDeque::new(), net_rate: 0.0, mem_used_gib: 0.0, mem_total_gib: 0.0, cpu_temp_c: None, battery: Battery::default() }
    }
    fn read(&self, p: &str) -> String { std::fs::read_to_string(self.root.join(p)).unwrap_or_default() }

    /// Call once per `dt` seconds.
    pub fn sample(&mut self, dt: f64) {
        if let Some((total, idle)) = parse_cpu(&self.read("proc/stat")) {
            if let Some((lt, li)) = self.last_cpu {
                let (dt_, di) = (total.saturating_sub(lt), idle.saturating_sub(li));
                if dt_ > 0 { push(&mut self.cpu, 1.0 - di as f32 / dt_ as f32); }
            }
            self.last_cpu = Some((total, idle));
        }
        if let Some((t, a)) = parse_mem(&self.read("proc/meminfo")) {
            if t > 0 { push(&mut self.mem, 1.0 - a as f32 / t as f32); self.mem_used_gib = (t - a) as f32 / 1048576.0; self.mem_total_gib = t as f32 / 1048576.0; }
        }
        let n = parse_net(&self.read("proc/net/dev"));
        if let Some(l) = self.last_net {
            self.net_rate = n.saturating_sub(l) as f64 / dt.max(0.001);
            // log scale: 1 KiB/s .. 100 MiB/s
            let v = ((self.net_rate.max(1.0).log10() - 3.0) / 5.0).clamp(0.0, 1.0) as f32;
            push(&mut self.net, v);
        }
        self.last_net = Some(n);
        self.battery = self.read_battery();
        self.cpu_temp_c = self.read_cpu_temp();
    }

    /// hwmon `coretemp` package temperature (temp1), else the hottest coretemp/k10temp input.
    fn read_cpu_temp(&self) -> Option<f32> {
        for e in std::fs::read_dir(self.root.join("sys/class/hwmon")).ok()?.flatten() {
            let name = std::fs::read_to_string(e.path().join("name")).unwrap_or_default();
            if !matches!(name.trim(), "coretemp" | "k10temp" | "zenpower") { continue; }
            let v = std::fs::read_to_string(e.path().join("temp1_input")).ok()?.trim().parse::<f32>().ok()?;
            return Some(v / 1000.0);
        }
        None
    }

    fn read_battery(&self) -> Battery {
        let dir = self.root.join("sys/class/power_supply");
        let Ok(entries) = std::fs::read_dir(&dir) else { return Battery::default() };
        let entries: Vec<_> = entries.flatten().collect();
        let on_ac = entries.iter().any(|e| {
            let p = e.path();
            std::fs::read_to_string(p.join("type")).map(|t| t.trim() == "Mains").unwrap_or_else(|_| e.file_name().to_string_lossy().starts_with("ADP") || e.file_name().to_string_lossy().starts_with("AC"))
                && std::fs::read_to_string(p.join("online")).is_ok_and(|o| o.trim() == "1")
        });
        for e in entries {
            if !e.file_name().to_string_lossy().starts_with("BAT") { continue; }
            let cap = std::fs::read_to_string(e.path().join("capacity")).unwrap_or_default();
            let st = std::fs::read_to_string(e.path().join("status")).unwrap_or_default();
            if let Ok(p) = cap.trim().parse::<u8>() {
                return Battery { percent: p.min(100), charging: matches!(st.trim(), "Charging" | "Full"), present: true, on_ac };
            }
        }
        Battery::default()
    }
}

pub fn human_rate(bps: f64) -> String {
    if bps >= 1048576.0 { format!("{:.1}M", bps / 1048576.0) } else if bps >= 1024.0 { format!("{:.0}K", bps / 1024.0) } else { format!("{:.0}B", bps) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parsers() {
        assert_eq!(parse_cpu("cpu  10 0 10 70 10 0 0 0 0 0\ncpu0 1 2 3 4\n"), Some((100, 80)));
        assert_eq!(parse_mem("MemTotal: 1000 kB\nMemFree: 1 kB\nMemAvailable: 250 kB\n"), Some((1000, 250)));
        let dev = "h1\nh2\n    lo: 100 0 0 0 0 0 0 0 100 0 0 0 0 0 0 0\n  wlan0: 1000 5 0 0 0 0 0 0 500 3 0 0 0 0 0 0\n";
        assert_eq!(parse_net(dev), 1500);
        assert_eq!(human_rate(2.5 * 1048576.0), "2.5M");
    }
    #[test]
    fn samples_fake_root() {
        let r = std::env::temp_dir().join(format!("gtb-sys-{}", std::process::id()));
        std::fs::create_dir_all(r.join("proc/net")).unwrap();
        std::fs::create_dir_all(r.join("sys/class/power_supply/BAT0")).unwrap();
        std::fs::write(r.join("proc/stat"), "cpu  0 0 0 100 0\n").unwrap();
        std::fs::write(r.join("proc/meminfo"), "MemTotal: 100 kB\nMemAvailable: 25 kB\n").unwrap();
        std::fs::write(r.join("proc/net/dev"), "a\nb\n eth0: 0 0 0 0 0 0 0 0 0 0\n").unwrap();
        std::fs::write(r.join("sys/class/power_supply/BAT0/capacity"), "29\n").unwrap();
        std::fs::write(r.join("sys/class/power_supply/BAT0/status"), "Discharging\n").unwrap();
        std::fs::create_dir_all(r.join("sys/class/power_supply/ADP1")).unwrap();
        std::fs::write(r.join("sys/class/power_supply/ADP1/online"), "1\n").unwrap();
        std::fs::create_dir_all(r.join("sys/class/hwmon/hwmon5")).unwrap();
        std::fs::write(r.join("sys/class/hwmon/hwmon5/name"), "coretemp\n").unwrap();
        std::fs::write(r.join("sys/class/hwmon/hwmon5/temp1_input"), "64000\n").unwrap();
        let mut m = SysMon::new(r.clone());
        m.sample(1.0);
        std::fs::write(r.join("proc/stat"), "cpu  50 0 0 150 0\n").unwrap();
        m.sample(1.0);
        assert!((m.cpu[0] - 0.5).abs() < 1e-6);
        assert!((m.mem[1] - 0.75).abs() < 1e-6);
        assert_eq!(m.battery.percent, 29);
        assert_eq!(m.cpu_temp_c, Some(64.0));
        assert!(!m.battery.charging);
        assert!(m.battery.on_ac);
        std::fs::remove_dir_all(&r).unwrap();
    }
}
