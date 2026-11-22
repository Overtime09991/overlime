//! Gekoppelte Konsolen werden verschlüsselt (Windows-Benutzerkonto, DPAPI) in %APPDATA%\overlime\paired.dat gespeichert.
//! Eine alte, unverschlüsselte paired.txt wird beim ersten Start übernommen und gelöscht.
use crate::ffi::Paired;
use std::path::PathBuf;

fn path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var("APPDATA").ok()?).join("overlime").join("paired.txt"))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex16(s: &str) -> Option<[u8; 16]> {
    if s.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn dat_path() -> Option<PathBuf> {
    path().map(|p| p.with_file_name("paired.dat"))
}

#[repr(C)]
struct Blob {
    cb: u32,
    pb: *mut u8,
}

#[cfg(windows)]
#[link(name = "crypt32")]
extern "system" {
    fn CryptProtectData(pin: *const Blob, desc: *const u16, ent: *const Blob, res: *mut std::ffi::c_void, prompt: *mut std::ffi::c_void, flags: u32, pout: *mut Blob) -> i32;
    fn CryptUnprotectData(pin: *const Blob, desc: *mut *mut u16, ent: *const Blob, res: *mut std::ffi::c_void, prompt: *mut std::ffi::c_void, flags: u32, pout: *mut Blob) -> i32;
}
#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(h: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
}

pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
    #[cfg(windows)]
    unsafe {
        let inp = Blob { cb: data.len() as u32, pb: data.as_ptr() as *mut u8 };
        let mut out = Blob { cb: 0, pb: std::ptr::null_mut() };
        if CryptProtectData(&inp, std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut out) == 0 {
            return None;
        }
        let v = std::slice::from_raw_parts(out.pb, out.cb as usize).to_vec();
        LocalFree(out.pb as *mut std::ffi::c_void);
        return Some(v);
    }
    #[allow(unreachable_code)]
    None
}

pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    #[cfg(windows)]
    unsafe {
        let inp = Blob { cb: data.len() as u32, pb: data.as_ptr() as *mut u8 };
        let mut out = Blob { cb: 0, pb: std::ptr::null_mut() };
        if CryptUnprotectData(&inp, std::ptr::null_mut(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut out) == 0 {
            return None;
        }
        let v = std::slice::from_raw_parts(out.pb, out.cb as usize).to_vec();
        LocalFree(out.pb as *mut std::ffi::c_void);
        return Some(v);
    }
    #[allow(unreachable_code)]
    None
}

fn parse(text: &str) -> Vec<Paired> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() != 5 {
                return None;
            }
            Some(Paired {
                console_id: f[0].to_string(),
                ps5: f[1] == "1",
                nickname: f[2].to_string(),
                rp_regist_key: unhex16(f[3])?,
                rp_key: unhex16(f[4])?,
            })
        })
        .collect()
}

pub fn load() -> Vec<Paired> {
    if let Some(enc) = dat_path().and_then(|p| std::fs::read(p).ok()) {
        if let Some(plain) = unprotect(&enc) {
            return parse(&String::from_utf8_lossy(&plain));
        }
        return vec![];
    }
    // Übernahme der alten, unverschlüsselten Datei
    let Some(text) = path().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return vec![];
    };
    let list = parse(&text);
    if !list.is_empty() {
        save(&list);
    }
    list
}

pub fn save(list: &[Paired]) {
    let Some(p) = path() else { return };
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let text: String = list
        .iter()
        .map(|c| {
            format!(
                "{}\t{}\t{}\t{}\t{}\n",
                c.console_id,
                if c.ps5 { "1" } else { "0" },
                c.nickname.replace('\t', " "),
                hex(&c.rp_regist_key),
                hex(&c.rp_key)
            )
        })
        .collect();
    match (protect(text.as_bytes()), dat_path()) {
        (Some(enc), Some(dp)) => {
            if std::fs::write(dp, enc).is_ok() {
                let _ = std::fs::remove_file(p);
            }
        }
        _ => {
            let _ = std::fs::write(p, text);
        }
    }
}
