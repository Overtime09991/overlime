//! Update-Bildschirm und Willkommens-Animation nach dem Update.
use super::*;
use std::f32::consts::PI;

/// Neuigkeiten, die nach dem Update gezeigt werden.
pub const NEWS: &[&str] = &[
    "Updates während der App offen ist: nur Hinweis, kein Überraschen",
    "Während eines Spiels wird nie installiert",
    "Update-Hinweis auch im Stream-Menü",
    "Update-Bereich, Controller-Diagnose und mehr",
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

fn tick(p: &egui::Painter, c: Pos2, s: f32, col: Color32, frac: f32) {
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

/// Zeigt die Update-Animation. Gibt true zurück, wenn bei einem Fehler auf "Zurück" geklickt wurde.
pub fn update_screen(ctx: &egui::Context, t: f64, stage: u8, pct: f32, shown: f32, from: &str, to: &str, err: Option<&str>) -> bool {
    let mut back = false;
    egui::CentralPanel::default().frame(egui::Frame::none().fill(th_bg())).show(ctx, |ui| {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        background(&p, r, t);
        let cx = r.center().x;
        let top = r.top() + (r.height() * 0.5 - 285.0).max(16.0);
        let pulse = 0.5 + 0.5 * (t as f32 * 2.0).sin();

        p.text(Pos2::new(cx, top), Align2::CENTER_TOP, "OVERLIME", FontId::proportional(40.0), Color32::WHITE);
        p.text(Pos2::new(cx, top + 48.0), Align2::CENTER_TOP, "UPDATE", FontId::proportional(13.0), th_blue_light());

        let c = Pos2::new(cx, top + 170.0);
        let col = if err.is_some() { RED } else { th_blue_light() };
        p.circle_filled(c, 100.0 + 6.0 * pulse, col.gamma_multiply(0.07 + 0.05 * pulse));
        p.circle_filled(c, 82.0, col.gamma_multiply(0.07));
        ring(&p, c, 76.0, shown, 9.0, col);
        let big = if err.is_some() { "!".to_string() } else { format!("{}", (shown * 100.0).round() as i32) };
        p.text(c + Vec2::new(0.0, -4.0), Align2::CENTER_CENTER, big, FontId::proportional(46.0), Color32::WHITE);
        if err.is_none() {
            p.text(c + Vec2::new(0.0, 32.0), Align2::CENTER_CENTER, "PROZENT", FontId::proportional(11.0), TEXT_DIM);
        }

        // Versions-Chips
        let y = top + 300.0;
        let wf = p.layout_no_wrap(from.to_string(), FontId::monospace(17.0), TEXT_DIM).size().x + 30.0;
        let wt = p.layout_no_wrap(to.to_string(), FontId::monospace(17.0), GREEN).size().x + 30.0;
        let gap = 70.0;
        let total = wf + gap + wt;
        let left = cx - total / 2.0;
        chip(&p, Pos2::new(left + wf / 2.0, y), from, false, 1.0);
        chevrons(&p, Pos2::new(left + wf + gap / 2.0, y), t, th_blue_light());
        let glow = 0.75 + 0.25 * pulse;
        chip(&p, Pos2::new(left + wf + gap + wt / 2.0, y), to, true, glow);

        // Stufenliste
        let list_x = cx - 170.0;
        let mut ly = y + 48.0;
        for (i, name) in STAGES.iter().enumerate() {
            let i = i as u8;
            let ic = Pos2::new(list_x + 11.0, ly + 11.0);
            if err.is_some() && i == stage {
                p.circle_filled(ic, 11.0, RED.gamma_multiply(0.25));
                p.text(ic, Align2::CENTER_CENTER, "!", FontId::proportional(14.0), RED);
            } else if i < stage {
                p.circle_filled(ic, 11.0, GREEN.gamma_multiply(0.22));
                tick(&p, ic, 14.0, GREEN, 1.0);
            } else if i == stage {
                p.circle_stroke(ic, 10.0, Stroke::new(2.0, Color32::from_white_alpha(25)));
                let a0 = (t as f32 * 4.5) % (2.0 * PI);
                p.add(Shape::line(arc(ic, 10.0, a0, PI * 1.1), Stroke::new(2.6, th_blue_light())));
            } else {
                p.circle_stroke(ic, 10.0, Stroke::new(1.5, Color32::from_white_alpha(28)));
            }
            let (tc, label) = if i == stage && i == 1 && err.is_none() {
                (Color32::WHITE, format!("{name}   {}%", pct.round() as i32))
            } else if i == stage {
                (Color32::WHITE, name.to_string())
            } else if i < stage {
                (Color32::from_rgb(170, 185, 215), name.to_string())
            } else {
                (TEXT_DIM.gamma_multiply(0.7), name.to_string())
            };
            p.text(Pos2::new(list_x + 34.0, ic.y), Align2::LEFT_CENTER, label, FontId::proportional(15.0), tc);
            ly += 34.0;
        }

        if let Some(e) = err {
            p.text(Pos2::new(cx, ly + 12.0), Align2::CENTER_TOP, e, FontId::proportional(13.0), RED);
            egui::Area::new(egui::Id::new("upd_back")).order(egui::Order::Foreground).fixed_pos(Pos2::new(cx - 100.0, ly + 42.0)).show(ctx, |ui| {
                if ui.add(big_button("Zurück", th_blue()).min_size(Vec2::new(200.0, 44.0))).clicked() {
                    back = true;
                }
            });
        } else {
            p.text(Pos2::new(cx, ly + 14.0), Align2::CENTER_TOP, "Bitte lass das Fenster geöffnet – OVERLIME startet danach von selbst neu.", FontId::proportional(12.0), TEXT_DIM);
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
        background(&p, r, ts + 3.0);
        let cx = r.center().x;
        let top = r.top() + (r.height() * 0.5 - 290.0).max(16.0);
        let tsf = ts as f32;

        // Konfetti
        let palette = [th_blue_light(), GREEN, YELLOW, Color32::from_rgb(170, 120, 255), Color32::WHITE, Color32::from_rgb(255, 120, 150)];
        let fade = (1.0 - ((tsf - 6.5) / 2.5).clamp(0.0, 1.0)).max(0.0);
        if tsf > 0.5 && fade > 0.0 {
            let ct = tsf - 0.5;
            for i in 0..110u32 {
                let delay = rnd(i, 7) * 1.2;
                if ct < delay {
                    continue;
                }
                let lt = ct - delay;
                let x0 = r.left() + rnd(i, 1) * r.width();
                let sway = (lt * (1.2 + rnd(i, 8) * 1.5) + rnd(i, 9) * 6.0).sin() * (14.0 + rnd(i, 10) * 26.0);
                let y = r.top() - 20.0 + lt * (110.0 + rnd(i, 2) * 190.0);
                if y > r.bottom() + 20.0 {
                    continue;
                }
                let ang = lt * (2.0 + rnd(i, 11) * 5.0) + rnd(i, 12) * 6.0;
                let (w, h) = (5.0 + rnd(i, 13) * 6.0, 3.0 + rnd(i, 14) * 4.0);
                let col = palette[(i as usize) % palette.len()].gamma_multiply(0.9 * fade);
                let (s, c) = (ang.sin(), ang.cos());
                let pts: Vec<Pos2> = [(-w, -h), (w, -h), (w, h), (-w, h)]
                    .iter()
                    .map(|(px, py)| Pos2::new(x0 + sway + px * c - py * s, y + px * s + py * c))
                    .collect();
                p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
            }
        }

        // Erfolgs-Ring
        let c = Pos2::new(cx, top + 110.0);
        let ring_f = ease_out(tsf / 1.1);
        let pop = 1.0 + 0.06 * ((tsf - 1.0).max(0.0) * 6.0).sin() * (-(tsf - 1.0).max(0.0) * 3.0).exp();
        p.circle_filled(c, 96.0 * pop, GREEN.gamma_multiply(0.07 + 0.05 * ring_f));
        ring(&p, c, 64.0 * pop, ring_f, 9.0, GREEN);
        tick(&p, c, 44.0 * pop, GREEN, ease_out((tsf - 0.9) / 0.6));

        let a = |delay: f32| ease_out((tsf - delay) / 0.6);
        let slide = |delay: f32| (1.0 - a(delay)) * 14.0;

        let a1 = a(1.2);
        p.text(Pos2::new(cx, top + 210.0 + slide(1.2)), Align2::CENTER_TOP, "Update installiert", FontId::proportional(34.0), Color32::WHITE.gamma_multiply(a1));
        let a2 = a(1.6);
        p.text(Pos2::new(cx, top + 256.0 + slide(1.6)), Align2::CENTER_TOP, format!("Du nutzt jetzt Version {to}"), FontId::proportional(16.0), Color32::from_rgb(190, 205, 235).gamma_multiply(a2));

        let a3 = a(2.0);
        let y = top + 312.0;
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

        // Neuigkeiten
        let mut ny = y + 52.0;
        p.text(Pos2::new(cx, ny), Align2::CENTER_TOP, "NEU IN DIESER VERSION", FontId::proportional(11.5), th_blue_light().gamma_multiply(a(2.4)));
        ny += 24.0;
        for (i, line) in NEWS.iter().enumerate() {
            let d = 2.6 + i as f32 * 0.35;
            let al = a(d);
            p.circle_filled(Pos2::new(cx - 190.0, ny + 9.0 + slide(d)), 3.5, GREEN.gamma_multiply(al));
            p.text(Pos2::new(cx - 176.0, ny + slide(d)), Align2::LEFT_TOP, *line, FontId::proportional(14.5), Color32::from_rgb(210, 220, 240).gamma_multiply(al));
            ny += 26.0;
        }

        // Weiter-Knopf
        let ab = a(4.2);
        if ab > 0.01 {
            egui::Area::new(egui::Id::new("welcome_btn")).order(egui::Order::Foreground).fixed_pos(Pos2::new(cx - 110.0, ny + 18.0)).show(ctx, |ui| {
                ui.set_opacity(ab);
                if ui.add(big_button("Los geht's", th_blue()).min_size(Vec2::new(220.0, 46.0))).clicked() {
                    done = true;
                }
            });
        }
        // Nach 8 Sekunden von selbst weiter; Esc, Eingabe oder Leertaste beenden sofort
        if ts > 8.0 || (ts > 0.4 && ctx.input(|i| i.key_pressed(Key::Escape) || i.key_pressed(Key::Enter) || i.key_pressed(Key::Space))) {
            done = true;
        }
    });
    done
}
