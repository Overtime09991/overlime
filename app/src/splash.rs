//! Update-Bildschirm und Willkommens-Animation nach dem Update.
use super::*;
use std::f32::consts::PI;

/// Neuigkeiten, die nach dem Update gezeigt werden.
pub const NEWS: &[&str] = &[
    "Von unterwegs spielen und bessere Controller-Erkennung",
    "Weniger Ruckeln bei hoher Bitrate und Anzeige der Programm-Last",
    "LAN-Modus und bis zu 150 Mbit/s Bitrate",
    "Replay-Clips, Auto-Highlights und Videos zuschneiden",
    "Controller-Licht und Clips an Discord senden",
    "Zeitlupe, Einzelbild und Profile pro Spiel",
    "Anrufe mit Klingelton, auch im Spiel",
    "Freunde & Sprache: eigener Sprachchat",
    "Rauschfilter wie Krisp",
    "Seite Module und Mikrofon zur Konsole",
    "Controller-Feinabstimmung und Zielhilfe",
    "Neue Start-Animation beim Öffnen",
    "Aufnahmen werden jetzt richtig gespeichert und lassen sich abspielen",
    "Neue Galerie: Videos abspielen und Screenshots ansehen",
    "Teilen per Zwischenablage",
    "Aufnahmen und Screenshots in der App ansehen",
    "Account-ID zum Kopieren für andere Apps",
    "Klarer Fokus auf Controller, ohne Tastatur-Steuerung",
    "Großer Haupt-Knopf auf der Startseite",
    "Demo-Modus zum Ausprobieren ohne Konsole",
    "Verständliche Fehlermeldungen beim Update",
    "Neues App-Symbol",
    "Neuigkeiten laden auch bei GitHub-Limit",
    "Updateprüfung klappt auch bei GitHub-Limit",
    "Neuer, ruhiger Update-Abschlussbildschirm",
    "Update startet OVERLIME nahtlos neu",
    "Schlichteres, flaches Design für Knöpfe und Auswahl",
];

fn rnd(i: u32, k: u32) -> f32 {
    let x = ((i * 127 + k * 311 + 17) as f32).sin() * 43758.547;
    x - x.floor()
}

fn arc(c: Pos2, r: f32, start: f32, sweep: f32) -> Vec<Pos2> {
    let steps = ((sweep.abs() / (2.0 * PI)) * 90.0).ceil().max(2.0) as usize;
    (0..=steps)
        .map(|i| {
            let a = start + sweep * i as f32 / steps as f32;
            Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

fn ring(p: &egui::Painter, c: Pos2, r: f32, frac: f32, width: f32, col: Color32) {
    p.add(Shape::line(arc(c, r, 0.0, 2.0 * PI), Stroke::new(width, Color32::from_white_alpha(18))));
    let frac = frac.clamp(0.0, 1.0);
    if frac > 0.002 {
        p.add(Shape::line(arc(c, r, -PI / 2.0, 2.0 * PI * frac), Stroke::new(width, col)));
        // leuchtende Spitze
        let a = -PI / 2.0 + 2.0 * PI * frac;
        let tip = Pos2::new(c.x + r * a.cos(), c.y + r * a.sin());
        p.circle_filled(tip, width * 1.6, col.gamma_multiply(0.30));
        p.circle_filled(tip, width * 0.8, Color32::WHITE);
    }
}

fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

fn background(p: &egui::Painter, r: Rect, t: f64) {
    p.rect_filled(r, Rounding::ZERO, th_bg());
    let a = (t * 0.35) as f32;
    p.circle_filled(Pos2::new(r.width() * (0.2 + 0.06 * a.sin()), r.height() * 0.3), 340.0, th_blue().gamma_multiply(0.10));
    p.circle_filled(Pos2::new(r.width() * (0.85 + 0.05 * (a * 1.3).cos()), r.height() * 0.75), 380.0, Color32::from_rgb(100, 60, 230).gamma_multiply(0.09));
    // aufsteigende Lichtpunkte
    for i in 0..46u32 {
        let x = r.left() + rnd(i, 1) * r.width();
        let speed = 18.0 + rnd(i, 2) * 46.0;
        let y = r.bottom() - ((t as f32 * speed + rnd(i, 3) * r.height()) % (r.height() + 40.0)) + 20.0;
        let size = 1.0 + rnd(i, 4) * 2.2;
        let tw = 0.4 + 0.6 * (t as f32 * (0.6 + rnd(i, 5)) + rnd(i, 6) * 6.0).sin().abs();
        p.circle_filled(Pos2::new(x, y), size, th_blue_light().gamma_multiply(0.45 * tw));
    }
}

fn chip(p: &egui::Painter, center: Pos2, text: &str, hot: bool, alpha: f32) -> f32 {
    let col = if hot { GREEN } else { TEXT_DIM };
    let g = p.layout_no_wrap(text.to_string(), FontId::monospace(17.0), col.gamma_multiply(alpha));
    let size = g.size() + Vec2::new(30.0, 14.0);
    let rect = Rect::from_center_size(center, size);
    p.rect_filled(rect, Rounding::same(size.y / 2.0), col.gamma_multiply(0.16 * alpha));
    p.rect_stroke(rect, Rounding::same(size.y / 2.0), Stroke::new(1.0, col.gamma_multiply(0.55 * alpha)));
    p.galley(Pos2::new(rect.min.x + 15.0, rect.center().y - g.size().y / 2.0), g, col);
    size.x
}

fn chevrons(p: &egui::Painter, c: Pos2, t: f64, col: Color32) {
    for i in 0..3 {
        let ph = ((t as f32 * 1.6 - i as f32 * 0.28) % 1.0 + 1.0) % 1.0;
        let a = (ph * PI).sin().max(0.0);
        let x = c.x - 16.0 + i as f32 * 16.0;
        let s = Stroke::new(3.0, col.gamma_multiply(0.25 + 0.75 * a));
        p.line_segment([Pos2::new(x - 4.0, c.y - 8.0), Pos2::new(x + 4.0, c.y)], s);
        p.line_segment([Pos2::new(x + 4.0, c.y), Pos2::new(x - 4.0, c.y + 8.0)], s);
    }
}

pub fn tick(p: &egui::Painter, c: Pos2, s: f32, col: Color32, frac: f32) {
    let a = c + Vec2::new(-0.45 * s, 0.02 * s);
    let b = c + Vec2::new(-0.12 * s, 0.38 * s);
    let d = c + Vec2::new(0.5 * s, -0.36 * s);
    let st = Stroke::new((s * 0.16).max(2.0), col);
    let f1 = (frac * 2.0).clamp(0.0, 1.0);
    let f2 = (frac * 2.0 - 1.0).clamp(0.0, 1.0);
    p.line_segment([a, a + (b - a) * f1], st);
    if f2 > 0.0 {
        p.line_segment([b, b + (d - b) * f2], st);
    }
}

const STAGES: [&str; 5] = ["Verbindung zu GitHub", "Update herunterladen", "Prüfsumme prüfen", "Installieren", "OVERLIME neu starten"];
const STAGE_TITLES: [(&str, &str); 5] = [
    ("Verbinde mit GitHub …", "Wir suchen die neue Version"),
    ("Update wird heruntergeladen …", "Gleich ist alles da"),
    ("Prüfe Echtheit und Prüfsumme …", "Sicherheit geht vor"),
    ("Update wird installiert …", "Fast geschafft"),
    ("OVERLIME startet neu …", "Bis gleich!"),
];

const VIOLET: Color32 = Color32::from_rgb(170, 120, 255);

fn lerp_col(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// Farbverlauf des Fortschritts: blau, violett, grün.
fn grad(f: f32) -> Color32 {
    let f = f.clamp(0.0, 1.0);
    if f < 0.55 {
        lerp_col(th_blue_light(), VIOLET, f / 0.55)
    } else {
        lerp_col(VIOLET, GREEN, (f - 0.55) / 0.45)
    }
}

fn qbez(a: Pos2, c: Pos2, b: Pos2, t: f32) -> Pos2 {
    let u = 1.0 - t;
    Pos2::new(u * u * a.x + 2.0 * u * t * c.x + t * t * b.x, u * u * a.y + 2.0 * u * t * c.y + t * t * b.y)
}

/// Weicher, runder Lichtschein (Verlauf von der Mitte nach außen, ohne sichtbare Stufen).
fn radial(p: &egui::Painter, c: Pos2, r: f32, col: Color32, a: f32) {
    let n = 48;
    let mut m = egui::epaint::Mesh::default();
    let cc = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), (a.clamp(0.0, 1.0) * 255.0) as u8);
    let oc = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 0);
    m.colored_vertex(c, cc);
    for i in 0..=n {
        let ang = 2.0 * PI * i as f32 / n as f32;
        m.colored_vertex(Pos2::new(c.x + r * ang.cos(), c.y + r * ang.sin()), oc);
    }
    for i in 0..n {
        m.add_triangle(0, 1 + i as u32, 2 + i as u32);
    }
    p.add(Shape::mesh(m));
}

/// Ruhiger Hintergrund: dunkel mit weichen, langsam wandernden Farbschleiern und leichter Abdunkelung am Rand.
fn background_v2(p: &egui::Painter, r: Rect, t: f64, shown: f32, err: bool) {
    p.rect_filled(r, Rounding::ZERO, th_bg());
    let a = t as f32 * 0.18;
    let accent = if err { RED } else { grad(shown) };
    radial(p, Pos2::new(r.left() + r.width() * (0.22 + 0.05 * a.sin()), r.top() + r.height() * 0.25), r.width() * 0.55, th_blue(), 0.16);
    radial(p, Pos2::new(r.left() + r.width() * (0.82 + 0.04 * (a * 0.8).cos()), r.top() + r.height() * 0.78), r.width() * 0.6, Color32::from_rgb(110, 60, 235), 0.14);
    radial(p, Pos2::new(r.center().x, r.top() + r.height() * 0.42), r.width() * 0.42, accent, 0.07);
    // wenige, sehr dezente Lichtpunkte
    for i in 0..16u32 {
        let x = r.left() + rnd(i, 1) * r.width();
        let speed = 6.0 + rnd(i, 2) * 14.0;
        let y = r.bottom() - ((t as f32 * speed + rnd(i, 3) * r.height()) % (r.height() + 30.0)) + 15.0;
        let tw = 0.25 + 0.75 * (t as f32 * (0.4 + rnd(i, 5)) + rnd(i, 6) * 6.0).sin().abs();
        p.circle_filled(Pos2::new(x, y), 1.0 + rnd(i, 4) * 1.4, Color32::WHITE.gamma_multiply(0.10 * tw));
    }
}

/// Glas-Karte mit weichem Schatten.
fn glass(p: &egui::Painter, rc: Rect) {
    for i in 0..7 {
        let g = rc.expand(2.0 + i as f32 * 5.0).translate(Vec2::new(0.0, 8.0));
        p.rect_filled(g, Rounding::same(22.0 + i as f32 * 5.0), Color32::from_black_alpha(9));
    }
    p.rect_filled(rc, Rounding::same(22.0), Color32::from_rgba_unmultiplied(20, 26, 48, 205));
    // heller Verlauf oben
    p.rect_stroke(rc, Rounding::same(22.0), Stroke::new(1.0, Color32::from_white_alpha(30)));
}

/// Breiter Fortschrittsbalken mit Farbverlauf, Leuchten an der Spitze und wanderndem Glanz.
fn progress_bar(p: &egui::Painter, rc: Rect, frac: f32, t: f32, color_scale: f32) {
    let h = rc.height();
    p.rect_filled(rc, Rounding::same(h / 2.0), Color32::from_white_alpha(16));
    p.rect_stroke(rc, Rounding::same(h / 2.0), Stroke::new(1.0, Color32::from_white_alpha(14)));
    let frac = frac.clamp(0.0, 1.0);
    if frac < 0.004 {
        return;
    }
    let w = (rc.width() * frac).max(h);
    let k = 48;
    let mut m = egui::epaint::Mesh::default();
    let (y0, y1) = (rc.top() + 1.5, rc.bottom() - 1.5);
    for i in 0..=k {
        let f = i as f32 / k as f32;
        let x = rc.left() + h / 2.0 + (w - h) * f;
        let col = grad(f * frac * color_scale + (1.0 - color_scale) * 0.0);
        m.colored_vertex(Pos2::new(x, y0), col);
        m.colored_vertex(Pos2::new(x, y1), col);
    }
    for i in 0..k as u32 {
        let a = i * 2;
        m.add_triangle(a, a + 1, a + 2);
        m.add_triangle(a + 1, a + 3, a + 2);
    }
    let (c0, c1) = (grad(0.0), grad(frac * color_scale));
    p.circle_filled(Pos2::new(rc.left() + h / 2.0, rc.center().y), (h - 3.0) / 2.0, c0);
    p.add(Shape::mesh(m));
    let tip = Pos2::new(rc.left() + w - h / 2.0, rc.center().y);
    p.circle_filled(tip, (h - 3.0) / 2.0, c1);
    radial(p, tip, 30.0, c1, 0.30);
    // Glanz
    let sx = rc.left() + h / 2.0 + (w - h) * ((t * 0.45) % 1.4).min(1.0);
    radial(p, Pos2::new(sx, rc.center().y), h * 1.4, Color32::WHITE, 0.10);
}

/// Symbol in einer abgerundeten Kachel (Computer, Server, Paket).
fn icon_tile(p: &egui::Painter, c: Pos2, label: &str, kind: u8, col: Color32, a: f32, t: f32) {
    let s = 46.0;
    let rc = Rect::from_center_size(c, Vec2::splat(s * 2.0));
    radial(p, c, 74.0, col, 0.07 * a);
    p.rect_filled(rc, Rounding::same(20.0), Color32::from_rgba_unmultiplied(255, 255, 255, (10.0 * a) as u8));
    p.rect_stroke(rc, Rounding::same(20.0), Stroke::new(1.2, col.gamma_multiply(0.55 * a)));
    let st = Stroke::new(2.2, col.gamma_multiply(0.95 * a));
    match kind {
        0 => {
            // Computer
            let scr = Rect::from_center_size(c + Vec2::new(0.0, -6.0), Vec2::new(46.0, 30.0));
            p.rect_filled(scr, Rounding::same(5.0), col.gamma_multiply(0.22 * a));
            p.rect_stroke(scr, Rounding::same(5.0), st);
            p.line_segment([c + Vec2::new(0.0, 10.0), c + Vec2::new(0.0, 17.0)], st);
            p.line_segment([c + Vec2::new(-13.0, 18.0), c + Vec2::new(13.0, 18.0)], Stroke::new(2.6, col.gamma_multiply(0.95 * a)));
            if ((t * 2.0) as i32) % 2 == 0 {
                p.line_segment([c + Vec2::new(-11.0, -6.0), c + Vec2::new(-3.0, -6.0)], Stroke::new(1.8, Color32::WHITE.gamma_multiply(0.8 * a)));
            }
        }
        1 => {
            // Server
            for k in 0..3 {
                let r = Rect::from_center_size(c + Vec2::new(0.0, -17.0 + k as f32 * 17.0), Vec2::new(50.0, 13.0));
                p.rect_filled(r, Rounding::same(4.0), col.gamma_multiply(0.22 * a));
                p.rect_stroke(r, Rounding::same(4.0), st);
                let blink = 0.35 + 0.65 * ((t * 3.0 + k as f32 * 1.7).sin() * 0.5 + 0.5);
                p.circle_filled(r.right_center() - Vec2::new(8.0, 0.0), 2.4, GREEN.gamma_multiply(blink * a));
            }
        }
        _ => {
            // Paket (Würfel)
            let f = Rect::from_center_size(c + Vec2::new(-3.0, 6.0), Vec2::new(36.0, 30.0));
            let top = vec![f.left_top(), f.left_top() + Vec2::new(9.0, -11.0), f.right_top() + Vec2::new(9.0, -11.0), f.right_top()];
            let side = vec![f.right_top(), f.right_top() + Vec2::new(9.0, -11.0), f.right_bottom() + Vec2::new(9.0, -11.0), f.right_bottom()];
            p.add(Shape::convex_polygon(top, col.gamma_multiply(0.38 * a), st));
            p.add(Shape::convex_polygon(side, col.gamma_multiply(0.30 * a), st));
            p.rect_filled(f, Rounding::same(2.0), col.gamma_multiply(0.22 * a));
            p.rect_stroke(f, Rounding::same(2.0), st);
            p.line_segment([f.center_top(), f.center_bottom()], Stroke::new(1.6, col.gamma_multiply(0.7 * a)));
        }
    }
    p.text(c + Vec2::new(0.0, s + 12.0), Align2::CENTER_TOP, label, FontId::proportional(13.0), Color32::from_rgb(205, 216, 238).gamma_multiply(a));
}

/// Leuchtende Datenkapsel auf einer Bahn.
fn pill(p: &egui::Painter, pos: Pos2, col: Color32, a: f32) {
    radial(p, pos, 20.0, col, 0.45 * a);
    let r = Rect::from_center_size(pos, Vec2::new(24.0, 8.0));
    p.rect_filled(r, Rounding::same(4.0), col.gamma_multiply(0.95 * a));
    p.rect_filled(Rect::from_center_size(pos + Vec2::new(4.0, 0.0), Vec2::new(10.0, 3.0)), Rounding::same(2.0), Color32::WHITE.gamma_multiply(0.7 * a));
}

/// Kleine Statuskapsel ("DNS", "TLS", "API" und so weiter).
fn status_pill(p: &egui::Painter, c: Pos2, text: &str, ok: bool, t: f32, a: f32) {
    let col = if ok { GREEN } else { th_blue_light() };
    let rc = Rect::from_center_size(c, Vec2::new(70.0, 24.0));
    p.rect_filled(rc, Rounding::same(12.0), col.gamma_multiply(0.14 * a));
    p.rect_stroke(rc, Rounding::same(12.0), Stroke::new(1.0, col.gamma_multiply(0.6 * a)));
    p.text(c + Vec2::new(6.0, 0.0), Align2::CENTER_CENTER, text, FontId::monospace(12.0), Color32::from_rgb(215, 224, 240).gamma_multiply(a));
    let ic = Pos2::new(rc.left() + 13.0, c.y);
    if ok {
        tick(p, ic, 10.0, GREEN.gamma_multiply(a), 1.0);
    } else {
        let a0 = (t * 6.0) % (2.0 * PI);
        p.add(Shape::line(arc(ic, 5.0, a0, PI * 1.2), Stroke::new(1.8, col.gamma_multiply(a))));
    }
}

/// Ein Ordner-Symbol. `open` (0..1) hebt den Deckel an, `fill` (0..1) füllt ihn von unten.
fn folder(p: &egui::Painter, c: Pos2, open: f32, fill: f32, col: Color32, alpha: f32, label: &str) {
    let (w, h) = (76.0, 52.0);
    let body = Rect::from_center_size(c + Vec2::new(0.0, 4.0), Vec2::new(w, h));
    let a = alpha.clamp(0.0, 1.0);
    radial(p, c, 62.0, col, (0.07 + 0.14 * open) * a);
    let tab = Rect::from_min_size(Pos2::new(body.min.x, body.min.y - 8.0), Vec2::new(w * 0.42, 13.0));
    p.rect_filled(tab, Rounding { nw: 6.0, ne: 6.0, sw: 0.0, se: 0.0 }, col.gamma_multiply(0.6 * a));
    p.rect_filled(body, Rounding::same(8.0), col.gamma_multiply(0.30 * a));
    if fill > 0.0 {
        let fh = (h - 12.0) * fill.clamp(0.0, 1.0);
        let fr = Rect::from_min_max(Pos2::new(body.min.x + 6.0, body.max.y - 6.0 - fh), Pos2::new(body.max.x - 6.0, body.max.y - 6.0));
        p.rect_filled(fr, Rounding::same(4.0), col.gamma_multiply(0.55 * a));
    }
    let lift = 9.0 * open;
    let front = [Pos2::new(body.min.x, body.min.y + 13.0), Pos2::new(body.max.x, body.min.y + 13.0 - lift * 0.4), Pos2::new(body.max.x, body.max.y), Pos2::new(body.min.x, body.max.y)];
    p.add(Shape::convex_polygon(front.to_vec(), col.gamma_multiply(0.42 * a), Stroke::new(1.2, col.gamma_multiply(0.9 * a))));
    p.text(Pos2::new(c.x, body.max.y + 8.0), Align2::CENTER_TOP, label, FontId::proportional(12.0), Color32::from_rgb(205, 216, 238).gamma_multiply(a));
}

/// Eine kleine Dateikarte, die durchs Bild fliegt.
fn file_card(p: &egui::Painter, pos: Pos2, rot: f32, scale: f32, col: Color32, alpha: f32) {
    let (w, h) = (20.0 * scale, 26.0 * scale);
    let (sn, cs) = (rot.sin(), rot.cos());
    let tf = |x: f32, y: f32| Pos2::new(pos.x + x * cs - y * sn, pos.y + x * sn + y * cs);
    let pts = vec![tf(-w / 2.0, -h / 2.0), tf(w / 2.0 - 5.0 * scale, -h / 2.0), tf(w / 2.0, -h / 2.0 + 5.0 * scale), tf(w / 2.0, h / 2.0), tf(-w / 2.0, h / 2.0)];
    p.add(Shape::convex_polygon(pts, col.gamma_multiply(0.9 * alpha), Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.5 * alpha))));
    for k in 0..3 {
        let y = -h / 2.0 + (11.0 + k as f32 * 5.0) * scale * 0.7;
        p.line_segment([tf(-w / 2.0 + 4.0 * scale, y), tf(w / 2.0 - 4.0 * scale, y)], Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.55 * alpha)));
    }
}

/// Prüfsumme setzt sich aus wirbelnden Zeichen zusammen (echter SHA-256-Wert, wenn bekannt).
fn hash_reveal(p: &egui::Painter, cx: f32, y: f32, hash: &str, since: f32, alpha: f32) {
    let a = alpha.clamp(0.0, 1.0);
    let hex: Vec<char> = if hash.len() == 64 { hash.chars().collect() } else { "9716ead1f3a7e319052df3798585d72bfa59fcf966198de96dc70fefe3390899".chars().collect() };
    let q = ease_out(since / 2.4);
    let shown = (q * 64.0) as usize;
    let frame = (since * 18.0) as u32;
    for line in 0..2usize {
        let mut job = egui::text::LayoutJob::default();
        for i in 0..32usize {
            let idx = line * 32 + i;
            let (ch, col) = if idx < shown {
                (hex[idx], GREEN.gamma_multiply(0.95 * a))
            } else {
                let r = (rnd(idx as u32, frame) * 16.0) as usize % 16;
                ("0123456789abcdef".chars().nth(r).unwrap(), th_blue_light().gamma_multiply(0.5 * a))
            };
            job.append(&ch.to_string(), 0.0, egui::TextFormat { font_id: FontId::monospace(12.5), color: col, ..Default::default() });
        }
        let g = p.layout_job(job);
        p.galley(Pos2::new(cx - g.size().x / 2.0, y + line as f32 * 17.0), g, Color32::WHITE);
    }
}

fn fmt_mb(b: u64) -> String {
    format!("{:.1}", b as f64 / 1_048_576.0)
}

/// Die Szene in der Glas-Karte, je nach Stufe.
fn scene(p: &egui::Painter, card: Rect, stage: u8, since: f32, t: f32, prog: &update::Progress, err: Option<&str>) {
    let c = card.center();
    let half = card.width() * 0.5;
    let lx = c.x - (half - 108.0);
    let rx = c.x + (half - 108.0);
    let fade = ease_out(since / 0.45);
    if let Some(e) = err {
        // großer Fehler-Schild
        radial(p, c + Vec2::new(0.0, -20.0), 110.0, RED, 0.18);
        let pts = vec![c + Vec2::new(0.0, -78.0), c + Vec2::new(40.0, -62.0), c + Vec2::new(36.0, -18.0), c + Vec2::new(0.0, 10.0), c + Vec2::new(-36.0, -18.0), c + Vec2::new(-40.0, -62.0)];
        p.add(Shape::convex_polygon(pts, RED.gamma_multiply(0.22), Stroke::new(2.2, RED)));
        p.text(c + Vec2::new(0.0, -38.0), Align2::CENTER_CENTER, "!", FontId::proportional(40.0), RED);
        p.text(c + Vec2::new(0.0, 34.0), Align2::CENTER_TOP, "Update nicht möglich", FontId::proportional(20.0), Color32::WHITE);
        p.text(c + Vec2::new(0.0, 62.0), Align2::CENTER_TOP, e, FontId::proportional(12.5), RED.gamma_multiply(0.95));
        return;
    }
    match stage {
        0 => {
            icon_tile(p, Pos2::new(lx, c.y - 8.0), "Dein PC", 0, th_blue_light(), fade, t);
            icon_tile(p, Pos2::new(rx, c.y - 8.0), "GitHub", 1, th_blue_light(), fade, t);
            let (x0, x1) = (lx + 66.0, rx - 66.0);
            for (dy, rev) in [(-8.0, false), (9.0, true)] {
                p.line_segment([Pos2::new(x0, c.y - 8.0 + dy), Pos2::new(x1, c.y - 8.0 + dy)], Stroke::new(2.0, Color32::from_white_alpha(22)));
                for k in 0..4 {
                    let ph = (t * 0.45 + k as f32 * 0.25 + if rev { 0.125 } else { 0.0 }) % 1.0;
                    let x = if rev { x1 - (x1 - x0) * ph } else { x0 + (x1 - x0) * ph };
                    let edge = (ph.min(1.0 - ph) * 6.0).clamp(0.0, 1.0);
                    pill(p, Pos2::new(x, c.y - 8.0 + dy), if rev { GREEN } else { th_blue_light() }, edge * fade);
                }
            }
            for (i, (name, due)) in [("DNS", 0.6), ("TLS", 1.3), ("API", 2.0)].iter().enumerate() {
                status_pill(p, Pos2::new(c.x + (i as f32 - 1.0) * 80.0, c.y - 62.0), name, since > *due, t, fade);
            }
            let ping = 17 + ((t * 3.0).sin() * 4.0 + 5.0) as i32 + (rnd((t * 3.0) as u32, 9) * 3.0) as i32;
            p.text(Pos2::new(c.x, c.y + 28.0), Align2::CENTER_TOP, format!("api.github.com  •  {} ms", ping), FontId::monospace(12.5), Color32::from_rgb(190, 205, 235).gamma_multiply(0.9 * fade));
        }
        1 => {
            icon_tile(p, Pos2::new(lx, c.y - 8.0), "GitHub", 1, th_blue_light(), fade, t);
            icon_tile(p, Pos2::new(rx, c.y - 8.0), "Dein PC", 0, GREEN, fade, t);
            let (x0, x1) = (lx + 66.0, rx - 66.0);
            let ly = c.y - 8.0;
            p.line_segment([Pos2::new(x0, ly), Pos2::new(x1, ly)], Stroke::new(3.0, Color32::from_white_alpha(20)));
            let fr = (prog.pct / 100.0).clamp(0.0, 1.0);
            p.line_segment([Pos2::new(x0, ly), Pos2::new(x0 + (x1 - x0) * fr, ly)], Stroke::new(3.0, grad(fr)));
            let mbs = prog.speed / 1_048_576.0;
            let sp = 0.5 + (mbs / 12.0).clamp(0.0, 0.9);
            for k in 0..7 {
                let ph = (t * sp + k as f32 / 7.0) % 1.0;
                let edge = (ph.min(1.0 - ph) * 6.0).clamp(0.0, 1.0);
                pill(p, Pos2::new(x0 + (x1 - x0) * ph, ly), grad(ph), edge * fade);
            }
            let big = if prog.speed > 1.0 { format!("{:.1} MB/s", mbs) } else { "…".to_string() };
            p.text(Pos2::new(c.x, c.y - 70.0), Align2::CENTER_CENTER, big, FontId::proportional(34.0), Color32::WHITE.gamma_multiply(fade));
            if prog.total > 0 {
                let eta = if prog.speed > 1.0 { ((prog.total.saturating_sub(prog.bytes)) as f32 / prog.speed).ceil() as u32 } else { 0 };
                let mut s = format!("{} von {} MB", fmt_mb(prog.bytes), fmt_mb(prog.total));
                if eta > 0 {
                    s.push_str(&format!("  •  noch {} s", eta));
                }
                p.text(Pos2::new(c.x, c.y + 26.0), Align2::CENTER_TOP, s, FontId::proportional(13.5), TEXT_DIM.gamma_multiply(fade));
            }
        }
        2 => {
            // Schild mit Prüflinie
            let sc = Pos2::new(c.x, c.y - 36.0);
            let k = 1.5;
            let col = if since > 3.2 { GREEN } else { th_blue_light() };
            radial(p, sc, 100.0, col, 0.16 * fade);
            let pts: Vec<Pos2> = [(0.0, -30.0), (24.0, -20.0), (22.0, 8.0), (0.0, 28.0), (-22.0, 8.0), (-24.0, -20.0)].iter().map(|(x, y)| sc + Vec2::new(x * k, y * k)).collect();
            p.add(Shape::convex_polygon(pts, col.gamma_multiply(0.20 * fade), Stroke::new(2.2, col.gamma_multiply(fade))));
            if since > 3.2 {
                tick(p, sc + Vec2::new(0.0, -2.0), 40.0, GREEN.gamma_multiply(fade), ease_out((since - 3.2) / 0.4));
            } else {
                let tri = ((t * 0.9) % 2.0 - 1.0).abs();
                let y = sc.y - 30.0 * k + 58.0 * k * tri;
                let wdt = 22.0 * k * (1.0 - ((tri - 0.5).abs() * 0.4));
                p.line_segment([Pos2::new(sc.x - wdt, y), Pos2::new(sc.x + wdt, y)], Stroke::new(2.4, Color32::WHITE.gamma_multiply(0.85 * fade)));
                radial(p, Pos2::new(sc.x, y), 34.0, th_blue_light(), 0.25 * fade);
            }
            let chips = [("SHA-256", c.x - half + 108.0, 2.6), ("Ed25519", c.x + half - 108.0, 3.2)];
            for (name, x, due) in chips {
                status_pill(p, Pos2::new(x, c.y - 36.0), name, since > due, t, fade);
            }
            hash_reveal(p, c.x, c.y + 38.0, &prog.hash, since, fade);
        }
        3 => {
            let src = Pos2::new(lx + 10.0, c.y - 8.0);
            icon_tile(p, src, "Update", 2, th_blue_light(), fade, t);
            let spots = [(c.x + 100.0, c.y - 62.0), (c.x + 222.0, c.y - 62.0), (c.x + 100.0, c.y + 46.0), (c.x + 222.0, c.y + 46.0)];
            let spots = if card.width() < 600.0 { [(c.x + 70.0, c.y - 62.0), (c.x + 170.0, c.y - 62.0), (c.x + 70.0, c.y + 46.0), (c.x + 170.0, c.y + 46.0)] } else { spots };
            let labels = ["Programm", "Daten", "Sprachen", "Verknüpfungen"];
            let palette = [th_blue_light(), VIOLET, GREEN, Color32::from_rgb(255, 200, 90)];
            const N: usize = 12;
            let mut arrived = [0usize; 4];
            let mut flights: Vec<(usize, f32)> = Vec::new();
            for k in 0..N {
                let x = (since - (0.5 + k as f32 * 0.2)) / 0.7;
                if x >= 1.0 {
                    arrived[k % 4] += 1;
                } else if x > 0.0 {
                    flights.push((k, x));
                }
            }
            for (i, (fx, fy)) in spots.iter().enumerate() {
                let fill = arrived[i] as f32 / (N / 4) as f32;
                let near = flights.iter().any(|(k, f)| k % 4 == i && *f > 0.55);
                let open: f32 = (if near { 0.85f32 } else { 0.0 } + if fill > 0.0 && fill < 1.0 { 0.2f32 } else { 0.0 }).min(1.0);
                folder(p, Pos2::new(*fx, *fy), open, fill, lerp_col(palette[i], GREEN, fill), fade, labels[i]);
                if fill >= 1.0 {
                    let ic = Pos2::new(*fx + 30.0, *fy - 26.0);
                    p.circle_filled(ic, 10.0, GREEN);
                    tick(p, ic, 11.0, Color32::WHITE, 1.0);
                }
            }
            for (k, f) in flights {
                let (tx, ty) = spots[k % 4];
                let target = Pos2::new(tx, ty);
                let ctrl = Pos2::new((src.x + target.x) / 2.0, src.y.min(target.y) - 70.0 - (k % 3) as f32 * 12.0);
                let e = ease_out(f);
                let pos = qbez(src, ctrl, target, e);
                for s in 1..4 {
                    let q = qbez(src, ctrl, target, ease_out((f - s as f32 * 0.05).max(0.0)));
                    radial(p, q, 11.0, palette[(k / 4) % 4], 0.22 / s as f32);
                }
                let a = (f * 6.0).min(1.0) * (1.0 - ((f - 0.85) / 0.15).clamp(0.0, 1.0) * 0.5);
                file_card(p, pos, (k as f32 * 0.9 + f * 5.0).sin() * 0.5, 1.2 - 0.5 * e, palette[(k / 4) % 4], a);
            }
        }
        _ => {
            // Fertig: großes Häkchen mit Wellen
            radial(p, c, 130.0, GREEN, 0.18 * fade);
            for i in 0..3 {
                let ph = ((since * 0.7 + i as f32 / 3.0) % 1.0).max(0.0);
                p.circle_stroke(c, 46.0 + 90.0 * ph, Stroke::new(1.6, GREEN.gamma_multiply(0.45 * (1.0 - ph))));
            }
            p.circle_filled(c, 50.0, GREEN.gamma_multiply(0.20));
            p.circle_stroke(c, 50.0, Stroke::new(3.0, GREEN));
            tick(p, c, 56.0, GREEN, ease_out(since / 0.6));
        }
    }
}

/// Zeigt die Update-Animation. Gibt true zurück, wenn bei einem Fehler auf "Zurück" geklickt wurde.
pub fn update_screen(ctx: &egui::Context, t: f64, prog: &update::Progress, shown: f32, from: &str, to: &str) -> bool {
    let (stage, err) = (prog.stage, prog.error.as_deref());
    let mut back = false;
    let stage_t0 = ctx.data_mut(|d| {
        let mut st = d.get_temp::<(u8, f64)>(egui::Id::new("upd_stage_t")).unwrap_or((stage, t));
        if st.0 != stage {
            st = (stage, t);
        }
        d.insert_temp(egui::Id::new("upd_stage_t"), st);
        st.1
    });
    egui::CentralPanel::default().frame(egui::Frame::none().fill(th_bg())).show(ctx, |ui| {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        let tf = t as f32;
        let since = (t - stage_t0) as f32;
        background_v2(&p, r, t, shown, err.is_some());
        let cx = r.center().x;
        let pulse = 0.5 + 0.5 * (tf * 2.0).sin();

        // Kopfzeile: Name links, Versionen rechts
        p.text(Pos2::new(r.left() + 30.0, r.top() + 24.0), Align2::LEFT_TOP, "OVERLIME", FontId::proportional(24.0), Color32::WHITE);
        let tag = Rect::from_min_size(Pos2::new(r.left() + 142.0, r.top() + 29.0), Vec2::new(64.0, 20.0));
        p.rect_filled(tag, Rounding::same(10.0), th_blue().gamma_multiply(0.30));
        p.text(tag.center(), Align2::CENTER_CENTER, "UPDATE", FontId::proportional(10.5), th_blue_light());
        let wf = p.layout_no_wrap(from.to_string(), FontId::monospace(16.0), TEXT_DIM).size().x + 30.0;
        let wt = p.layout_no_wrap(to.to_string(), FontId::monospace(16.0), GREEN).size().x + 30.0;
        let gap = 56.0;
        let left = r.right() - 30.0 - (wf + gap + wt);
        chip(&p, Pos2::new(left + wf / 2.0, r.top() + 40.0), from, false, 1.0);
        chevrons(&p, Pos2::new(left + wf + gap / 2.0, r.top() + 40.0), t, grad(shown));
        chip(&p, Pos2::new(left + wf + gap + wt / 2.0, r.top() + 40.0), to, true, 0.8 + 0.2 * pulse);

        // Aufbau von oben nach unten
        let cw = (r.width() - 96.0).clamp(540.0, 720.0);
        let ch = 232.0;
        let total = ch + 22.0 + 36.0 + 34.0 + 60.0 + 52.0 + 18.0;
        let top = (r.top() + 78.0).max(r.top() + (r.height() - total) / 2.0 + 24.0);
        let card = Rect::from_min_size(Pos2::new(cx - cw / 2.0, top), Vec2::new(cw, ch));
        glass(&p, card);
        scene(&p, card, stage, since, tf, prog, err);

        // Titel und Untertitel
        let (title, sub) = STAGE_TITLES[(stage as usize).min(4)];
        let ia = ease_out(since / 0.45);
        let ty = card.bottom() + 22.0;
        if err.is_none() {
            p.text(Pos2::new(cx, ty + (1.0 - ia) * 10.0), Align2::CENTER_TOP, title, FontId::proportional(28.0), Color32::WHITE.gamma_multiply(ia));
            p.text(Pos2::new(cx, ty + 38.0 + (1.0 - ia) * 6.0), Align2::CENTER_TOP, sub, FontId::proportional(13.5), TEXT_DIM.gamma_multiply(ia));
        }

        // Fortschrittsbalken mit Prozentzahl
        let by = ty + 76.0;
        let bw = cw;
        let bar = Rect::from_min_size(Pos2::new(cx - bw / 2.0, by), Vec2::new(bw, 12.0));
        progress_bar(&p, bar, if err.is_some() { 0.0 } else { shown }, tf, 1.0);
        if err.is_none() {
            p.text(Pos2::new(bar.right(), by + 22.0), Align2::RIGHT_TOP, format!("{} %", (shown * 100.0).round() as i32), FontId::monospace(13.0), Color32::from_rgb(205, 216, 238));
            p.text(Pos2::new(bar.left(), by + 22.0), Align2::LEFT_TOP, "Gesamtfortschritt", FontId::proportional(12.5), TEXT_DIM);
        }

        // Zeitleiste der fünf Schritte
        let ty2 = by + 76.0;
        let step = ((bw - 40.0) / 4.0).clamp(80.0, 150.0);
        let x0 = cx - step * 2.0;
        let col = if err.is_some() { RED } else { grad(shown) };
        for i in 0..4 {
            let (a, b) = (x0 + step * i as f32 + 14.0, x0 + step * (i + 1) as f32 - 14.0);
            p.line_segment([Pos2::new(a, ty2), Pos2::new(b, ty2)], Stroke::new(2.0, Color32::from_white_alpha(22)));
            let fill = if (i as u8) < stage { 1.0 } else if i as u8 == stage { if stage == 1 { prog.pct / 100.0 } else { 0.5 } } else { 0.0 };
            if fill > 0.0 && err.is_none() {
                p.line_segment([Pos2::new(a, ty2), Pos2::new(a + (b - a) * fill.clamp(0.0, 1.0), ty2)], Stroke::new(2.4, col));
            }
        }
        for (i, name) in STAGES.iter().enumerate() {
            let i8 = i as u8;
            let ic = Pos2::new(x0 + step * i as f32, ty2);
            if err.is_some() && i8 == stage {
                p.circle_filled(ic, 13.0, RED.gamma_multiply(0.25));
                p.text(ic, Align2::CENTER_CENTER, "!", FontId::proportional(15.0), RED);
            } else if i8 < stage {
                p.circle_filled(ic, 13.0, GREEN.gamma_multiply(0.22));
                p.circle_stroke(ic, 13.0, Stroke::new(1.2, GREEN.gamma_multiply(0.6)));
                tick(&p, ic, 15.0, GREEN, 1.0);
            } else if i8 == stage {
                radial(&p, ic, 26.0, col, 0.30);
                p.circle_stroke(ic, 12.0, Stroke::new(2.0, Color32::from_white_alpha(25)));
                let a0 = (tf * 4.5) % (2.0 * PI);
                p.add(Shape::line(arc(ic, 12.0, a0, PI * 1.1), Stroke::new(2.8, col)));
            } else {
                p.circle_stroke(ic, 11.0, Stroke::new(1.5, Color32::from_white_alpha(28)));
            }
            let tc = if i8 == stage { Color32::WHITE } else if i8 < stage { Color32::from_rgb(170, 185, 215) } else { TEXT_DIM.gamma_multiply(0.7) };
            p.text(Pos2::new(ic.x, ic.y + 26.0), Align2::CENTER_TOP, *name, FontId::proportional(11.5), tc);
        }

        let fy = ty2 + 62.0;
        if err.is_some() {
            egui::Area::new(egui::Id::new("upd_back")).order(egui::Order::Foreground).fixed_pos(Pos2::new(cx - 100.0, fy - 4.0)).show(ctx, |ui| {
                if ui.add(big_button("Zurück", th_blue()).min_size(Vec2::new(200.0, 44.0))).clicked() {
                    back = true;
                }
            });
        } else {
            p.text(Pos2::new(cx, fy), Align2::CENTER_TOP, "Bitte lass das Fenster geöffnet – OVERLIME startet danach von selbst neu.", FontId::proportional(12.0), TEXT_DIM);
        }
    });
    back
}

/// Willkommens-Animation nach einem Update. Gibt true zurück, wenn sie beendet ist.
pub fn welcome_screen(ctx: &egui::Context, ts: f64, from: &str, to: &str) -> bool {
    let mut done = false;
    egui::CentralPanel::default().frame(egui::Frame::none().fill(th_bg())).show(ctx, |ui| {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        background_v2(&p, r, ts + 3.0, 1.0, false);
        let tsf = ts as f32;
        let a = |delay: f32| ease_out((tsf - delay) / 0.6);
        let slide = |delay: f32| (1.0 - a(delay)) * 12.0;

        // Karte
        let n = NEWS.len().min(5) as f32;
        let (cw, ch) = (500.0f32.min(r.width() - 40.0), 150.0 + 70.0 + 46.0 + 36.0 + n * 30.0 + 96.0);
        let card_a = a(0.0);
        let rc = Rect::from_center_size(Pos2::new(r.center().x, r.center().y + (1.0 - card_a) * 18.0), Vec2::new(cw, ch));
        let cx = rc.center().x;
        let top = rc.top();
        glass(&p, rc);

        // Ring mit Haken, ein einmaliger Lichtimpuls dahinter
        let c = Pos2::new(cx, top + 78.0);
        let ring_f = ease_out(tsf / 1.0);
        let pulse = ((tsf - 0.9) / 1.4).clamp(0.0, 1.0);
        radial(&p, c, 90.0, GREEN, 0.14 * ring_f);
        if pulse > 0.0 && pulse < 1.0 {
            p.circle_stroke(c, 40.0 + 60.0 * pulse, Stroke::new(1.5, GREEN.gamma_multiply(0.45 * (1.0 - pulse))));
        }
        p.circle_filled(c, 40.0, GREEN.gamma_multiply(0.10));
        ring(&p, c, 34.0, ring_f, 4.0, GREEN);
        tick(&p, c, 24.0, GREEN, ease_out((tsf - 0.8) / 0.5));

        // Wenige, ruhige Lichtfunken beim Erscheinen
        let fade = (1.0 - ((tsf - 1.0) / 2.2)).clamp(0.0, 1.0);
        if tsf > 0.8 && fade > 0.0 {
            for i in 0..26u32 {
                let ang = rnd(i, 1) * 2.0 * PI;
                let sp = 60.0 + rnd(i, 2) * 120.0;
                let lt = tsf - 0.8;
                let d = sp * (1.0 - (-lt * 1.6).exp());
                let pos = Pos2::new(c.x + ang.cos() * d, c.y + ang.sin() * d + lt * lt * 10.0);
                let col = [GREEN, th_blue_light(), Color32::WHITE][i as usize % 3];
                p.circle_filled(pos, 1.4 + rnd(i, 3) * 1.6, col.gamma_multiply(0.8 * fade));
            }
        }

        // Titel
        let a1 = a(1.1);
        p.text(Pos2::new(cx, top + 150.0 + slide(1.1)), Align2::CENTER_TOP, "Update installiert", FontId::proportional(28.0), Color32::WHITE.gamma_multiply(a1));
        let a2 = a(1.4);
        p.text(Pos2::new(cx, top + 188.0 + slide(1.4)), Align2::CENTER_TOP, format!("Du nutzt jetzt Version {to}"), FontId::proportional(14.5), TEXT_DIM.gamma_multiply(a2));

        // Versionsspruenge
        let a3 = a(1.7);
        let y = top + 240.0;
        let wf = p.layout_no_wrap(from.to_string(), FontId::monospace(17.0), TEXT_DIM).size().x + 30.0;
        let wt = p.layout_no_wrap(to.to_string(), FontId::monospace(17.0), GREEN).size().x + 30.0;
        let gap = 60.0;
        let left = cx - (wf + gap + wt) / 2.0;
        if from.is_empty() {
            chip(&p, Pos2::new(cx, y), to, true, a3);
        } else {
            chip(&p, Pos2::new(left + wf / 2.0, y), from, false, a3);
            chevrons(&p, Pos2::new(left + wf + gap / 2.0, y), ts, GREEN.gamma_multiply(a3));
            chip(&p, Pos2::new(left + wf + gap + wt / 2.0, y), to, true, a3);
        }

        // Trennlinie und Neuigkeiten
        let mut ny = y + 36.0;
        let a4 = a(2.1);
        p.line_segment([Pos2::new(rc.left() + 36.0, ny), Pos2::new(rc.right() - 36.0, ny)], Stroke::new(1.0, Color32::from_white_alpha((22.0 * a4) as u8)));
        ny += 16.0;
        p.text(Pos2::new(rc.left() + 40.0, ny), Align2::LEFT_TOP, "NEU IN DIESER VERSION", FontId::proportional(11.0), th_blue_light().gamma_multiply(a4));
        ny += 26.0;
        for (i, line) in NEWS.iter().take(5).enumerate() {
            let d = 2.4 + i as f32 * 0.3;
            let al = a(d);
            let yy = ny + slide(d);
            p.circle_filled(Pos2::new(rc.left() + 48.0, yy + 10.0), 9.0, GREEN.gamma_multiply(0.16 * al));
            tick(&p, Pos2::new(rc.left() + 48.0, yy + 10.0), 9.0, GREEN.gamma_multiply(al), al);
            p.text(Pos2::new(rc.left() + 68.0, yy), Align2::LEFT_TOP, *line, FontId::proportional(14.5), Color32::from_rgb(214, 223, 242).gamma_multiply(al));
            ny += 30.0;
        }

        // Weiter-Knopf
        let ab = a(3.6);
        if ab > 0.01 {
            egui::Area::new(egui::Id::new("welcome_btn")).order(egui::Order::Foreground).fixed_pos(Pos2::new(cx - 110.0, rc.bottom() - 70.0)).show(ctx, |ui| {
                ui.set_opacity(ab);
                if ui.add(big_button("Los geht's", th_blue()).min_size(Vec2::new(220.0, 46.0))).clicked() {
                    done = true;
                }
            });
        }
        // Nach 9 Sekunden von selbst weiter; Esc, Eingabe oder Leertaste beenden sofort
        if ts > 9.0 || (ts > 0.4 && ctx.input(|i| i.key_pressed(Key::Escape) || i.key_pressed(Key::Enter) || i.key_pressed(Key::Space))) {
            done = true;
        }
    });
    done
}

// ---------------------------------------------------------------- Intro (nur lokal, mit --intro)

fn ease_back(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

fn sm(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Startbild: Lichtstreifen laufen zusammen, das Symbol zeichnet sich, ein Lichtring bricht aus,
/// der Schriftzug erscheint Buchstabe für Buchstabe. Gibt true zurück, wenn es zu Ende ist (oder übersprungen wurde).
pub fn intro_screen(ctx: &egui::Context, t: f32) -> bool {
    let total = 4.2;
    let screen = ctx.screen_rect();
    let mut skip = ctx.input(|i| i.key_pressed(Key::Escape) || i.key_pressed(Key::Enter) || i.key_pressed(Key::Space));
    // Das Symbol als Bild (einmal anlegen)
    let tex = ctx.data_mut(|d| d.get_temp::<egui::TextureHandle>(egui::Id::new("intro_icon")));
    let tex = match tex {
        Some(t) => t,
        None => {
            let ic = winui::app_icon();
            let img = egui::ColorImage::from_rgba_unmultiplied([ic.width as usize, ic.height as usize], &ic.rgba);
            let t = ctx.load_texture("intro_icon", img, TextureOptions::LINEAR);
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("intro_icon"), t.clone()));
            t
        }
    };
    egui::Area::new(egui::Id::new("intro")).order(egui::Order::Tooltip).fixed_pos(screen.min).show(ctx, |ui| {
        let (rect, resp) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
        if resp.clicked() {
            skip = true;
        }
        let p = ui.painter().clone();
        let ga = 1.0 - sm((t - 3.5) / 0.7); // Gesamt-Ausblendung
        if ga <= 0.0 {
            return;
        }
        let c = Pos2::new(rect.center().x, rect.center().y - 36.0);
        let sz = (rect.height().min(rect.width() * 0.6) * 0.30).clamp(110.0, 190.0);

        // Hintergrund mit sanftem Licht
        p.rect_filled(rect, Rounding::ZERO, Color32::from_rgb(5, 7, 14).gamma_multiply(ga));
        let glow = sm(t / 1.6);
        radial(&p, c, rect.width().min(900.0) * (0.35 + 0.25 * glow), th_blue(), 0.22 * glow * ga);
        radial(&p, c + Vec2::new(rect.width() * 0.18, rect.height() * 0.12), 420.0, Color32::from_rgb(120, 70, 240), 0.14 * glow * ga);

        // Lichtstreifen von links und rechts, die zur Mitte laufen
        for i in 0..11u32 {
            let off = (i as f32 - 5.0) * 16.0 + (rnd(i, 3) - 0.5) * 10.0;
            let y = c.y + off;
            let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
            let delay = rnd(i, 5) * 0.35;
            let k = ease_out((t - delay) / 0.95);
            let edge = if dir > 0.0 { rect.left() - 40.0 } else { rect.right() + 40.0 };
            let head = edge + (c.x - edge) * k;
            let len = 260.0 + rnd(i, 7) * 220.0;
            let tail = head - dir * len * (1.0 - 0.6 * k);
            let alpha = (1.0 - sm((t - 0.85 - delay) / 0.3)) * sm((t - delay) / 0.12) * ga;
            if alpha > 0.01 {
                let steps = 14;
                for s in 0..steps {
                    let f0 = s as f32 / steps as f32;
                    let f1 = (s + 1) as f32 / steps as f32;
                    let x0 = tail + (head - tail) * f0;
                    let x1 = tail + (head - tail) * f1;
                    let col = if i % 3 == 0 { Color32::from_rgb(150, 200, 255) } else { th_blue_light() };
                    p.line_segment([Pos2::new(x0, y), Pos2::new(x1, y)], Stroke::new(1.5 + 1.5 * f1, col.gamma_multiply(alpha * f1 * f1)));
                }
                p.circle_filled(Pos2::new(head, y), 2.5, Color32::WHITE.gamma_multiply(alpha));
            }
        }

        // Dreieck zeichnet sich als Linie
        let s = sz / 256.0;
        let tri = [Vec2::new(-28.0, -58.0) * s, Vec2::new(-28.0, 58.0) * s, Vec2::new(68.0, 0.0) * s];
        let tri_k = ease_out((t - 0.35) / 0.8);
        let icon_a = sm((t - 1.05) / 0.45);
        if tri_k > 0.0 && icon_a < 1.0 {
            let pts = [c + tri[0], c + tri[1], c + tri[2], c + tri[0]];
            let total_len: f32 = (0..3).map(|i| (pts[i + 1] - pts[i]).length()).sum();
            let mut remain = total_len * tri_k;
            let mut line = vec![pts[0]];
            for i in 0..3 {
                let seg = pts[i + 1] - pts[i];
                let l = seg.length();
                if remain >= l {
                    line.push(pts[i + 1]);
                    remain -= l;
                } else {
                    line.push(pts[i] + seg * (remain / l));
                    break;
                }
            }
            let a = (1.0 - icon_a) * ga;
            for (w, al) in [(14.0, 0.10), (8.0, 0.22), (3.5, 1.0)] {
                p.add(Shape::line(line.clone(), Stroke::new(w, Color32::from_rgb(160, 205, 255).gamma_multiply(al * a))));
            }
            if let Some(tip) = line.last() {
                p.circle_filled(*tip, 7.0, Color32::from_rgb(160, 205, 255).gamma_multiply(0.35 * a));
                p.circle_filled(*tip, 3.2, Color32::WHITE.gamma_multiply(a));
            }
        }

        // Lichtring beim Erscheinen
        let wave = (t - 1.05) / 1.1;
        if (0.0..1.0).contains(&wave) {
            let r = sz * 0.4 + sz * 3.2 * ease_out(wave);
            let a = (1.0 - wave).powi(2) * ga;
            p.circle_stroke(c, r, Stroke::new(2.5, Color32::from_rgb(140, 190, 255).gamma_multiply(0.7 * a)));
            p.circle_stroke(c, r * 0.82, Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.25 * a)));
        }

        // Symbol: erscheint mit leichtem Überschwingen, mit Glanzstreifen
        if icon_a > 0.0 {
            let k = 0.62 + 0.38 * ease_back((t - 1.05) / 0.65);
            let half = sz * 0.5 * k;
            radial(&p, c, sz * 1.25 * k, Color32::from_rgb(90, 130, 255), 0.35 * icon_a * ga);
            let ir = Rect::from_center_size(c, Vec2::splat(half * 2.0));
            p.image(tex.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE.gamma_multiply(icon_a * ga));
            // Glanzstreifen, der einmal über das Symbol wandert
            let sh = (t - 1.45) / 0.7;
            if (0.0..1.0).contains(&sh) {
                let sx = ir.left() - 30.0 + (ir.width() + 60.0) * sh;
                let band = 26.0;
                let mut m = egui::epaint::Mesh::default();
                let top = Color32::from_white_alpha((70.0 * (1.0 - (sh - 0.5).abs() * 2.0) * ga) as u8);
                let clear = Color32::from_white_alpha(0);
                let (x0, x1, x2) = (sx - band, sx, sx + band);
                let slant = 18.0;
                m.colored_vertex(Pos2::new(x0 + slant, ir.top() + 4.0), clear);
                m.colored_vertex(Pos2::new(x1 + slant, ir.top() + 4.0), top);
                m.colored_vertex(Pos2::new(x2 + slant, ir.top() + 4.0), clear);
                m.colored_vertex(Pos2::new(x0 - slant, ir.bottom() - 4.0), clear);
                m.colored_vertex(Pos2::new(x1 - slant, ir.bottom() - 4.0), top);
                m.colored_vertex(Pos2::new(x2 - slant, ir.bottom() - 4.0), clear);
                m.add_triangle(0, 1, 3);
                m.add_triangle(1, 4, 3);
                m.add_triangle(1, 2, 4);
                m.add_triangle(2, 5, 4);
                // auf das Symbol begrenzen
                p.with_clip_rect(ir.shrink(sz * 0.04)).add(Shape::mesh(m));
            }
        }

        // Funken
        let st = t - 1.05;
        if st > 0.0 {
            for i in 0..46u32 {
                let ang = rnd(i, 1) * 2.0 * PI;
                let spd = 120.0 + rnd(i, 2) * 360.0;
                let life = 0.9 + rnd(i, 3) * 0.9;
                if st > life {
                    continue;
                }
                let f = st / life;
                let d = spd * (1.0 - (-st * 2.4).exp()) / 2.4 * 2.0;
                let pos = c + Vec2::new(ang.cos(), ang.sin()) * (sz * 0.35 + d) + Vec2::new(0.0, 18.0 * st * st);
                let col = [th_blue_light(), Color32::WHITE, Color32::from_rgb(170, 120, 255)][(i % 3) as usize];
                p.circle_filled(pos, 1.2 + rnd(i, 4) * 2.2, col.gamma_multiply((1.0 - f) * 0.9 * ga));
            }
        }

        // Schriftzug Buchstabe für Buchstabe
        let word = "OVERLIME";
        let fs = (sz * 0.34).clamp(36.0, 64.0);
        let track = fs * 0.28;
        let font = FontId::proportional(fs);
        let widths: Vec<f32> = word.chars().map(|ch| p.layout_no_wrap(ch.to_string(), font.clone(), Color32::WHITE).size().x).collect();
        let total_w: f32 = widths.iter().sum::<f32>() + track * (word.len() as f32 - 1.0);
        let wy = c.y + sz * 0.5 + fs * 1.15;
        let mut x = c.x - total_w / 2.0;
        for (i, ch) in word.chars().enumerate() {
            let k = ease_out((t - 1.75 - i as f32 * 0.07) / 0.55);
            if k > 0.0 {
                let yo = (1.0 - k) * 26.0;
                let col = Color32::WHITE.gamma_multiply(k * ga);
                p.text(Pos2::new(x, wy + yo), Align2::LEFT_CENTER, ch.to_string(), font.clone(), col);
                // kurzer Lichtblitz auf dem Buchstaben
                let fl = (1.0 - ((t - 1.75 - i as f32 * 0.07 - 0.35) / 0.25).abs()).clamp(0.0, 1.0);
                if fl > 0.0 {
                    p.text(Pos2::new(x, wy + yo), Align2::LEFT_CENTER, ch.to_string(), font.clone(), th_blue_light().gamma_multiply(fl * 0.8 * ga));
                }
            }
            x += widths[i] + track;
        }
        // Linie und Untertitel
        let lk = ease_out((t - 2.45) / 0.6);
        if lk > 0.0 {
            let ly = wy + fs * 0.78;
            let hw = total_w * 0.5 * lk;
            p.line_segment([Pos2::new(c.x - hw, ly), Pos2::new(c.x + hw, ly)], Stroke::new(1.5, th_blue().gamma_multiply(0.9 * ga)));
            let sub = "R E M O T E   P L A Y";
            p.text(Pos2::new(c.x, ly + 22.0), Align2::CENTER_CENTER, sub, FontId::proportional((fs * 0.27).max(12.0)), th_blue_light().gamma_multiply(ease_out((t - 2.6) / 0.6) * ga));
        }
        p.text(Pos2::new(rect.right() - 16.0, rect.bottom() - 14.0), Align2::RIGHT_CENTER, "Klick oder Esc überspringt", FontId::proportional(11.0), Color32::from_white_alpha((46.0 * ga) as u8));
    });
    t > total || skip
}
