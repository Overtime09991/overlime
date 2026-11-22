//! Clips und Screenshots an einen eigenen Discord-Kanal schicken (über die Webhook-Adresse, die du selbst anlegst).
//! Die Datei geht direkt von deinem PC zu Discord, nichts läuft über einen OVERLIME-Server.

use std::path::Path;

pub fn valid_webhook(url: &str) -> bool {
    let u = url.trim();
    (u.starts_with("https://discord.com/api/webhooks/") || u.starts_with("https://discordapp.com/api/webhooks/") || u.starts_with("https://canary.discord.com/api/webhooks/") || u.starts_with("https://ptb.discord.com/api/webhooks/"))
        && u.len() > 50
        && !u.contains(char::is_whitespace)
}

fn mime_for(name: &str) -> &'static str {
    let l = name.to_lowercase();
    if l.ends_with(".mp4") {
        "video/mp4"
    } else if l.ends_with(".png") {
        "image/png"
    } else if l.ends_with(".gif") {
        "image/gif"
    } else {
        "application/octet-stream"
    }
}

/// Baut die Daten für das Hochladen (multipart/form-data) zusammen.
pub fn build_body(boundary: &str, file_name: &str, data: &[u8], message: &str) -> Vec<u8> {
    let payload = serde_json::json!({ "content": message, "username": "OVERLIME", "allowed_mentions": { "parse": [] } }).to_string();
    let mut b: Vec<u8> = Vec::with_capacity(data.len() + 1024);
    b.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"payload_json\"\r\nContent-Type: application/json\r\n\r\n{payload}\r\n").as_bytes());
    let safe: String = file_name.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect();
    b.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"files[0]\"; filename=\"{safe}\"\r\nContent-Type: {}\r\n\r\n", mime_for(&safe)).as_bytes());
    b.extend_from_slice(data);
    b.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    b
}

pub fn send_file(webhook: &str, path: &Path, message: &str) -> Result<(), String> {
    let test = std::env::var_os("OL_WEBHOOK_TEST").is_some();
    if !test && !valid_webhook(webhook) {
        return Err("Die Webhook-Adresse ist ungültig.".into());
    }
    let data = std::fs::read(path).map_err(|e| format!("Datei nicht lesbar: {e}"))?;
    if data.len() > 100 * 1024 * 1024 {
        return Err("Die Datei ist zu groß.".into());
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "datei".into());
    let boundary = format!("----overlime{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let body = build_body(&boundary, &name, &data, message);
    let resp = ureq::post(webhook.trim())
        .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
        .timeout(std::time::Duration::from_secs(180))
        .send_bytes(&body);
    match resp {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(413, _)) => Err(format!("Die Datei ist zu groß für diesen Discord-Server ({:.1} MB).", data.len() as f64 / 1_048_576.0)),
        Err(ureq::Error::Status(404, _)) => Err("Die Webhook-Adresse gibt es nicht mehr.".into()),
        Err(ureq::Error::Status(429, _)) => Err("Discord bremst gerade, bitte gleich noch einmal versuchen.".into()),
        Err(ureq::Error::Status(c, _)) => Err(format!("Discord hat die Datei abgelehnt (Fehler {c}).")),
        Err(e) => Err(format!("Keine Verbindung zu Discord: {e}")),
    }
}
