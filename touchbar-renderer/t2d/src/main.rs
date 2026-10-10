//! t1-dash-t2d: lets t1-dash (or any T1Bridge renderer) drive a T2 MacBook Touch Bar.
//!
//! T2 Macs have no T1Bridge. The Touch Bar there is a USB device whose second configuration
//! exposes a display (appletbdrm, a DRM card with a 60 x 2008 portrait mode) and a multitouch
//! digitizer ("Touch Bar Display Touchpad"). This root daemon owns those, and serves the same
//! SOCK_SEQPACKET protocol T1Bridge offers renderers (protocol v1.0, the subset t1-dash uses):
//! Hello, RegisterBuffer (memfd), SubmitFrame, TapKeys (sent through uinput), plus InputFrame
//! events with touches and the Fn key state. The renderer keeps running as the desktop user.
use drm::buffer::{Buffer, DrmFourcc};
use drm::control::{connector, dumbbuffer::DumbBuffer, framebuffer, ClipRect, Device as ControlDevice};
use drm::Device as DrmDevice;
use evdev::{AbsoluteAxisType, AttributeSet, Device, EventType, InputEvent, Key};
use std::collections::{BTreeMap, HashMap};
use std::fs::{File, OpenOptions};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::time::Instant;

const HELLO: u16 = 0x0001;
const REGISTER_BUFFER: u16 = 0x0002;
const SUBMIT_FRAME: u16 = 0x0003;
const TAP_KEYS: u16 = 0x0004;
const HELLO_ACK: u16 = 0x8001;
const ACK: u16 = 0x8002;
const FRAME_RELEASED: u16 = 0x9001;
const INPUT_FRAME: u16 = 0x9002;
const KEYS: [u16; 13] = [1, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 87, 88];

fn log(msg: impl AsRef<str>) { eprintln!("t1-dash-t2d: {}", msg.as_ref()); }
fn die(msg: impl AsRef<str>) -> ! { log(msg); std::process::exit(1) }

// ---------- DRM output ----------
struct Card(File);
impl AsFd for Card { fn as_fd(&self) -> BorrowedFd<'_> { self.0.as_fd() } }
impl DrmDevice for Card {}
impl ControlDevice for Card {}

struct Output { card: Option<(Card, DumbBuffer, framebuffer::Handle)>, mode: (u16, u16), fake: Vec<u8> }

impl Output {
    fn open() -> Result<Output, String> {
        let mut tried = vec![];
        for e in std::fs::read_dir("/dev/dri").map_err(|e| e.to_string())?.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("card") { continue; }
            let Ok(f) = OpenOptions::new().read(true).write(true).open(e.path()) else { continue };
            let card = Card(f);
            let drv = card.get_driver().map(|d| d.name().to_string_lossy().to_string()).unwrap_or_default();
            if drv != "appletbdrm" { tried.push(format!("{name}={drv}")); continue; }
            return Output::setup(card).map_err(|e| format!("{name}: {e}"));
        }
        Err(format!("no appletbdrm card (saw {})", tried.join(", ")))
    }
    fn setup(card: Card) -> Result<Output, String> {
        let s = |e: std::io::Error| e.to_string();
        let _ = card.acquire_master_lock();
        let res = card.resource_handles().map_err(s)?;
        let con = res.connectors().iter().flat_map(|c| card.get_connector(*c, true))
            .find(|c| c.state() == connector::State::Connected).ok_or("no connected connector")?;
        let mode = *con.modes().first().ok_or("connector has no mode")?;
        let crtc = *res.crtcs().first().ok_or("no crtc")?;
        let (w, h) = mode.size();
        let mut db = card.create_dumb_buffer((w as u32, h as u32), DrmFourcc::Xrgb8888, 32).map_err(s)?;
        { let mut m = card.map_dumb_buffer(&mut db).map_err(s)?; m.as_mut().fill(0); }
        let fb = card.add_framebuffer(&db, 24, 32).map_err(s)?;
        card.set_crtc(crtc, Some(fb), (0, 0), &[con.handle()], Some(mode)).map_err(s)?;
        Ok(Output { card: Some((card, db, fb)), mode: mode.size(), fake: vec![] })
    }
    /// Physical (width, height) of the scanout, e.g. (60, 2008).
    /// Test mode without hardware: frames land in a file as raw XRGB8888 portrait scanout.
    fn fake(w: u16, h: u16) -> Output { Output { card: None, mode: (w, h), fake: vec![0; w as usize * h as usize * 4] } }
    fn phys(&self) -> (u32, u32) { let (w, h) = self.mode; (w as u32, h as u32) }
    /// Logical landscape size the renderer draws, e.g. 2008 x 60.
    fn logical(&self) -> (u32, u32) { let (w, h) = self.phys(); if w < h { (h, w) } else { (w, h) } }
    /// Copy one XRGB8888 landscape frame (stride `src_stride`) into the scanout buffer.
    fn present(&mut self, src: &[u8], src_stride: usize) -> Result<(), String> {
        let (pw, ph) = self.phys();
        let (lw, lh) = self.logical();
        let rotated = pw < ph;
        let pitch = match &self.card { Some((_, db, _)) => db.pitch() as usize, None => pw as usize * 4 };
        let mut fake = std::mem::take(&mut self.fake);
        let mut mapping = match &mut self.card {
            Some((card, db, _)) => Some(card.map_dumb_buffer(db).map_err(|e| e.to_string())?),
            None => None,
        };
        {
            let dst: &mut [u8] = match &mut mapping { Some(m) => m.as_mut(), None => &mut fake };
            for y in 0..lh as usize {
                let row = &src[y * src_stride..y * src_stride + lw as usize * 4];
                if rotated {
                    // same orientation as tiny-dfr: logical (x, y) lands at column (pw - 1 - y), row x
                    let col = (pw as usize - 1 - y) * 4;
                    for x in 0..lw as usize {
                        let d = x * pitch + col;
                        dst[d..d + 4].copy_from_slice(&row[x * 4..x * 4 + 4]);
                    }
                } else {
                    dst[y * pitch..y * pitch + row.len()].copy_from_slice(row);
                }
            }
        }
        drop(mapping);
        match &self.card {
            Some((card, _, fb)) => card.dirty_framebuffer(*fb, &[ClipRect::new(0, 0, pw as u16, ph as u16)]).map_err(|e| e.to_string()),
            None => { let r = std::fs::write("/tmp/t1-dash-t2d-fake.raw", &fake).map_err(|e| e.to_string()); self.fake = fake; r }
        }
    }
}

// ---------- input ----------
fn nonblock(fd: RawFd) { unsafe { let f = libc::fcntl(fd, libc::F_GETFL); libc::fcntl(fd, libc::F_SETFL, f | libc::O_NONBLOCK); } }

struct Touch { dev: Device, xr: (i32, i32), yr: (i32, i32), slot: usize, slots: BTreeMap<usize, (i32, i32, i32)> }

fn input_devices() -> Vec<(PathBuf, Device)> {
    let mut v: Vec<_> = evdev::enumerate().collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn find_touch() -> Option<Touch> {
    for (path, dev) in input_devices() {
        let name = dev.name().unwrap_or("").to_string();
        let id = dev.input_id();
        let mt = dev.supported_absolute_axes().map_or(false, |a| a.contains(AbsoluteAxisType::ABS_MT_POSITION_X));
        if mt && (name.contains("Touch Bar") || (id.vendor() == 0x05ac && id.product() == 0x8302)) {
            let abs = dev.get_abs_state().ok()?;
            let r = |a: AbsoluteAxisType| (abs[a.0 as usize].minimum, abs[a.0 as usize].maximum);
            let (xr, yr) = (r(AbsoluteAxisType::ABS_MT_POSITION_X), r(AbsoluteAxisType::ABS_MT_POSITION_Y));
            log(format!("touch: {} ({name}) x {:?} y {:?}", path.display(), xr, yr));
            nonblock(dev.as_raw_fd());
            return Some(Touch { dev, xr, yr, slot: 0, slots: BTreeMap::new() });
        }
    }
    None
}

fn find_fn_keyboards() -> Vec<Device> {
    let mut v = vec![];
    for (path, dev) in input_devices() {
        if dev.name().unwrap_or("").starts_with("t1-dash") { continue; }
        if dev.supported_keys().map_or(false, |k| k.contains(Key::KEY_FN)) {
            log(format!("fn key: {} ({})", path.display(), dev.name().unwrap_or("?")));
            nonblock(dev.as_raw_fd());
            v.push(dev);
        }
    }
    v
}

// ---------- protocol ----------
fn pkt(t: u16, rid: u32, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(16 + payload.len());
    v.extend_from_slice(b"T1HW"); v.extend_from_slice(&1u16.to_le_bytes()); v.extend_from_slice(&t.to_le_bytes());
    v.extend_from_slice(&(payload.len() as u32).to_le_bytes()); v.extend_from_slice(&rid.to_le_bytes());
    v.extend_from_slice(payload); v
}
fn u32at(b: &[u8], o: usize) -> u32 { b.get(o..o + 4).map_or(0, |s| u32::from_le_bytes(s.try_into().unwrap())) }
fn u64at(b: &[u8], o: usize) -> u64 { b.get(o..o + 8).map_or(0, |s| u64::from_le_bytes(s.try_into().unwrap())) }

fn send(fd: &OwnedFd, data: &[u8]) -> bool {
    unsafe { libc::send(fd.as_raw_fd(), data.as_ptr().cast(), data.len(), libc::MSG_NOSIGNAL) == data.len() as isize }
}

/// recvmsg one packet plus at most one passed descriptor. None = closed/error, Some(empty) = would block.
fn recv(fd: &OwnedFd) -> Option<(Vec<u8>, Option<OwnedFd>)> {
    let mut buf = vec![0u8; 4096];
    let mut cbuf = [0u8; 64];
    let mut iov = libc::iovec { iov_base: buf.as_mut_ptr().cast(), iov_len: buf.len() };
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov; msg.msg_iovlen = 1;
    msg.msg_control = cbuf.as_mut_ptr().cast(); msg.msg_controllen = cbuf.len() as _;
    let n = unsafe { libc::recvmsg(fd.as_raw_fd(), &mut msg, libc::MSG_DONTWAIT | libc::MSG_CMSG_CLOEXEC) };
    if n < 0 {
        let e = std::io::Error::last_os_error().raw_os_error();
        return if e == Some(libc::EAGAIN) { Some((vec![], None)) } else { None };
    }
    if n == 0 { return None; }
    buf.truncate(n as usize);
    let mut passed = None;
    unsafe {
        let mut c = libc::CMSG_FIRSTHDR(&msg);
        while !c.is_null() {
            if (*c).cmsg_level == libc::SOL_SOCKET && (*c).cmsg_type == libc::SCM_RIGHTS {
                let fdp = libc::CMSG_DATA(c) as *const RawFd;
                let count = ((*c).cmsg_len as usize - libc::CMSG_LEN(0) as usize) / 4;
                for i in 0..count {
                    let f = OwnedFd::from_raw_fd(std::ptr::read_unaligned(fdp.add(i)));
                    if passed.is_none() { passed = Some(f); }
                }
            }
            c = libc::CMSG_NXTHDR(&msg, c);
        }
    }
    Some((buf, passed))
}

fn peer_uid(fd: &OwnedFd) -> Option<u32> {
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let r = unsafe { libc::getsockopt(fd.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&mut cred as *mut libc::ucred).cast(), &mut len) };
    (r == 0).then_some(cred.uid)
}

fn listen(path: &Path) -> OwnedFd {
    if let Some(dir) = path.parent() { let _ = std::fs::create_dir_all(dir); }
    let _ = std::fs::remove_file(path);
    unsafe {
        let fd = libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK, 0);
        if fd < 0 { die("socket failed"); }
        let fd = OwnedFd::from_raw_fd(fd);
        let mut addr: libc::sockaddr_un = std::mem::zeroed();
        addr.sun_family = libc::AF_UNIX as _;
        for (i, b) in path.as_os_str().as_encoded_bytes().iter().enumerate() { addr.sun_path[i] = *b as _; }
        if libc::bind(fd.as_raw_fd(), (&addr as *const libc::sockaddr_un).cast(), std::mem::size_of::<libc::sockaddr_un>() as _) != 0 {
            die(format!("bind {}: {}", path.display(), std::io::Error::last_os_error()));
        }
        // anyone may connect; the peer uid is checked on accept
        let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        libc::chmod(c.as_ptr(), 0o666);
        if libc::listen(fd.as_raw_fd(), 4) != 0 { die("listen failed"); }
        fd
    }
}

struct Buf { ptr: *const u8, len: usize, stride: usize }
impl Drop for Buf { fn drop(&mut self) { unsafe { libc::munmap(self.ptr as *mut _, self.len); } } }

/// `ready` turns on at the first SubmitFrame: input events are only sent after the handshake.
struct Client { fd: OwnedFd, bufs: HashMap<u32, Buf>, ready: bool }

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let sock = PathBuf::from(arg("--socket").unwrap_or("/run/t1-dash-t2/touchbar.sock".into()));
    let allow: Vec<u32> = arg("--allow-uid").map(|s| s.split(',').filter_map(|u| u.parse().ok()).collect()).unwrap_or_default();

    let mut out = match arg("--fake") {
        Some(sz) => { let (w, h) = sz.split_once('x').unwrap_or_else(|| die("--fake WxH")); Output::fake(w.parse().unwrap(), h.parse().unwrap()) }
        None => Output::open().unwrap_or_else(|e| die(e)),
    };
    let (lw, lh) = out.logical();
    log(format!("display: appletbdrm mode {:?} -> logical {}x{}", out.phys(), lw, lh));
    if args.iter().any(|a| a == "--probe") {
        let _ = find_touch(); let _ = find_fn_keyboards();
        return;
    }
    let mut touch = find_touch();
    if touch.is_none() { log("no Touch Bar digitizer found; display only"); }
    let mut fnkbds = find_fn_keyboards();
    let mut keys = AttributeSet::<Key>::new();
    for k in KEYS { keys.insert(Key::new(k)); }
    let mut uinput = evdev::uinput::VirtualDeviceBuilder::new().and_then(|b| b.name("t1-dash Touch Bar keys").with_keys(&keys)).and_then(|b| b.build())
        .map_err(|e| log(format!("uinput unavailable, Esc/F-keys disabled: {e}"))).ok();
    let listener = listen(&sock);
    log(format!("listening on {} (allowed uids {:?})", sock.display(), allow));

    let mut client: Option<Client> = None;
    let mut fn_down: HashMap<usize, bool> = HashMap::new();
    let mut last_input: Option<(bool, Vec<(u8, u32, u32)>)> = None;
    let start = Instant::now();
    loop {
        let mut pfds = vec![libc::pollfd { fd: listener.as_raw_fd(), events: libc::POLLIN, revents: 0 }];
        if let Some(c) = &client { pfds.push(libc::pollfd { fd: c.fd.as_raw_fd(), events: libc::POLLIN, revents: 0 }); }
        if let Some(t) = &touch { pfds.push(libc::pollfd { fd: t.dev.as_raw_fd(), events: libc::POLLIN, revents: 0 }); }
        for k in &fnkbds { pfds.push(libc::pollfd { fd: k.as_raw_fd(), events: libc::POLLIN, revents: 0 }); }
        unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as _, 1000); }

        // new connection replaces the old one
        let nfd = unsafe { libc::accept4(listener.as_raw_fd(), std::ptr::null_mut(), std::ptr::null_mut(), libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK) };
        if nfd >= 0 {
            let fd = unsafe { OwnedFd::from_raw_fd(nfd) };
            match peer_uid(&fd) {
                Some(u) if u == 0 || allow.contains(&u) => { log(format!("renderer connected (uid {u})")); client = Some(Client { fd, bufs: HashMap::new(), ready: false }); last_input = None; }
                u => log(format!("refused connection from uid {u:?}")),
            }
        }

        // renderer requests
        let mut drop_client = false;
        if let Some(c) = &mut client {
            loop {
                let Some((msg, passed)) = recv(&c.fd) else { drop_client = true; break };
                if msg.is_empty() { break; }
                if msg.len() < 16 || &msg[0..4] != b"T1HW" { drop_client = true; break; }
                let t = u16::from_le_bytes([msg[6], msg[7]]);
                if std::env::var_os("T1_DASH_T2D_DEBUG").is_some() { log(format!("req {t:#06x} len {} fd {}", msg.len(), passed.is_some())); }
                let rid = u32at(&msg, 12);
                let p = &msg[16..];
                let reply = match t {
                    HELLO => {
                        let mut a = vec![0u8; 4];
                        for v in [lw, lh, 1, 3] { a.extend_from_slice(&v.to_le_bytes()); }
                        pkt(HELLO_ACK, rid, &a)
                    }
                    REGISTER_BUFFER => {
                        let (id, stride, len) = (u32at(p, 0), u32at(p, 4) as usize, u64at(p, 8) as usize);
                        if let Some(f) = passed {
                            let ptr = unsafe { libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_SHARED, f.as_raw_fd(), 0) };
                            if ptr != libc::MAP_FAILED && stride >= lw as usize * 4 && len >= stride * lh as usize {
                                c.bufs.insert(id, Buf { ptr: ptr as *const u8, len, stride });
                            } else { log("bad buffer registration"); }
                        }
                        pkt(ACK, rid, &[])
                    }
                    SUBMIT_FRAME => {
                        let (id, fid) = (u32at(p, 0), u64at(p, 4));
                        c.ready = true;
                        if let Some(b) = c.bufs.get(&id) {
                            let src = unsafe { std::slice::from_raw_parts(b.ptr, b.len) };
                            if let Err(e) = out.present(src, b.stride) { die(format!("present: {e}")); }
                        }
                        let ok = send(&c.fd, &pkt(ACK, rid, &[]));
                        let mut r = id.to_le_bytes().to_vec(); r.extend_from_slice(&fid.to_le_bytes());
                        if !ok { drop_client = true; break; }
                        pkt(FRAME_RELEASED, 0, &r)
                    }
                    TAP_KEYS => {
                        let n = (p.first().copied().unwrap_or(0) as usize).min(4);
                        for i in 0..n {
                            let code = u16::from_le_bytes([p[4 + 2 * i], p[5 + 2 * i]]);
                            log(format!("key {code}"));
                            if let (true, Some(u)) = (KEYS.contains(&code), uinput.as_mut()) {
                                let _ = u.emit(&[InputEvent::new(EventType::KEY, code, 1)]);
                                let _ = u.emit(&[InputEvent::new(EventType::KEY, code, 0)]);
                            }
                        }
                        pkt(ACK, rid, &[])
                    }
                    _ => pkt(ACK, rid, &[]), // brightness, backlight, Touch ID cancel: nothing to do here
                };
                if !send(&c.fd, &reply) { drop_client = true; break; }
            }
        }
        if drop_client { log("renderer disconnected"); client = None; }

        // Fn state from any keyboard that has a Fn key
        for (i, k) in fnkbds.iter_mut().enumerate() {
            match k.fetch_events() {
                Ok(evs) => for e in evs { if e.event_type() == EventType::KEY && e.code() == Key::KEY_FN.code() { fn_down.insert(i, e.value() != 0); } },
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => die(format!("keyboard read: {e}")),
            }
        }
        let fn_pressed = fn_down.values().any(|v| *v);

        // touches
        let mut contacts = last_input.as_ref().map(|l| l.1.clone()).unwrap_or_default();
        if let Some(t) = &mut touch {
            let mut synced = false;
            match t.dev.fetch_events() {
                Ok(evs) => for e in evs {
                    match (e.event_type(), e.code()) {
                        (EventType::ABSOLUTE, c) if c == AbsoluteAxisType::ABS_MT_SLOT.0 => t.slot = e.value().max(0) as usize,
                        (EventType::ABSOLUTE, c) if c == AbsoluteAxisType::ABS_MT_TRACKING_ID.0 => {
                            if e.value() < 0 { t.slots.remove(&t.slot); } else { t.slots.entry(t.slot).or_insert((e.value(), -1, -1)).0 = e.value(); }
                        }
                        (EventType::ABSOLUTE, c) if c == AbsoluteAxisType::ABS_MT_POSITION_X.0 => { if let Some(s) = t.slots.get_mut(&t.slot) { s.1 = e.value(); } }
                        (EventType::ABSOLUTE, c) if c == AbsoluteAxisType::ABS_MT_POSITION_Y.0 => { if let Some(s) = t.slots.get_mut(&t.slot) { s.2 = e.value(); } }
                        (EventType::SYNCHRONIZATION, 0) => synced = true,
                        _ => {}
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => die(format!("touch read: {e}")),
            }
            if synced {
                let scale = |v: i32, (lo, hi): (i32, i32), n: u32| (((v - lo).max(0) as i64 * n as i64) / ((hi - lo).max(1) as i64 + 1)).min(n as i64 - 1) as u32;
                contacts = t.slots.iter().filter(|(_, s)| s.1 >= 0 && s.2 >= 0).take(10)
                    .map(|(slot, s)| ((*slot as u8) & 15, scale(s.1, t.xr, lw), scale(s.2, t.yr, lh))).collect();
            }
        }
        let state = (fn_pressed, contacts);
        if let Some(c) = client.as_ref().filter(|c| c.ready) {
            if last_input.as_ref() != Some(&state) {
                let mut p = (start.elapsed().as_nanos() as u64).to_le_bytes().to_vec();
                p.push(state.0 as u8); p.push(state.1.len() as u8); p.extend_from_slice(&[0, 0]);
                for (id, x, y) in &state.1 { p.extend_from_slice(&[*id, 1, 1, 0]); p.extend_from_slice(&x.to_le_bytes()); p.extend_from_slice(&y.to_le_bytes()); }
                if send(&c.fd, &pkt(INPUT_FRAME, 0, &p)) { last_input = Some(state); }
            }
        } else if client.is_none() { last_input = None; }
        if fnkbds.is_empty() && start.elapsed().as_secs() % 30 == 0 { fnkbds = find_fn_keyboards(); }
    }
}
