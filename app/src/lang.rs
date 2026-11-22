//! Sprache der Oberfläche (Deutsch oder Englisch).
//! Die deutschen Texte stehen im Quelltext. Für Englisch werden sie beim Zeichnen übersetzt
//! (Haken im gebündelten epaint), mit einem Wörterbuch aus `lang_en.rs`.
//! Texte mit Werten (z. B. „Update {} verfügbar“) werden über Vorlagen mit `{}` erkannt.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

static LANG: AtomicU8 = AtomicU8::new(0);

/// 0 = Deutsch, 1 = English
pub fn set(lang: usize) {
    LANG.store(lang.min(1) as u8, Ordering::Relaxed);
}

pub fn is_english() -> bool {
    LANG.load(Ordering::Relaxed) == 1
}

struct Template {
    parts: Vec<&'static str>,
    en: &'static str,
}

struct Dict {
    exact: HashMap<&'static str, &'static str>,
    templates: Vec<Template>,
}

fn dict() -> &'static Dict {
    static D: OnceLock<Dict> = OnceLock::new();
    D.get_or_init(|| {
        let mut exact = HashMap::new();
        let mut templates = Vec::new();
        for (de, en) in crate::lang_en::EN {
            if de.contains("{}") {
                templates.push(Template { parts: de.split("{}").collect(), en });
            } else {
                exact.insert(*de, *en);
            }
        }
        // spezifischere (längere) Vorlagen zuerst
        templates.sort_by_key(|t| std::cmp::Reverse(t.parts.iter().map(|p| p.len()).sum::<usize>()));
        Dict { exact, templates }
    })
}

/// Passt `text` auf die Vorlage? Gibt die Werte für die Platzhalter zurück.
fn match_template<'a>(parts: &[&str], text: &'a str) -> Option<Vec<&'a str>> {
    let n = parts.len();
    if n < 2 {
        return None;
    }
    if !text.starts_with(parts[0]) {
        return None;
    }
    let last = parts[n - 1];
    if !text.ends_with(last) || text.len() < parts[0].len() + last.len() {
        return None;
    }
    let end = text.len() - last.len();
    let mut pos = parts[0].len();
    let mut args = Vec::with_capacity(n - 1);
    for i in 1..n - 1 {
        let lit = parts[i];
        let found = if lit.is_empty() { Some(pos) } else { text[pos..end].find(lit).map(|p| p + pos) };
        let f = found?;
        args.push(&text[pos..f]);
        pos = f + lit.len();
    }
    if pos > end {
        return None;
    }
    args.push(&text[pos..end]);
    Some(args)
}

fn fill(en: &str, args: &[String]) -> String {
    let mut out = String::with_capacity(en.len() + 16);
    let mut idx = 0usize;
    let b = en.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' {
            if let Some(close) = en[i..].find('}') {
                let inner = &en[i + 1..i + close];
                if inner.is_empty() {
                    if let Some(a) = args.get(idx) {
                        out.push_str(a);
                    }
                    idx += 1;
                    i += close + 1;
                    continue;
                } else if let Ok(n) = inner.parse::<usize>() {
                    if let Some(a) = args.get(n.saturating_sub(1)) {
                        out.push_str(a);
                    }
                    i += close + 1;
                    continue;
                }
            }
        }
        let ch = en[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Entfernt führende Symbole (Icons, Leerzeichen, Aufzählungspunkte) und gibt (Präfix, Rest) zurück.
fn split_prefix(s: &str) -> (&str, &str) {
    for (i, c) in s.char_indices() {
        if c.is_alphanumeric() || c == '{' || c == '„' || c == '“' || c == '"' || c == '(' {
            // Icons liegen im privaten Bereich und gelten für is_alphanumeric als falsch
            return (&s[..i], &s[i..]);
        }
    }
    (s, "")
}

fn core(body: &str, depth: u8) -> Option<String> {
    let d = dict();
    if let Some(en) = d.exact.get(body) {
        return Some((*en).to_string());
    }
    for t in &d.templates {
        if let Some(args) = match_template(&t.parts, body) {
            // Werte können selbst Text enthalten (z. B. eine Fehlermeldung): auch sie übersetzen
            let tr_args: Vec<String> = args.iter().map(|a| if depth < 2 { line(a, depth + 1).unwrap_or_else(|| (*a).to_string()) } else { (*a).to_string() }).collect();
            return Some(fill(t.en, &tr_args));
        }
    }
    None
}

fn line(text: &str, depth: u8) -> Option<String> {
    let t = text;
    let (pre, body) = split_prefix(t);
    if body.is_empty() {
        return None;
    }
    // Nachgestellte Leerzeichen bleiben erhalten
    let trimmed = body.trim_end();
    let post = &body[trimmed.len()..];
    if let Some(x) = core(trimmed, depth) {
        return Some(format!("{pre}{x}{post}"));
    }
    // an Trennzeichen zerlegen: "A  •  B"
    for sep in ["  •  ", " • ", " – "] {
        if trimmed.contains(sep) {
            let mut changed = false;
            let parts: Vec<String> = trimmed
                .split(sep)
                .map(|p| match line(p, depth + 1) {
                    Some(x) => {
                        changed = true;
                        x
                    }
                    None => p.to_string(),
                })
                .collect();
            if changed {
                return Some(format!("{pre}{}{post}", parts.join(sep)));
            }
        }
    }
    None
}

fn translate_uncached(text: &str) -> Option<String> {
    if text.contains('\n') {
        let mut changed = false;
        let parts: Vec<String> = text
            .split('\n')
            .map(|l| match line(l, 0) {
                Some(x) => {
                    changed = true;
                    x
                }
                None => l.to_string(),
            })
            .collect();
        return if changed { Some(parts.join("\n")) } else { None };
    }
    line(text, 0)
}

fn cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static C: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Debug: nicht übersetzte deutsche Texte sammeln (Umgebungsvariable OL_LANG_DEBUG).
fn note_miss(text: &str) {
    static ON: OnceLock<bool> = OnceLock::new();
    if !*ON.get_or_init(|| std::env::var("OL_LANG_DEBUG").is_ok()) {
        return;
    }
    if !text.chars().any(|c| c.is_alphabetic()) || text.len() > 400 {
        return;
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("ol_lang_miss.txt")) {
        let _ = writeln!(f, "{}", text.replace('\n', "\\n"));
    }
}

/// Übersetzungsfunktion für den Haken in epaint. Gibt `None` zurück, wenn nichts zu ändern ist.
pub fn translate(text: &str) -> Option<String> {
    if LANG.load(Ordering::Relaxed) != 1 || text.is_empty() {
        return None;
    }
    {
        let c = cache().lock().unwrap();
        if let Some(r) = c.get(text) {
            return r.clone();
        }
    }
    let r = translate_uncached(text);
    if r.is_none() {
        note_miss(text);
    }
    let mut c = cache().lock().unwrap();
    if c.len() > 6000 {
        c.clear();
    }
    c.insert(text.to_string(), r.clone());
    r
}

/// Übersetzt einen einzelnen Text (für Stellen außerhalb von egui, z. B. das Symbol neben der Uhr).
pub fn tr(text: &str) -> String {
    translate(text).unwrap_or_else(|| text.to_string())
}

pub fn install() {
    eframe::egui::epaint::text::set_translator(translate);
}
