//! Hyprland IPC: socket2 event stream (workspaces, screensaver window) and dispatch.
use serde_json::Value;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

pub const SCREENSAVER_CLASS: &str = "org.omarchy.screensaver";

#[derive(Clone, Debug, PartialEq)]
pub struct Workspace { pub id: i64, pub windows: u32 }

#[derive(Clone, Debug, PartialEq)]
pub enum HyprEvent {
    Workspaces { list: Vec<Workspace>, active: i64 },
    Screensaver(bool),
    Activity,
}

/// $XDG_RUNTIME_DIR/hypr/<sig>; uses $HYPRLAND_INSTANCE_SIGNATURE, else the newest instance
/// (the renderer runs under systemd and may not inherit the signature).
pub fn instance_dir() -> Option<PathBuf> {
    let rt = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })));
    let base = rt.join("hypr");
    if let Some(sig) = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE") {
        let d = base.join(sig);
        if d.join(".socket2.sock").exists() { return Some(d); }
    }
    std::fs::read_dir(&base).ok()?.flatten()
        .filter(|e| e.path().join(".socket2.sock").exists())
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

pub fn request(cmd: &str) -> Option<String> {
    let mut s = UnixStream::connect(instance_dir()?.join(".socket.sock")).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    s.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    Some(out)
}

pub fn parse_workspaces(json: &str, active_json: &str) -> (Vec<Workspace>, i64) {
    let list = serde_json::from_str::<Value>(json).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default()
        .iter().filter_map(|w| Some(Workspace { id: w.get("id")?.as_i64()?, windows: w.get("windows").and_then(Value::as_u64).unwrap_or(0) as u32 }))
        .filter(|w| w.id > 0).collect();
    let active = serde_json::from_str::<Value>(active_json).ok().and_then(|v| v.get("id")?.as_i64()).unwrap_or(1);
    (list, active)
}

fn query_workspaces() -> Option<HyprEvent> {
    let (list, active) = parse_workspaces(&request("j/workspaces")?, &request("j/activeworkspace")?);
    Some(HyprEvent::Workspaces { list, active })
}

fn query_screensaver(set: &mut HashSet<String>) {
    set.clear();
    if let Some(v) = request("j/clients").and_then(|j| serde_json::from_str::<Value>(&j).ok()) {
        for c in v.as_array().into_iter().flatten() {
            if c.get("class").and_then(Value::as_str) == Some(SCREENSAVER_CLASS) {
                if let Some(a) = c.get("address").and_then(Value::as_str) { set.insert(a.trim_start_matches("0x").to_string()); }
            }
        }
    }
}

/// Applies one socket2 line; returns (refresh workspaces?, screensaver changed?).
pub fn handle_line(line: &str, screensavers: &mut HashSet<String>) -> (bool, bool) {
    let Some((ev, data)) = line.split_once(">>") else { return (false, false) };
    let before = !screensavers.is_empty();
    match ev {
        "openwindow" => {
            let mut f = data.splitn(4, ',');
            let (addr, _ws, class) = (f.next().unwrap_or(""), f.next(), f.next().unwrap_or(""));
            if class == SCREENSAVER_CLASS { screensavers.insert(addr.trim_start_matches("0x").to_string()); }
        }
        "closewindow" => { screensavers.remove(data.trim().trim_start_matches("0x")); }
        _ => {}
    }
    let refresh = matches!(ev, "workspace" | "workspacev2" | "createworkspace" | "createworkspacev2" | "destroyworkspace"
        | "destroyworkspacev2" | "focusedmon" | "focusedmonv2" | "moveworkspace" | "moveworkspacev2" | "openwindow"
        | "closewindow" | "movewindow" | "movewindowv2" | "renameworkspace" | "configreloaded");
    (refresh, before != !screensavers.is_empty())
}

pub fn spawn_watcher(tx: Sender<HyprEvent>) {
    std::thread::spawn(move || loop {
        let mut ss = HashSet::new();
        if let Some(dir) = instance_dir() {
            if let Ok(stream) = UnixStream::connect(dir.join(".socket2.sock")) {
                if let Some(e) = query_workspaces() { let _ = tx.send(e); }
                query_screensaver(&mut ss);
                let _ = tx.send(HyprEvent::Screensaver(!ss.is_empty()));
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    let (refresh, ss_changed) = handle_line(&line, &mut ss);
                    if refresh { if let Some(e) = query_workspaces() { if tx.send(e).is_err() { return; } } }
                    if ss_changed { let _ = tx.send(HyprEvent::Screensaver(!ss.is_empty())); }
                    if tx.send(HyprEvent::Activity).is_err() { return; }
                }
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    });
}

pub const GROK_CLASS: &str = "grok-bot";
pub const GROK_LAUNCH: &str = "/opt/Grok Bot/grok-bot";

/// Tap on a bot mark: ONLY focus (or launch) the Grok Bot app. Never sends keystrokes or text.
/// This app version registers grokbot:// routes only for bot-template/plugin-add/open/settings,
/// so there is no per-agent deep link to navigate to a specific chat.
pub fn open_bot(_bot_name: String, _navigate: bool) {
    std::thread::spawn(focus_or_launch_grok_sync);
}

/// Focus the Grok Bot window if one exists, else have Hyprland launch the app
/// (so it lives in the session, not in the t1-touchbar service cgroup).

fn focus_or_launch_grok_sync() {
    {
        let running = request("j/clients").and_then(|j| serde_json::from_str::<Value>(&j).ok())
            .and_then(|v| v.as_array().map(|a| a.iter().any(|c| c.get("class").and_then(Value::as_str) == Some(GROK_CLASS))))
            .unwrap_or(false);
        let ok = |r: Option<String>| r.is_some_and(|r| r.trim() == "ok");
        if running {
            if !ok(request(&format!("dispatch hl.dsp.focus({{ window = \"class:{GROK_CLASS}\" }})"))) {
                let _ = request(&format!("dispatch focuswindow class:{GROK_CLASS}"));
            }
        } else if !ok(request(&format!("dispatch hl.dsp.exec_cmd([[\"{GROK_LAUNCH}\"]])"))) {
            let _ = request(&format!("dispatch exec \"{GROK_LAUNCH}\""));
        }
    }
}

/// Omarchy 4 Lua syntax first, classic dispatcher as fallback (as omarchy's own scripts do).
pub fn switch_workspace(n: i64) {
    std::thread::spawn(move || {
        let ok = request(&format!("dispatch hl.dsp.focus({{ workspace = {n} }})")).is_some_and(|r| r.trim() == "ok");
        if !ok { let _ = request(&format!("dispatch workspace {n}")); }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_workspaces() {
        let (l, a) = parse_workspaces(r#"[{"id":1,"windows":2},{"id":3,"windows":0},{"id":-98,"name":"special:x","windows":1}]"#, r#"{"id":3}"#);
        assert_eq!(l, vec![Workspace { id: 1, windows: 2 }, Workspace { id: 3, windows: 0 }]);
        assert_eq!(a, 3);
    }
    #[test]
    fn screensaver_events() {
        let mut s = HashSet::new();
        assert_eq!(handle_line("openwindow>>55aa,special:screensaver-eDP-1,org.omarchy.screensaver,Screensaver", &mut s), (true, true));
        assert_eq!(handle_line("openwindow>>66bb,1,Alacritty,zsh", &mut s), (true, false));
        assert_eq!(handle_line("closewindow>>66bb", &mut s), (true, false));
        assert_eq!(handle_line("closewindow>>55aa", &mut s), (true, true));
        assert_eq!(handle_line("workspacev2>>2,2", &mut s), (true, false));
        assert_eq!(handle_line("activewindow>>a,b", &mut s), (false, false));
    }
}
