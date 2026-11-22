//! Gekoppelte Konsolen werden in %APPDATA%\overlime\paired.txt gespeichert.
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

pub fn load() -> Vec<Paired> {
    let Some(text) = path().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return vec![];
    };
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
    let _ = std::fs::write(p, text);
}
