//! Einstellungen, verschlüsselt (Windows-Benutzerkonto) in %APPDATA%\overlime\settings.dat (key=value im Inneren).
use std::path::PathBuf;

/// Eigene Qualität für eine einzelne Konsole.
#[derive(Clone, PartialEq, Debug)]
pub struct Profile {
    pub resolution: usize,
    pub fps: usize,
    pub bitrate: f32,
    pub codec: usize,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
    pub language: usize, // 0 Deutsch, 1 English
    pub interp: bool,
    pub hud_items: u32,
    pub hud_pos: usize,  // 0 oben links, 1 oben rechts, 2 unten links, 3 unten rechts
    pub hud_size: usize, // 0 klein, 1 normal, 2 groß
    pub hud_box: bool,
    pub tour_done: bool,
    pub intro: bool,
    pub notify_ready: bool,
    pub eq_bass: f32,
    pub eq_mid: f32,
    pub eq_treble: f32,
    pub wake_on: bool,
    pub wake_hour: u8,
    pub wake_min: u8,
    pub wake_days: u8, // Bit 0 = Montag ... Bit 6 = Sonntag
    pub profiles: Vec<(String, Profile)>,
    pub resolution: usize, // 0=540p 1=720p 2=1080p
    pub fps: usize,        // 0=30 1=60
    pub bitrate: f32,      // Mbit/s
    pub codec: usize,      // 0=Automatisch 1=H.264 2=HEVC
    pub upscale: usize,    // 0=Aus 1=Bilinear 2=Scharf 3=Ultra (4K)
    pub fullscreen: bool,
    pub rumble: bool,
    pub psn_account: String,
    pub psn_name: String,
    pub auto_update: bool,
    pub manual_ip: String,
    pub keys: Vec<String>,
    pub last_version: String,
    pub theme: String,
    pub discord: bool,
    pub discord_game: bool,
    pub discord_id: String,
    pub autoconnect: bool,
    pub ask_feedback: bool,
    pub volume: f32,
    pub kb_mode: usize,
    pub ds_features: bool,
    pub sharp: f32,
    pub ui_scale: usize,
    pub news_seen: String,
    pub audio_device: String,
    pub welcome_anim: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            profiles: vec![],
            language: if crate::winui::system_is_german() { 0 } else { 1 },
            interp: false,
            hud_items: 0x0FF,
            hud_pos: 0,
            hud_size: 1,
            hud_box: false,
            tour_done: false,
            intro: true,
            notify_ready: true,
            eq_bass: 0.0,
            eq_mid: 0.0,
            eq_treble: 0.0,
            wake_on: false,
            wake_hour: 19,
            wake_min: 0,
            wake_days: 127,
            resolution: 2,
            fps: 1,
            bitrate: 30.0,
            codec: 0,
            upscale: 2,
            fullscreen: false,
            rumble: true,
            psn_account: String::new(),
            psn_name: String::new(),
            auto_update: true,
            manual_ip: String::new(),
            keys: crate::keys::defaults(),
            last_version: String::new(),
            theme: "Ozean".to_string(),
            discord: false,
            discord_game: true,
            discord_id: String::new(),
            autoconnect: false,
            ask_feedback: true,
            volume: 1.0,
            kb_mode: 0,
            ds_features: true,
            sharp: 0.55,
            ui_scale: 0,
            news_seen: String::new(),
            audio_device: String::new(),
            welcome_anim: true,
        }
    }
}

impl Settings {
    /// Höchste Qualität, die Sony im Remote Play erlaubt.
    pub fn max_quality(&mut self) {
        self.resolution = 2;
        self.fps = 1;
        self.bitrate = 50.0;
        self.codec = 2;
        self.upscale = 3;
    }

    fn path() -> Option<PathBuf> {
        let base = std::env::var("APPDATA").ok()?;
        Some(PathBuf::from(base).join("overlime").join("settings.txt"))
    }

    pub fn load() -> Self {
        let mut s = Self::default();
        // verschlüsselte Datei (nur dein Windows-Konto kann sie lesen), sonst die alte Textdatei
        let from_dat = Self::path()
            .map(|p| p.with_file_name("settings.dat"))
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|enc| crate::store::unprotect(&enc))
            .map(|b| String::from_utf8_lossy(&b).to_string());
        let used_dat = from_dat.is_some();
        let Some(text) = from_dat.or_else(|| Self::path().and_then(|p| std::fs::read_to_string(p).ok())) else {
            return s;
        };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k.trim() {
                "resolution" => s.resolution = v.trim().parse().unwrap_or(s.resolution).min(2),
                "fps" => s.fps = v.trim().parse().unwrap_or(s.fps).min(1),
                "bitrate" => s.bitrate = v.trim().parse().unwrap_or(s.bitrate).clamp(2.0, 50.0),
                "codec" => s.codec = v.trim().parse().unwrap_or(s.codec).min(2),
                "upscale" => s.upscale = v.trim().parse().unwrap_or(s.upscale).min(3),
                "fullscreen" => s.fullscreen = v.trim() == "true",
                "rumble" => s.rumble = v.trim() == "true",
                "psn_account" => s.psn_account = v.trim().to_string(),
                "psn_name" => s.psn_name = v.trim().to_string(),
                "auto_update" => s.auto_update = v.trim() != "false",
                "manual_ip" => s.manual_ip = v.trim().to_string(),
                "last_version" => s.last_version = v.trim().to_string(),
                "theme" => s.theme = v.trim().to_string(),
                "discord" => s.discord = v.trim() == "true",
                "discord_game" => s.discord_game = v.trim() != "false",
                "discord_id" => s.discord_id = v.trim().to_string(),
                "autoconnect" => s.autoconnect = v.trim() == "true",
                "ask_feedback" => s.ask_feedback = v.trim() != "false",
                "ui_scale" => s.ui_scale = v.trim().parse().unwrap_or(0usize).min(2),
                "news_seen" => s.news_seen = v.trim().to_string(),
                "sharp" => s.sharp = v.trim().parse().unwrap_or(0.55f32).clamp(0.0, 1.0),
                "ds_features" => s.ds_features = v.trim() != "false",
                "kb_mode" => s.kb_mode = v.trim().parse().unwrap_or(0usize).min(2),
                "volume" => s.volume = v.trim().parse().unwrap_or(1.0f32).clamp(0.0, 1.5),
                "audio_device" => s.audio_device = v.trim().to_string(),
                "welcome_anim" => s.welcome_anim = v.trim() != "false",
                "eq_bass" => s.eq_bass = v.trim().parse().unwrap_or(0.0f32).clamp(-8.0, 8.0),
                "eq_mid" => s.eq_mid = v.trim().parse().unwrap_or(0.0f32).clamp(-8.0, 8.0),
                "eq_treble" => s.eq_treble = v.trim().parse().unwrap_or(0.0f32).clamp(-8.0, 8.0),
                "language" => s.language = v.trim().parse().unwrap_or(0usize).min(1),
                "interp" => s.interp = v.trim() == "true",
                "hud_items" => s.hud_items = v.trim().parse().unwrap_or(0x0FFu32) & 0x3FF,
                "hud_pos" => s.hud_pos = v.trim().parse().unwrap_or(0usize).min(3),
                "hud_size" => s.hud_size = v.trim().parse().unwrap_or(1usize).min(2),
                "hud_box" => s.hud_box = v.trim() == "true",
                "tour_done" => s.tour_done = v.trim() == "true",
                "intro" => s.intro = v.trim() != "false",
                "notify_ready" => s.notify_ready = v.trim() != "false",
                "wake_on" => s.wake_on = v.trim() == "true",
                "wake_hour" => s.wake_hour = v.trim().parse().unwrap_or(19u8).min(23),
                "wake_min" => s.wake_min = v.trim().parse().unwrap_or(0u8).min(59),
                "wake_days" => s.wake_days = v.trim().parse().unwrap_or(127u8) & 127,
                "profile" => {
                    let f: Vec<&str> = v.trim().split('|').collect();
                    if f.len() == 5 {
                        s.profiles.push((
                            f[0].to_string(),
                            Profile {
                                resolution: f[1].parse().unwrap_or(2usize).min(2),
                                fps: f[2].parse().unwrap_or(1usize).min(1),
                                bitrate: f[3].parse().unwrap_or(30.0f32).clamp(2.0, 50.0),
                                codec: f[4].parse().unwrap_or(0usize).min(2),
                            },
                        ));
                    }
                }
                k if k.starts_with("key_") => {
                    if let Ok(i) = k[4..].parse::<usize>() {
                        if i < s.keys.len() {
                            s.keys[i] = v.trim().to_string();
                        }
                    }
                }
                _ => {}
            }
        }
        if !used_dat {
            s.save(); // alte Textdatei in die verschlüsselte Datei übernehmen
        }
        s
    }

    pub fn save(&self) {
        let Some(p) = Self::path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text = format!(
            "resolution={}\nfps={}\nbitrate={}\ncodec={}\nupscale={}\nfullscreen={}\nrumble={}\npsn_account={}\npsn_name={}\nauto_update={}\nmanual_ip={}\n",
            self.resolution, self.fps, self.bitrate, self.codec, self.upscale, self.fullscreen, self.rumble, self.psn_account, self.psn_name, self.auto_update, self.manual_ip
        );
        let mut text = text;
        text.push_str(&format!("last_version={}\ntheme={}\n", self.last_version, self.theme));
        text.push_str(&format!("discord={}\ndiscord_game={}\ndiscord_id={}\n", self.discord, self.discord_game, self.discord_id));
        text.push_str(&format!("welcome_anim={}\n", self.welcome_anim));
        text.push_str(&format!("autoconnect={}\n", self.autoconnect));
        text.push_str(&format!("ask_feedback={}\n", self.ask_feedback));
        text.push_str(&format!("volume={}\naudio_device={}\n", self.volume, self.audio_device));
        text.push_str(&format!("kb_mode={}\n", self.kb_mode));
        text.push_str(&format!("ds_features={}\n", self.ds_features));
        text.push_str(&format!("sharp={}\n", self.sharp));
        text.push_str(&format!("ui_scale={}\nnews_seen={}\n", self.ui_scale, self.news_seen));
        for (i, k) in self.keys.iter().enumerate() {
            text.push_str(&format!("key_{i}={k}\n"));
        }
        text.push_str(&format!("wake_on={}\nwake_hour={}\nwake_min={}\nwake_days={}\n", self.wake_on, self.wake_hour, self.wake_min, self.wake_days));
        text.push_str(&format!("eq_bass={}\neq_mid={}\neq_treble={}\n", self.eq_bass, self.eq_mid, self.eq_treble));
        text.push_str(&format!("notify_ready={}\n", self.notify_ready));
        text.push_str(&format!("tour_done={}\n", self.tour_done));
        text.push_str(&format!("intro={}\n", self.intro));
        text.push_str(&format!("hud_items={}\nhud_pos={}\nhud_size={}\nhud_box={}\n", self.hud_items, self.hud_pos, self.hud_size, self.hud_box));
        text.push_str(&format!("interp={}\n", self.interp));
        text.push_str(&format!("language={}\n", self.language));
        for (id, p) in &self.profiles {
            text.push_str(&format!("profile={}|{}|{}|{}|{}\n", id, p.resolution, p.fps, p.bitrate, p.codec));
        }
        // verschlüsselt speichern; die alte Textdatei wird entfernt
        match crate::store::protect(text.as_bytes()) {
            Some(enc) if std::fs::write(p.with_file_name("settings.dat"), &enc).is_ok() => {
                let _ = std::fs::remove_file(&p);
            }
            _ => {
                let _ = std::fs::write(p, text);
            }
        }
    }
}
