//! Discord-Status (Rich Presence) über die lokale Discord-App. Läuft in einem eigenen Thread.
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// Standard-Anwendungs-ID von OVERLIME (aus dem Discord-Entwicklerportal). Leer = nur per Einstellung nutzbar.
pub const DEFAULT_APP_ID: &str = "1571990471327096842";

pub const ST_OFF: u8 = 0;
pub const ST_TRYING: u8 = 1;
pub const ST_CONNECTED: u8 = 2;
pub const ST_NO_ID: u8 = 3;

#[derive(Clone, PartialEq, Debug)]
pub struct Presence {
    pub app_id: String,
    pub details: String,
    pub state: String,
    pub start: Option<i64>,
    pub buttons: bool,
    pub large_text: String,
}

enum Cmd {
    Set(Presence),
    Clear,
}

pub struct Handle {
    tx: mpsc::Sender<Cmd>,
    pub status: Arc<AtomicU8>,
    last: Option<Presence>,
}

impl Handle {
    /// Setzt den gewünschten Status (None = ausblenden). Sendet nur bei Änderungen.
    pub fn update(&mut self, want: Option<Presence>) {
        if want == self.last {
            return;
        }
        match &want {
            Some(p) if p.app_id.trim().is_empty() => {
                self.status.store(ST_NO_ID, Ordering::Relaxed);
                let _ = self.tx.send(Cmd::Clear);
            }
            Some(p) => {
                let _ = self.tx.send(Cmd::Set(p.clone()));
            }
            None => {
                self.status.store(ST_OFF, Ordering::Relaxed);
                let _ = self.tx.send(Cmd::Clear);
            }
        }
        self.last = want;
    }

    pub fn state(&self) -> u8 {
        self.status.load(Ordering::Relaxed)
    }
}

pub fn spawn() -> Handle {
    let (tx, rx) = mpsc::channel::<Cmd>();
    let status = Arc::new(AtomicU8::new(ST_OFF));
    let st = status.clone();
    std::thread::spawn(move || {
        let mut client: Option<DiscordIpcClient> = None;
        let mut client_id = String::new();
        let mut desired: Option<Presence> = None;
        let mut sent: Option<Presence> = None;
        let mut last_try = Instant::now() - Duration::from_secs(60);
        loop {
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(Cmd::Set(p)) => desired = Some(p),
                Ok(Cmd::Clear) => desired = None,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            match &desired {
                None => {
                    if let Some(mut c) = client.take() {
                        let _ = c.clear_activity();
                        let _ = c.close();
                    }
                    sent = None;
                    st.store(ST_OFF, Ordering::Relaxed);
                }
                Some(p) => {
                    // ID gewechselt -> neu verbinden
                    if client.is_some() && client_id != p.app_id {
                        if let Some(mut c) = client.take() {
                            let _ = c.close();
                        }
                        sent = None;
                    }
                    if client.is_none() {
                        st.store(ST_TRYING, Ordering::Relaxed);
                        if last_try.elapsed() >= Duration::from_secs(10) {
                            last_try = Instant::now();
                            let mut c = DiscordIpcClient::new(&p.app_id);
                            if c.connect().is_ok() {
                                client = Some(c);
                                client_id = p.app_id.clone();
                                sent = None;
                            }
                        }
                    }
                    if let Some(c) = client.as_mut() {
                        if sent.as_ref() != Some(p) {
                            let mut act = activity::Activity::new()
                                .details(&p.details)
                                .state(&p.state)
                                .assets(activity::Assets::new().large_image("overlime").large_text(&p.large_text));
                            if p.buttons {
                                act = act.buttons(vec![
                                    activity::Button::new("OVERLIME herunterladen", "https://github.com/Overtime09991/overlime/releases/latest"),
                                    activity::Button::new("Quellcode ansehen", "https://github.com/Overtime09991/overlime"),
                                ]);
                            }
                            if let Some(s) = p.start {
                                act = act.timestamps(activity::Timestamps::new().start(s));
                            }
                            if c.set_activity(act).is_ok() {
                                sent = Some(p.clone());
                                st.store(ST_CONNECTED, Ordering::Relaxed);
                            } else {
                                // Verbindung weg (Discord beendet) -> später neu versuchen
                                client = None;
                                sent = None;
                                st.store(ST_TRYING, Ordering::Relaxed);
                            }
                        } else {
                            st.store(ST_CONNECTED, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
    });
    Handle { tx, status, last: None }
}
