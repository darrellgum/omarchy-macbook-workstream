//! Bot roster (Grok Bot desktop persistence) and activity status files.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, PartialEq)]
pub struct Bot {
    pub id: String, pub name: String, pub shape: Option<String>, pub color: Option<String>,
    /// roster `unreadCount` / `hasUnread` (the app's "unread" marker)
    pub unread: u32,
    /// roster `awaitingUserResponse != null` (the app's "blocked" marker; wins over unread)
    pub blocked: bool,
}

pub fn parse_roster(text: &str) -> Vec<Bot> {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return vec![] };
    let rows = v.pointer("/value/rows").or_else(|| v.get("rows")).and_then(Value::as_array);
    rows.map(|rows| {
        rows.iter()
            .filter(|r| !r.get("isGroup").and_then(Value::as_bool).unwrap_or(false))
            .filter_map(|r| Some(Bot {
                id: r.get("id")?.as_str()?.to_string(),
                name: r.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                shape: r.get("avatarShape").and_then(Value::as_str).map(str::to_string),
                color: r.get("avatarColor").and_then(Value::as_str).map(str::to_string),
                unread: r.get("unreadCount").and_then(Value::as_u64).map(|n| n as u32)
                    .unwrap_or(0).max(u32::from(r.get("hasUnread").and_then(Value::as_bool).unwrap_or(false))),
                blocked: r.get("awaitingUserResponse").is_some_and(|v| !v.is_null()),
            }))
            .collect()
    }).unwrap_or_default()
}

fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c { b'a'..=b'z' => c - b'a', b'A'..=b'Z' => c - b'A', b'2'..=b'7' => c - b'2' + 26, b'=' => continue, _ => return None };
        buf = (buf << 5) | v as u32; bits += 5;
        if bits >= 8 { bits -= 8; out.push((buf >> bits) as u8); buf &= (1 << bits) - 1; }
    }
    Some(out)
}

/// Finds `<base32("…roster.last-roster")>.blob` in the persistence dir.
pub fn find_roster(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(stem) = name.strip_suffix(".blob") else { continue };
        let Some(key) = base32_decode(stem).and_then(|b| String::from_utf8(b).ok()) else { continue };
        if key.ends_with(".roster.last-roster") {
            let m = e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            if best.as_ref().is_none_or(|(t, _)| m > *t) { best = Some((m, e.path())); }
        }
    }
    best.map(|(_, p)| p)
}

/// Maps bot id -> `<base32>.blob` transcript replica written by the app.
pub fn find_replicas(dir: &Path) -> std::collections::HashMap<String, PathBuf> {
    let mut m = std::collections::HashMap::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(stem) = name.strip_suffix(".blob") else { continue };
        let Some(key) = base32_decode(stem).and_then(|b| String::from_utf8(b).ok()) else { continue };
        if let Some((_, id)) = key.split_once(".transcript.replicas.") { m.insert(id.to_string(), e.path()); }
    }
    m
}

/// Timestamp (ms) of the newest user message if no bot message (`send-message`) follows it.
pub fn pending_user_message(replica_json: &str) -> Option<u64> {
    let v: Value = serde_json::from_str(replica_json).ok()?;
    let entries = v.pointer("/value/entries")?.as_array()?;
    let last = entries.iter().max_by_key(|e| e.get("seq").and_then(Value::as_u64).unwrap_or(0))?;
    if last.get("kind")?.as_str()? == "message" && last.get("role").and_then(Value::as_str) == Some("user") {
        return last.get("timestampMs")?.as_u64();
    }
    None
}

pub fn matches(b: &Bot, x: &str) -> bool { x.trim().eq_ignore_ascii_case(&b.id) || x.trim().eq_ignore_ascii_case(b.name.trim()) }

/// Applies [bots] show / hide / order. `show` non-empty = only those, in that order.
pub fn select(bots: Vec<Bot>, show: &[String], hide: &[String], order: &[String]) -> Vec<Bot> {
    let pos = |b: &Bot, l: &[String]| l.iter().position(|x| matches(b, x));
    let mut v: Vec<Bot> = bots.into_iter().filter(|b| pos(b, hide).is_none()).collect();
    if !show.is_empty() {
        v.retain(|b| pos(b, show).is_some());
        v.sort_by_key(|b| pos(b, show));
    }
    if !order.is_empty() { v.sort_by_key(|b| pos(b, order).unwrap_or(usize::MAX)); }
    v
}

/// One generic activity entry: `<status_dir>/<name>` containing `working [label]` or `idle`.
#[derive(Clone, Debug, PartialEq)]
pub struct Activity { pub name: String, pub label: String }

/// Working entries in the status dir, excluding names in `skip` (bot ids/slugs shown as marks).
pub fn read_activities(dir: &Path, skip: &[String], stale_after: u64, now: SystemTime) -> Vec<Activity> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || skip.iter().any(|s| s == &name) { continue; }
        let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
        let text = text.trim();
        let (state, label) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
        if !state.eq_ignore_ascii_case("working") { continue; }
        if stale_after > 0 {
            if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
                if now.duration_since(m).unwrap_or_default() > Duration::from_secs(stale_after) { continue; }
            }
        }
        let label: String = label.trim().chars().take(24).collect();
        v.push(Activity { label: if label.is_empty() { name.clone() } else { label }, name });
    }
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v.truncate(6);
    v
}

pub fn slug(name: &str) -> String {
    name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// `<dir>/<bot-id>` (or `<dir>/<name-slug>`) containing `working` or `idle`.
pub fn is_working(dir: &Path, bot: &Bot, stale_after: u64, now: SystemTime) -> bool {
    for f in [dir.join(&bot.id), dir.join(slug(&bot.name))] {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        let working = text.trim().eq_ignore_ascii_case("working");
        if working && stale_after > 0 {
            if let Ok(m) = std::fs::metadata(&f).and_then(|m| m.modified()) {
                if now.duration_since(m).unwrap_or_default() > Duration::from_secs(stale_after) { return false; }
            }
        }
        return working;
    }
    false
}

#[cfg(test)]
mod select_tests {
    use super::*;
    fn b(id: &str, name: &str) -> Bot { Bot { id: id.into(), name: name.into(), shape: None, color: None, unread: 0, blocked: false } }
    #[test]
    fn show_hide_order() {
        let all = vec![b("1", "Reed"), b("2", "Scout"), b("3", "KALE 9000")];
        let names = |v: Vec<Bot>| v.into_iter().map(|b| b.name).collect::<Vec<_>>();
        assert_eq!(names(select(all.clone(), &[], &["scout".into()], &[])), ["Reed", "KALE 9000"]);
        assert_eq!(names(select(all.clone(), &["kale 9000".into(), "1".into()], &[], &[])), ["KALE 9000", "Reed"]);
        assert_eq!(names(select(all.clone(), &[], &[], &["Scout".into()])), ["Scout", "Reed", "KALE 9000"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roster_fixture() {
        let b = parse_roster(include_str!("../tests/fixtures/roster.json"));
        assert_eq!(b.len(), 6);
        assert_eq!(b[0].name, "Atlas");
        assert_eq!(b[0].shape.as_deref(), Some("hex"));
        let s = select(b.clone(), &["echo 9000".into(), "Atlas".into()], &[], &[]);
        assert_eq!(s.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), ["Echo 9000", "Atlas"]);
        assert_eq!(select(b, &[], &["Dune".into()], &[]).len(), 5);
    }
    #[test]
    fn roster_markers_and_pending() {
        let b = parse_roster(r#"{"value":{"rows":[{"id":"a","name":"A","hasUnread":true,"unreadCount":3,"awaitingUserResponse":null},{"id":"b","name":"B","hasUnread":true,"unreadCount":0,"awaitingUserResponse":{"kind":"form"}}]}}"#);
        assert_eq!((b[0].unread, b[0].blocked), (3, false));
        assert_eq!((b[1].unread, b[1].blocked), (1, true));
        let r = r#"{"value":{"entries":[{"kind":"send-message","seq":1,"timestampMs":5},{"kind":"message","role":"user","seq":2,"timestampMs":9}]}}"#;
        assert_eq!(pending_user_message(r), Some(9));
        let r2 = r#"{"value":{"entries":[{"kind":"message","role":"user","seq":2,"timestampMs":9},{"kind":"send-message","seq":3,"timestampMs":12}]}}"#;
        assert_eq!(pending_user_message(r2), None);
    }
    #[test]
    fn base32_key() {
        let k = base32_decode("onqw4zbomnwgszlooqxhg3djmnss4").unwrap();
        assert!(String::from_utf8_lossy(&k).starts_with("sand.client.slice"));
    }
    #[test]
    fn status_files() {
        let d = std::env::temp_dir().join(format!("gtb-status-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let bot = Bot { id: "abc".into(), name: "Echo 9000".into(), shape: None, color: None, unread: 0, blocked: false };
        assert!(!is_working(&d, &bot, 0, SystemTime::now()));
        std::fs::write(d.join("abc"), "working\n").unwrap();
        assert!(is_working(&d, &bot, 0, SystemTime::now()));
        assert!(!is_working(&d, &bot, 10, SystemTime::now() + Duration::from_secs(60)));
        std::fs::write(d.join("abc"), "idle").unwrap();
        assert!(!is_working(&d, &bot, 0, SystemTime::now()));
        std::fs::remove_file(d.join("abc")).unwrap();
        std::fs::write(d.join("echo-9000"), "working").unwrap();
        assert!(is_working(&d, &bot, 0, SystemTime::now()));
        let acts = read_activities(&d, &["echo-9000".into()], 0, SystemTime::now());
        assert!(acts.is_empty());
        std::fs::write(d.join("claude"), "working Refactor\n").unwrap();
        std::fs::write(d.join("codex"), "idle").unwrap();
        let acts = read_activities(&d, &[], 0, SystemTime::now());
        assert_eq!(acts.iter().map(|a| (a.name.as_str(), a.label.as_str())).collect::<Vec<_>>(), [("claude", "Refactor"), ("echo-9000", "echo-9000")]);
        std::fs::remove_dir_all(&d).unwrap();
    }
}
