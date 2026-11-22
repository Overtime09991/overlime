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
        let _ = std::fs::write(p, text);
    }
}
