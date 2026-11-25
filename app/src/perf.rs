//! Leistungs-Feinschliff während eines Streams: höhere Priorität, feinerer Zeittakt und kein Energiesparen für den Prozess.
//! Beim Beenden des Streams wird alles zurückgesetzt.

use std::sync::atomic::{AtomicBool, Ordering};

static ACTIVE: AtomicBool = AtomicBool::new(false);

#[repr(C)]
struct PowerThrottling {
    version: u32,
    control_mask: u32,
    state_mask: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> *mut std::ffi::c_void;
    fn SetPriorityClass(h: *mut std::ffi::c_void, class: u32) -> i32;
    fn SetProcessInformation(h: *mut std::ffi::c_void, class: i32, info: *const std::ffi::c_void, size: u32) -> i32;
}

#[link(name = "winmm")]
extern "system" {
    fn timeBeginPeriod(ms: u32) -> u32;
    fn timeEndPeriod(ms: u32) -> u32;
}

const ABOVE_NORMAL: u32 = 0x0000_8000;
const NORMAL: u32 = 0x0000_0020;
const PROCESS_POWER_THROTTLING: i32 = 4;

fn throttling(off: bool) {
    // ControlMask 1 = wir bestimmen das Verhalten, StateMask 0 = Drosselung aus
    let st = PowerThrottling { version: 1, control_mask: if off { 1 } else { 0 }, state_mask: 0 };
    unsafe {
        SetProcessInformation(GetCurrentProcess(), PROCESS_POWER_THROTTLING, &st as *const _ as *const std::ffi::c_void, std::mem::size_of::<PowerThrottling>() as u32);
    }
}

/// Schaltet die Stream-Leistung ein oder aus (mehrfaches Einschalten ist harmlos).
pub fn boost(on: bool) {
    if ACTIVE.swap(on, Ordering::SeqCst) == on {
        return;
    }
    unsafe {
        if on {
            SetPriorityClass(GetCurrentProcess(), ABOVE_NORMAL);
            timeBeginPeriod(1);
        } else {
            SetPriorityClass(GetCurrentProcess(), NORMAL);
            timeEndPeriod(1);
        }
    }
    throttling(on);
}

// ---- Eigene Last (CPU und Arbeitsspeicher) für die Statusanzeige ----

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct FileTime {
    lo: u32,
    hi: u32,
}

#[repr(C)]
struct MemCounters {
    cb: u32,
    page_faults: u32,
    peak_ws: usize,
    ws: usize,
    rest: [usize; 6],
}

#[link(name = "kernel32")]
extern "system" {
    fn GetProcessTimes(h: *mut std::ffi::c_void, c: *mut FileTime, e: *mut FileTime, k: *mut FileTime, u: *mut FileTime) -> i32;
    fn K32GetProcessMemoryInfo(h: *mut std::ffi::c_void, c: *mut MemCounters, size: u32) -> i32;
}

struct LoadState {
    at: std::time::Instant,
    cpu_100ns: u64,
    text: String,
}

static LOAD: std::sync::Mutex<Option<LoadState>> = std::sync::Mutex::new(None);

fn ft(f: FileTime) -> u64 {
    ((f.hi as u64) << 32) | f.lo as u64
}

/// Text wie "CPU 4 % • RAM 210 MB" (CPU in Prozent aller Kerne, wird höchstens einmal pro Sekunde neu gemessen).
pub fn load_text() -> String {
    let mut g = match LOAD.lock() {
        Ok(g) => g,
        Err(_) => return String::new(),
    };
    let now = std::time::Instant::now();
    let (mut c, mut e, mut k, mut u) = (FileTime::default(), FileTime::default(), FileTime::default(), FileTime::default());
    let used = unsafe {
        GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u);
        ft(k) + ft(u)
    };
    match g.as_mut() {
        Some(s) if now.duration_since(s.at).as_secs_f64() < 1.0 => return s.text.clone(),
        Some(s) => {
            let dt = now.duration_since(s.at).as_secs_f64();
            let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
            let cpu = (used.saturating_sub(s.cpu_100ns)) as f64 / 1e7 / dt / cores * 100.0;
            let mut m = MemCounters { cb: std::mem::size_of::<MemCounters>() as u32, page_faults: 0, peak_ws: 0, ws: 0, rest: [0; 6] };
            unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut m, m.cb) };
            s.text = format!("CPU {:.0} % • RAM {} MB", cpu, m.ws / (1024 * 1024));
            s.at = now;
            s.cpu_100ns = used;
            s.text.clone()
        }
        None => {
            *g = Some(LoadState { at: now, cpu_100ns: used, text: "CPU … • RAM …".to_string() });
            "CPU … • RAM …".to_string()
        }
    }
}
