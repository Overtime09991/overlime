//! PSN-Anmeldung (einmalig), um die Account-ID für die Kopplung zu bekommen.
//! Der Nutzer meldet sich selbst auf Sonys Seite an; die App sieht nie ein Passwort.
use base64::{engine::general_purpose::STANDARD, Engine};

const CLIENT_ID: &str = "ba495a24-818c-472b-b12d-ff231c1b5745";
const CLIENT_SECRET: &str = "mvaiZkRsAsI1IBkY";
const TOKEN_URL: &str = "https://auth.api.sonyentertainmentnetwork.com/2.0/oauth/token";
const SCOPE: &str = "psn:clientapp referenceDataService:countryConfig.read pushNotification:webSocket.desktop.connect sessionManager:remotePlaySession.system.update";
const REDIRECT: &str = "https://remoteplay.dl.playstation.net/remoteplay/redirect";

pub fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Anmelde-Schlüssel für „Von unterwegs spielen“.
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    pub expires_in: u64,
}

fn tokens_from(resp: &serde_json::Value) -> Option<Tokens> {
    Some(Tokens {
        access: resp["access_token"].as_str()?.to_string(),
        refresh: resp["refresh_token"].as_str().unwrap_or("").to_string(),
        expires_in: resp["expires_in"].as_u64().unwrap_or(3000),
    })
}

/// Holt mit dem Erneuerungs-Schlüssel einen neuen Zugriffs-Schlüssel. Blockierend.
pub fn refresh(refresh_token: &str) -> Result<Tokens, String> {
    let body = format!("grant_type=refresh_token&refresh_token={}&scope={}&redirect_uri={}&", enc(refresh_token), enc(SCOPE), enc(REDIRECT));
    let resp: serde_json::Value = ureq::post(TOKEN_URL)
        .set("Authorization", &basic())
        .set("Content-Type", "application/x-www-form-urlencoded")
        .timeout(std::time::Duration::from_secs(20))
        .send_string(&body)
        .map_err(|e| match e {
            ureq::Error::Status(..) => "Die PSN-Anmeldung ist abgelaufen. Bitte unter „Von unterwegs“ neu anmelden.".to_string(),
            other => format!("PSN nicht erreichbar: {other}"),
        })?
        .into_json()
        .map_err(|e| e.to_string())?;
    tokens_from(&resp).ok_or_else(|| "PSN hat keinen Zugriffs-Schlüssel geschickt".to_string())
}

pub fn login_url() -> String {
    // Mit Geräte-Kennung, damit dieselbe Anmeldung auch für „Von unterwegs“ gilt
    let duid = crate::ffi::psn_duid().map(|d| format!("duid={d}&")).unwrap_or_default();
    base_login_url() + &duid
}

fn base_login_url() -> String {
    format!(
        "https://auth.api.sonyentertainmentnetwork.com/2.0/oauth/authorize?service_entity=urn:service-entity:psn&response_type=code&client_id={}&redirect_uri={}&scope={}&request_locale=de_DE&ui=pr&service_logo=ps&layout_type=popup&smcid=remoteplay&prompt=always&PlatformPrivacyWs1=minimal&",
        CLIENT_ID,
        enc(REDIRECT),
        enc(SCOPE)
    )
}

/// Zieht den `code=` Wert aus der eingefügten Adresse (oder nimmt den Text direkt).
pub fn parse_code(pasted: &str) -> Option<String> {
    let t = pasted.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(i) = t.find("code=") {
        let rest = &t[i + 5..];
        let end = rest.find(['&', '#', ' ']).unwrap_or(rest.len());
        let c = &rest[..end];
        return if c.is_empty() { None } else { Some(c.to_string()) };
    }
    if t.len() < 20 && !t.contains('/') {
        Some(t.to_string())
    } else {
        None
    }
}

fn basic() -> String {
    format!("Basic {}", STANDARD.encode(format!("{CLIENT_ID}:{CLIENT_SECRET}")))
}

/// Tauscht den Code gegen die Account-ID (8 Byte, little endian). Blockierend.
pub fn account_id_from_code(code: &str) -> Result<([u8; 8], String, Option<Tokens>), String> {
    let body = format!(
        "grant_type=authorization_code&code={}&scope={}&redirect_uri={}&",
        enc(code),
        enc(SCOPE),
        enc(REDIRECT)
    );
    let resp: serde_json::Value = ureq::post(TOKEN_URL)
        .set("Authorization", &basic())
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&body)
        .map_err(|e| format!("Anmeldung fehlgeschlagen (Code abgelaufen?): {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    let token = resp["access_token"].as_str().ok_or("Kein Zugriffstoken erhalten")?.to_string();

    let info: serde_json::Value = ureq::get(&format!("{TOKEN_URL}/{token}"))
        .set("Authorization", &basic())
        .call()
        .map_err(|e| format!("Konto-Abfrage fehlgeschlagen: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    let id: i64 = info["user_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or("Keine Konto-ID erhalten")?;
    let name = info["online_id"].as_str().or_else(|| info["username"].as_str()).unwrap_or("").to_string();
    Ok((id.to_le_bytes(), name, tokens_from(&resp).filter(|t| !t.refresh.is_empty())))
}

pub fn id_to_b64(id: [u8; 8]) -> String {
    STANDARD.encode(id)
}

pub fn id_from_b64(s: &str) -> Option<[u8; 8]> {
    let v = STANDARD.decode(s.trim()).ok()?;
    v.try_into().ok()
}
