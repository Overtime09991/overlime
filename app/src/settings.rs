//! Einstellungen, gespeichert in %APPDATA%\overlime\settings.txt (key=value).
use std::path::PathBuf;

#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
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
    pub audio_device: String,
    pub welcome_anim: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
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
        let Some(text) = Self::path().and_then(|p| std::fs::read_to_string(p).ok()) else {
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
                "sharp" => s.sharp = v.trim().parse().unwrap_or(0.55f32).clamp(0.0, 1.0),
                "ds_features" => s.ds_features = v.trim() != "false",
                "kb_mode" => s.kb_mode = v.trim().parse().unwrap_or(0usize).min(2),
                "volume" => s.volume = v.trim().parse().unwrap_or(1.0f32).clamp(0.0, 1.5),
                "audio_device" => s.audio_device = v.trim().to_string(),
                "welcome_anim" => s.welcome_anim = v.trim() != "false",
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
        for (i, k) in self.keys.iter().enumerate() {
            text.push_str(&format!("key_{i}={k}\n"));
        }
        let _ = std::fs::write(p, text);
    }
}
