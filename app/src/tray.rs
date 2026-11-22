//! Symbol neben der Uhr (Tray): läuft als eigener, kleiner Hintergrundprozess (`overlime.exe --tray`).
//! Klick = Konsole aufwecken, warten bis sie bereit ist und OVERLIME verbunden öffnen.
use std::cell::RefCell;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::sync::atomic::AtomicU32;
use std::time::Duration;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, SetTimer, TranslateMessage, MSG};

static BUSY: AtomicBool = AtomicBool::new(false);
static LAST_MIN: AtomicU32 = AtomicU32::new(u32::MAX);

#[repr(C)]
struct SysTime {
    year: u16,
    month: u16,
    dow: u16,
    day: u16,
    hour: u16,
    min: u16,
    sec: u16,
    ms: u16,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetLocalTime(t: *mut SysTime);
}

/// Weckt alle gekoppelten Konsolen im Ruhemodus (Zeitplan).
fn scheduled_wake() {
    std::thread::spawn(|| {
        let paired = crate::store::load();
        if paired.is_empty() {
            return;
        }
        let list = crate::discovery::discover(None, Duration::from_millis(2000)).unwrap_or_default();
        for c in list {
            if c.state == crate::discovery::State::Ready {
                continue;
            }
            if let Some(p) = paired.iter().find(|p| p.console_id == c.id) {
                if let Some(cred) = p.wake_credential() {
                    let _ = crate::discovery::wakeup(&c.addr, cred, c.is_ps5);
                }
            }
        }
    });
}

/// Prüft einmal pro Minute, ob der Zeitplan zum Aufwecken fällig ist.
fn check_schedule() {
    let mut st = SysTime { year: 0, month: 0, dow: 0, day: 0, hour: 0, min: 0, sec: 0, ms: 0 };
    unsafe { GetLocalTime(&mut st) };
    let key = st.day as u32 * 1440 + st.hour as u32 * 60 + st.min as u32;
    if LAST_MIN.swap(key, Ordering::SeqCst) == key {
        return;
    }
    let s = crate::settings::Settings::load();
    if !s.wake_on || s.wake_hour as u16 != st.hour || s.wake_min as u16 != st.min {
        return;
    }
    // dow: 0 = Sonntag; Bit 0 = Montag
    let bit = if st.dow == 0 { 6 } else { st.dow - 1 };
    if s.wake_days & (1 << bit) != 0 {
        scheduled_wake();
    }
}
static STATUS: Mutex<String> = Mutex::new(String::new());
static CHILD: Mutex<Option<Child>> = Mutex::new(None);

thread_local! {
    static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
}

const IDLE_TIP: &str = "OVERLIME – Klick: Konsole aufwecken und verbinden";

fn set_status(s: &str) {
    if let Ok(mut g) = STATUS.lock() {
        *g = s.to_string();
    }
}

fn main_running() -> bool {
    let mut g = CHILD.lock().unwrap();
    match g.as_mut() {
        Some(c) => matches!(c.try_wait(), Ok(None)),
        None => false,
    }
}

/// Öffnet das Hauptfenster (optional direkt mit Verbinden).
fn open_main(connect: bool) {
    if main_running() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let mut cmd = Command::new(exe);
    if connect {
        cmd.arg("--connect-now");
    }
    if let Ok(child) = cmd.spawn() {
        *CHILD.lock().unwrap() = Some(child);
    }
}

/// Weckt die gekoppelte Konsole, wartet bis sie bereit ist und öffnet OVERLIME verbunden.
fn wake_and_connect() {
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        run_wake_and_connect();
        BUSY.store(false, Ordering::SeqCst);
        set_status("");
    });
}

fn run_wake_and_connect() {
    if main_running() {
        set_status("OVERLIME läuft schon");
        std::thread::sleep(Duration::from_secs(3));
        return;
    }
    // Die App selbst sucht die Konsole, weckt sie bei Bedarf und verbindet.
    set_status("Startet …");
    open_main(true);
    std::thread::sleep(Duration::from_secs(5));
}

unsafe extern "system" fn timer_proc(_: *mut std::ffi::c_void, _: u32, _: usize, _: u32) {
    check_schedule();
    let s = STATUS.lock().map(|g| g.clone()).unwrap_or_default();
    let tip = if s.is_empty() { IDLE_TIP.to_string() } else { format!("OVERLIME – {s}") };
    TRAY.with(|t| {
        if let Some(tray) = t.borrow().as_ref() {
            let _ = tray.set_tooltip(Some(tip));
        }
    });
}

/// Startet den Tray-Prozess. Kehrt nicht zurück.
pub fn run() {
    // nur eine Instanz
    unsafe {
        let name: Vec<u16> = "Local\\OVERLIME_TRAY".encode_utf16().chain(std::iter::once(0)).collect();
        let _h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
    }

    let menu = Menu::new();
    let wake = MenuItem::new("Aufwecken und verbinden", true, None);
    let open = MenuItem::new("OVERLIME öffnen", true, None);
    let quit = MenuItem::new("Beenden", true, None);
    let _ = menu.append_items(&[&wake, &open, &PredefinedMenuItem::separator(), &quit]);
    let (id_wake, id_open, id_quit) = (wake.id().clone(), open.id().clone(), quit.id().clone());

    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if e.id == id_wake {
            wake_and_connect();
        } else if e.id == id_open {
            open_main(false);
        } else if e.id == id_quit {
            std::process::exit(0);
        }
    }));
    TrayIconEvent::set_event_handler(Some(|e: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
            wake_and_connect();
        }
    }));

    let ic = crate::winui::app_icon();
    let icon = tray_icon::Icon::from_rgba(ic.rgba, ic.width, ic.height).ok();
    let mut b = TrayIconBuilder::new().with_menu(Box::new(menu)).with_menu_on_left_click(false).with_tooltip(IDLE_TIP);
    if let Some(i) = icon {
        b = b.with_icon(i);
    }
    match b.build() {
        Ok(t) => TRAY.with(|c| *c.borrow_mut() = Some(t)),
        Err(_) => return,
    }
    unsafe {
        SetTimer(std::ptr::null_mut(), 1, 1500, Some(timer_proc));
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

// ---------------- Autostart mit Windows ----------------

const RUN_KEY: &str = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run";

pub fn autostart_enabled() -> bool {
    Command::new("reg")
        .args(["query", RUN_KEY, "/v", "OVERLIME_TRAY"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn set_autostart(on: bool) -> Result<(), String> {
    let out = if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        Command::new("reg")
            .args(["add", RUN_KEY, "/v", "OVERLIME_TRAY", "/t", "REG_SZ", "/d", &format!("\"{}\" --tray", exe.display()), "/f"])
            .output()
    } else {
        Command::new("reg").args(["delete", RUN_KEY, "/v", "OVERLIME_TRAY", "/f"]).output()
    }
    .map_err(|e| e.to_string())?;
    if out.status.success() || !on {
        Ok(())
    } else {
        Err("Eintrag konnte nicht gesetzt werden".into())
    }
}

/// Startet den Tray-Prozess als eigenen Prozess (falls er noch nicht läuft).
pub fn spawn_tray() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg("--tray").spawn();
    }
}
