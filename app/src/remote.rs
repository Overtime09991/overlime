//! „Von unterwegs spielen“: Verbindung zur eigenen Konsole über PSN, ohne VPN und ohne Router-Einstellungen.
//! Die PSN-Anmeldung (Zugriffs- und Erneuerungs-Schlüssel) liegt nur auf diesem PC, verschlüsselt mit dem Windows-Benutzerkonto.

use crate::{ffi, psn, store};
use std::path::PathBuf;
use std::sync::Mutex;

/// Kennzeichen im Adressfeld einer Konsole, die über PSN verbunden wird.
pub const PREFIX: &str = "psn:";

#[derive(Clone, Default)]
pub struct Login {
    pub access: String,
    pub refresh: String,
    /// Ablaufzeit des Zugriffs-Schlüssels in Sekunden seit 1970
    pub expires: u64,
    pub account_b64: String,
}

#[derive(Clone)]
pub struct Device {
    pub name: String,
    pub ps5: bool,
    pub uid: [u8; 32],
}

// Verhindert, dass zwei Stellen gleichzeitig erneuern (der alte Erneuerungs-Schlüssel gilt dann nicht mehr).
static LOCK: Mutex<()> = Mutex::new(());

fn path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var("APPDATA").ok()?).join("overlime").join("psn.dat"))
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn encode(l: &Login) -> String {
    serde_json::json!({ "access": l.access, "refresh": l.refresh, "expires": l.expires, "account": l.account_b64 }).to_string()
}

fn decode(text: &str) -> Option<Login> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    Some(Login {
        access: v["access"].as_str()?.to_string(),
        refresh: v["refresh"].as_str()?.to_string(),
        expires: v["expires"].as_u64().unwrap_or(0),
        account_b64: v["account"].as_str()?.to_string(),
    })
}

pub fn save(tokens: &psn::Tokens, account_b64: &str) {
    let l = Login { access: tokens.access.clone(), refresh: tokens.refresh.clone(), expires: now() + tokens.expires_in, account_b64: account_b64.to_string() };
    store_login(&l);
}

fn store_login(l: &Login) {
    let (Some(p), Some(enc)) = (path(), store::protect(encode(l).as_bytes())) else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(p, enc);
}

pub fn load() -> Option<Login> {
    let raw = std::fs::read(path()?).ok()?;
    let plain = store::unprotect(&raw)?;
    decode(std::str::from_utf8(&plain).ok()?)
}

pub fn logged_in() -> bool {
    path().map(|p| p.exists()).unwrap_or(false)
}

pub fn logout() {
    if let Some(p) = path() {
        let _ = std::fs::remove_file(p);
    }
}

/// Gültige Anmeldung, bei Bedarf vorher erneuert. Blockierend.
pub fn fresh_login() -> Result<Login, String> {
    let _guard = LOCK.lock().map_err(|_| "Interner Fehler bei der PSN-Anmeldung".to_string())?;
    let mut l = load().ok_or("Bitte zuerst unter „Von unterwegs“ bei PSN anmelden.")?;
    if l.expires > now() + 300 {
        return Ok(l);
    }
    let t = psn::refresh(&l.refresh)?;
    l.access = t.access;
    if !t.refresh.is_empty() {
        l.refresh = t.refresh;
    }
    l.expires = now() + t.expires_in;
    store_login(&l);
    Ok(l)
}

/// PS5-Konsolen aus dem Konto, dazu der feste Eintrag für die Haupt-PS4. Blockierend.
pub fn devices() -> Result<Vec<Device>, String> {
    let l = fresh_login()?;
    let mut list = ffi::psn_list(&l.access)?;
    // PSN nennt PS4-Konsolen nicht einzeln: es wird immer die als „Haupt-PS4“ eingetragene verbunden.
    list.push(Device { name: "Haupt-PS4".to_string(), ps5: false, uid: [b'A'; 32] });
    Ok(list)
}

/// Alles, was für den Verbindungsaufbau gebraucht wird. `addr` ist `psn:<Geräte-Kennung>`.
pub fn link_for(addr: &str) -> Result<ffi::PsnLink, String> {
    let uid = parse_uid(addr.strip_prefix(PREFIX).unwrap_or("")).ok_or("Unbekannte Konsole")?;
    let l = fresh_login()?;
    let account_id = psn::id_from_b64(&l.account_b64).ok_or("Konto-ID fehlt, bitte neu anmelden.")?;
    Ok(ffi::PsnLink { token: l.access, uid, account_id })
}

pub fn address(uid: &[u8; 32]) -> String {
    format!("{PREFIX}{}", uid.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

fn parse_uid(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// Selbsttest ohne PSN: Adresse und verschlüsseltes Speichern.
pub fn selftest() -> bool {
    let uid: [u8; 32] = std::array::from_fn(|i| (i * 7) as u8);
    let a = address(&uid);
    let ok_addr = parse_uid(&a[PREFIX.len()..]) == Some(uid) && parse_uid("xyz").is_none();
    let t = psn::Tokens { access: "test-access".into(), refresh: "test-refresh".into(), expires_in: 3600 };
    save(&t, &psn::id_to_b64([1, 2, 3, 4, 5, 6, 7, 8]));
    let ok_store = load().map(|l| l.access == "test-access" && l.refresh == "test-refresh").unwrap_or(false);
    let raw = path().and_then(|p| std::fs::read(p).ok()).unwrap_or_default();
    let ok_encrypted = !raw.is_empty() && !raw.windows(11).any(|w| w == b"test-access");
    let ok_link = link_for(&a).map(|l| l.uid == uid && l.account_id == [1, 2, 3, 4, 5, 6, 7, 8]).unwrap_or(false);
    logout();
    let ok_logout = !logged_in();
    println!("Adresse {ok_addr}, Speichern {ok_store}, verschlüsselt {ok_encrypted}, Verbindungsdaten {ok_link}, Abmelden {ok_logout}");
    let mut duid_ok = false;
    if let Some(d) = ffi::psn_duid() {
        duid_ok = d.len() == 48 && d.starts_with("0000000700410080");
    }
    println!("Geräte-Kennung {duid_ok}");
    ok_addr && ok_store && ok_encrypted && ok_link && ok_logout && duid_ok
}
