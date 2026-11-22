//! Automatische Updates über GitHub Releases.
//! Erwartet im neuesten Release die Dateien `overlime.exe`, `overlime.exe.sha256` und `overlime.exe.sig`.
//! Installiert wird nur, wenn die Signatur (Ed25519) zum eingebauten öffentlichen Schlüssel passt.
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// "besitzer/repository" auf GitHub. Leer = Updates aus.
pub const UPDATE_REPO: &str = "Overtime09991/overlime";

/// Öffentlicher Schlüssel des Entwicklers. Nur Updates, die damit geprüft werden können, werden installiert.
const UPDATE_PUBKEY: &str = "f29c68e6866db362f59acbe00bbeecb944127e9b344661ea7f1da2a38dbbf6eb";

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()).collect()
}

/// Prüft die Signatur über die heruntergeladene Programmdatei.
pub fn verify_signature(data: &[u8], sig_hex: &str) -> Result<(), String> {
    let key: [u8; 32] = unhex(UPDATE_PUBKEY).and_then(|v| v.try_into().ok()).ok_or("Interner Schlüsselfehler")?;
    let key = VerifyingKey::from_bytes(&key).map_err(|_| "Interner Schlüsselfehler".to_string())?;
    let sig: [u8; 64] = unhex(sig_hex).and_then(|v| v.try_into().ok()).ok_or("Ungültige Signatur")?;
    key.verify(data, &Signature::from_bytes(&sig)).map_err(|_| "Signatur stimmt nicht – Update abgebrochen".to_string())
}

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub sha_url: String,
    pub sig_url: String,
    pub notes: String,
}

fn parse_ver(v: &str) -> Vec<u32> {
    v.trim().trim_start_matches(['v', 'V']).split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

pub fn is_newer(remote: &str, local: &str) -> bool {
    let (a, b) = (parse_ver(remote), parse_ver(local));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

pub fn enabled() -> bool {
    !UPDATE_REPO.is_empty()
}

/// Räumt die alte exe vom letzten Update weg.
pub fn cleanup_old() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
        if let (Some(dir), Some(stem)) = (exe.parent(), exe.file_stem().map(|s| s.to_string_lossy().to_string())) {
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().to_string();
                    if n.starts_with(&format!("{stem}.old")) {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
        }
    }
}

/// Prüft, ob es eine neuere Version gibt. Blockierend.
pub fn check(current: &str) -> Result<Option<Release>, String> {
    if !enabled() {
        return Ok(None);
    }
    let url = format!("https://api.github.com/repos/{UPDATE_REPO}/releases/latest");
    let v: serde_json::Value = ureq::get(&url)
        .set("User-Agent", "overlime-updater")
        .set("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    let tag = v["tag_name"].as_str().unwrap_or("");
    if !is_newer(tag, current) {
        return Ok(None);
    }
    let mut exe = None;
    let mut sha = None;
    let mut sig = None;
    if let Some(assets) = v["assets"].as_array() {
        for a in assets {
            let name = a["name"].as_str().unwrap_or("");
            let dl = a["browser_download_url"].as_str().unwrap_or("").to_string();
            if name == "overlime.exe" {
                exe = Some(dl);
            } else if name == "overlime.exe.sha256" {
                sha = Some(dl);
            } else if name == "overlime.exe.sig" {
                sig = Some(dl);
            }
        }
    }
    match (exe, sha, sig) {
        (Some(url), Some(sha_url), Some(sig_url)) => Ok(Some(Release {
            version: tag.trim_start_matches(['v', 'V']).to_string(),
            url,
            sha_url,
            sig_url,
            notes: v["body"].as_str().unwrap_or("").chars().take(3000).collect(),
        })),
        _ => Err("Release ohne Programmdatei, Prüfsumme oder Signatur".into()),
    }
}

fn download(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url)
        .set("User-Agent", "overlime-updater")
        .timeout(std::time::Duration::from_secs(300))
        .call()
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    resp.into_reader().take(limit).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}


/// Beendet andere laufende OVERLIME-Prozesse (z. B. das Symbol neben der Uhr), damit die Datei ersetzt werden kann.
fn stop_other_instances() {
    use std::os::windows::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else { return };
    let Some(name) = exe.file_name().map(|n| n.to_string_lossy().to_string()) else { return };
    let me = std::process::id();
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/FI", &format!("PID ne {me}"), "/IM", &name])
        .creation_flags(0x0800_0000)
        .output();
    std::thread::sleep(Duration::from_millis(600));
}

/// Ersetzt die laufende exe durch `data`. Die alte Datei bleibt als .old liegen (wird beim nächsten Start gelöscht).
fn swap_in(data: &[u8]) -> Result<std::path::PathBuf, String> {
    stop_other_instances();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut old = exe.with_extension("old");
    let _ = std::fs::remove_file(&old);
    if old.exists() {
        // alte Datei noch gesperrt: anderen Namen nehmen
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        old = exe.with_extension(format!("old{secs}"));
    }
    std::fs::rename(&exe, &old).map_err(|e| format!("Ersetzen nicht möglich: {e}"))?;
    if let Err(e) = std::fs::write(&exe, data) {
        let _ = std::fs::rename(&old, &exe);
        return Err(format!("Schreiben fehlgeschlagen: {e}"));
    }
    Ok(exe)
}

/// Lädt das Update, prüft die Prüfsumme, ersetzt die laufende exe und startet neu.
pub fn install(rel: &Release) -> Result<(), String> {
    let sha_txt = String::from_utf8(download(&rel.sha_url, 4096)?).map_err(|e| e.to_string())?;
    let expected = sha_txt.split_whitespace().next().unwrap_or("").to_lowercase();
    if expected.len() != 64 {
        return Err("Ungültige Prüfsumme".into());
    }
    let data = download(&rel.url, 250 * 1024 * 1024)?;
    let got: String = Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
    if got != expected {
        return Err("Prüfsumme stimmt nicht – Update abgebrochen".into());
    }
    if data.len() < 1_000_000 || &data[..2] != b"MZ" {
        return Err("Heruntergeladene Datei ist keine gültige Programmdatei".into());
    }
    let sig_txt = String::from_utf8(download(&rel.sig_url, 4096)?).map_err(|e| e.to_string())?;
    verify_signature(&data, &sig_txt)?;
    let exe = swap_in(&data)?;
    std::process::Command::new(&exe).spawn().map_err(|e| e.to_string())?;
    std::process::exit(0);
}

/// Fortschritt des Updates für die Animation.
/// Stufen: 0 Verbinden, 1 Herunterladen, 2 Prüfen, 3 Installieren, 4 Neustart
#[derive(Clone, Default)]
pub struct Progress {
    pub stage: u8,
    pub pct: f32,
    pub error: Option<String>,
}

fn set(p: &Arc<Mutex<Progress>>, stage: u8, pct: f32) {
    if let Ok(mut g) = p.lock() {
        g.stage = stage;
        g.pct = pct;
    }
}

/// Jede Stufe zeigt sich mindestens so lange, damit die Animation lesbar bleibt.
fn hold(start: Instant, min: Duration) {
    let el = start.elapsed();
    if el < min {
        std::thread::sleep(min - el);
    }
}

fn download_progress(url: &str, limit: u64, p: &Arc<Mutex<Progress>>) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url)
        .set("User-Agent", "overlime-updater")
        .timeout(Duration::from_secs(600))
        .call()
        .map_err(|e| e.to_string())?;
    let total: u64 = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut reader = resp.into_reader().take(limit);
    let mut buf = Vec::with_capacity(total as usize);
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if total > 0 {
            set(p, 1, (buf.len() as f32 / total as f32 * 100.0).min(100.0));
        }
    }
    set(p, 1, 100.0);
    Ok(buf)
}

/// Wie `install`, meldet aber den Fortschritt und lässt jede Stufe kurz sichtbar.
/// Dauer insgesamt etwa 15 Sekunden (die Willkommens-Animation folgt nach dem Neustart).
pub fn install_ui(rel: &Release, p: Arc<Mutex<Progress>>) -> Result<(), String> {
    let r = (|| -> Result<(), String> {
        // 0: Verbinden / Prüfsumme holen
        let t0 = Instant::now();
        set(&p, 0, 0.0);
        let sha_txt = String::from_utf8(download(&rel.sha_url, 4096)?).map_err(|e| e.to_string())?;
        let expected = sha_txt.split_whitespace().next().unwrap_or("").to_lowercase();
        if expected.len() != 64 {
            return Err("Ungültige Prüfsumme".into());
        }
        hold(t0, Duration::from_millis(2200));

        // 1: Herunterladen
        let t1 = Instant::now();
        set(&p, 1, 0.0);
        let data = download_progress(&rel.url, 250 * 1024 * 1024, &p)?;
        hold(t1, Duration::from_millis(5000));

        // 2: Prüfen
        let t2 = Instant::now();
        set(&p, 2, 0.0);
        let got: String = Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
        if got != expected {
            return Err("Prüfsumme stimmt nicht – Update abgebrochen".into());
        }
        if data.len() < 1_000_000 || &data[..2] != b"MZ" {
            return Err("Heruntergeladene Datei ist keine gültige Programmdatei".into());
        }
        let sig_txt = String::from_utf8(download(&rel.sig_url, 4096)?).map_err(|e| e.to_string())?;
        verify_signature(&data, &sig_txt)?;
        hold(t2, Duration::from_millis(3000));

        // 3: Installieren
        let t3 = Instant::now();
        set(&p, 3, 0.0);
        let exe = swap_in(&data)?;
        hold(t3, Duration::from_millis(3000));

        // 4: Neustart
        let t4 = Instant::now();
        set(&p, 4, 0.0);
        hold(t4, Duration::from_millis(2000));
        std::process::Command::new(&exe).spawn().map_err(|e| e.to_string())?;
        std::process::exit(0);
    })();
    if let Err(e) = &r {
        if let Ok(mut g) = p.lock() {
            g.error = Some(e.clone());
        }
    }
    r
}

/// Sammelt die Stichpunkte unter einer Überschrift, die `key` enthält ("neu" für Deutsch, "new in" für Englisch).
fn collect(notes: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in notes.lines() {
        let t = line.trim();
        if t.starts_with("###") {
            inside = t.to_lowercase().contains(key);
            continue;
        }
        if inside {
            if let Some(rest) = t.strip_prefix("- ") {
                out.push(rest.replace("**", "").replace('`', ""));
            }
        }
    }
    out
}

/// Stichpunkte in der eingestellten Sprache (Englisch: Abschnitt "New in ...", sonst Deutsch "Neu in ...").
pub fn highlights_all(notes: &str) -> Vec<String> {
    if crate::lang::is_english() {
        let en = collect(notes, "new in");
        if !en.is_empty() {
            return en;
        }
    }
    collect(notes, "neu")
}

/// Zieht die Stichpunkte unter "Neu in ..." aus den Release-Notizen (ohne Markdown-Zeichen).
pub fn highlights(notes: &str) -> Vec<String> {
    let mut out = highlights_all(notes);
    out.truncate(6);
    out
}

/// Eine Version mit ihren Neuerungen (aus den GitHub-Release-Notizen).
#[derive(Clone, Debug)]
pub struct NewsItem {
    pub version: String,
    pub date: String,
    pub items: Vec<String>,
}

/// Lädt die letzten Releases mit ihren Neuerungen. Blockierend.
pub fn fetch_news() -> Result<Vec<NewsItem>, String> {
    if !enabled() {
        return Ok(Vec::new());
    }
    let url = format!("https://api.github.com/repos/{UPDATE_REPO}/releases?per_page=15");
    let v: serde_json::Value = ureq::get(&url)
        .set("User-Agent", "overlime-news")
        .set("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if let Some(arr) = v.as_array() {
        for r in arr {
            if r["prerelease"].as_bool().unwrap_or(false) || r["draft"].as_bool().unwrap_or(false) {
                continue;
            }
            let tag = r["tag_name"].as_str().unwrap_or("").trim_start_matches(['v', 'V']).to_string();
            let items = highlights_all(r["body"].as_str().unwrap_or(""));
            if tag.is_empty() || items.is_empty() {
                continue;
            }
            let date = r["published_at"].as_str().unwrap_or("").chars().take(10).collect();
            out.push(NewsItem { version: tag, date, items });
        }
    }
    Ok(out)
}
