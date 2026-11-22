//! Everything related to text, fonts, text layout, cursors etc.

pub mod cursor;
mod font;
mod fonts;
mod text_layout;
mod text_layout_types;

/// One `\t` character is this many spaces wide.
pub const TAB_SIZE: usize = 4;

pub use {
    fonts::{FontData, FontDefinitions, FontFamily, FontId, FontTweak, Fonts, FontsImpl},
    text_layout::layout,
    text_layout_types::*,
};

/// Suggested character to use to replace those in password text fields.
pub const PASSWORD_REPLACEMENT_CHAR: char = '•';


/// OVERLIME: Eine Funktion, die Text (z. B. ins Englische) übersetzt. Gibt `None` zurück, wenn nichts zu ändern ist.
pub type Translator = fn(&str) -> Option<String>;
pub static TRANSLATOR: std::sync::OnceLock<Translator> = std::sync::OnceLock::new();

/// Setzt die Übersetzungsfunktion (nur einmal möglich).
pub fn set_translator(f: Translator) {
    let _ = TRANSLATOR.set(f);
}

pub(crate) fn translate_job(tr: &Translator, job: &mut LayoutJob) {
    if job.text.is_empty() {
        return;
    }
    // nur, wenn die Abschnitte den ganzen Text lückenlos abdecken
    let mut pos = 0usize;
    for s in &job.sections {
        if s.byte_range.start != pos {
            return;
        }
        pos = s.byte_range.end;
    }
    if pos != job.text.len() {
        return;
    }
    let mut new_text = String::with_capacity(job.text.len());
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::with_capacity(job.sections.len());
    let mut changed = false;
    for s in &job.sections {
        let piece = &job.text[s.byte_range.clone()];
        let start = new_text.len();
        match tr(piece) {
            Some(x) => {
                new_text.push_str(&x);
                changed = true;
            }
            None => new_text.push_str(piece),
        }
        ranges.push(start..new_text.len());
    }
    if changed {
        job.text = new_text;
        for (s, r) in job.sections.iter_mut().zip(ranges) {
            s.byte_range = r;
        }
    }
}
