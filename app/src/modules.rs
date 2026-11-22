//! Module: Funktionen von OVERLIME, die man einzeln ein- und ausschalten kann. Alle Module sind fester Teil der App
//! und von uns gebaut (kein Nachladen von fremdem Code).

use super::*;

pub struct ModuleDef {
    pub id: &'static str,
    pub icon: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    /// Einstellungs-Kategorie, die über „Einstellungen“ geöffnet wird (0 = Alle)
    pub cat: usize,
}

pub const MODULES: [ModuleDef; 6] = [
    ModuleDef { id: "gallery", icon: ic::IMAGES, title: "Galerie", desc: "Aufnahmen und Screenshots ansehen, Videos abspielen, Dateien teilen.", cat: 1 },
    ModuleDef { id: "friends", icon: ic::USERS_THREE, title: "Freunde & Sprache", desc: "Eigener Sprachchat mit Freundesliste. Sprich mit Freunden in Sprachräumen, auch während du spielst.", cat: 1 },
    ModuleDef { id: "voice", icon: ic::MICROPHONE, title: "Sprache (Mikrofon)", desc: "Sprich über dein Mikrofon mit deiner Party auf der Konsole. Der Ton deiner Party kommt wie bisher im Stream an.", cat: 1 },
    ModuleDef { id: "tuning", icon: ic::SLIDERS_HORIZONTAL, title: "Controller-Feinabstimmung", desc: "Totzone, Empfindlichkeit und Kurve der Sticks, dazu die Zielhilfe beim Halten von L2.", cat: 2 },
    ModuleDef { id: "intro", icon: ic::SPARKLE, title: "Start-Animation", desc: "Kurzes Intro beim Öffnen von OVERLIME.", cat: 3 },
    ModuleDef { id: "hud", icon: ic::GAUGE, title: "Statusanzeige", desc: "Ping, Bildrate, Datenrate und mehr als Einblendung im Stream.", cat: 3 },
];

impl Settings {
    pub fn module_on(&self, id: &str) -> bool {
        match id {
            "intro" => self.intro,
            _ => !self.modules_off.split(',').any(|m| m == id),
        }
    }

    pub fn set_module(&mut self, id: &str, on: bool) {
        if id == "intro" {
            self.intro = on;
            return;
        }
        let mut v: Vec<&str> = self.modules_off.split(',').filter(|m| !m.is_empty() && *m != id).collect();
        if !on {
            v.push(id);
        }
        self.modules_off = v.join(",");
    }
}

impl App {
    pub fn modules_page(&mut self, ui: &mut egui::Ui, t: f64) {
        ui.label(RichText::new("Module").size(28.0));
        ui.label(RichText::new("Jede Funktion von OVERLIME als Baustein. Schalte aus, was du nicht brauchst. Alle Module sind von uns gebaut und Teil der App.").color(TEXT_DIM).size(13.0));
        ui.add_space(12.0);
        let mut open_cat: Option<usize> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let gap = 14.0;
            let avail = ui.available_width();
            let cols = (((avail + gap) / (330.0 + gap)).floor() as usize).max(1);
            let cw = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
            let ch = 150.0;
            let x0 = ui.cursor().left();
            let y0 = ui.cursor().top();
            for (i, m) in MODULES.iter().enumerate() {
                let (col, row) = (i % cols, i / cols);
                let rect = Rect::from_min_size(Pos2::new(x0 + col as f32 * (cw + gap), y0 + row as f32 * (ch + gap)), Vec2::new(cw, ch));
                let on = self.settings.module_on(m.id);
                let a_on = ui.ctx().animate_bool_with_time(egui::Id::new(("mod_on", i)), on, 0.2);
                let resp = ui.interact(rect, egui::Id::new(("mod_card", i)), egui::Sense::hover());
                let hv = ui.ctx().animate_bool_with_time(egui::Id::new(("mod_hv", i)), resp.hovered(), 0.14);
                let ent = fade(t, self.page_since + 0.04 * i as f64, 0.5);
                let p = ui.painter();
                p.rect_filled(rect.translate(Vec2::new(0.0, -2.0 * hv)), Rounding::same(18.0), th_card().gamma_multiply(ent));
                p.rect_stroke(rect.translate(Vec2::new(0.0, -2.0 * hv)), Rounding::same(18.0), Stroke::new(1.0, Color32::from_white_alpha((14.0 + 20.0 * a_on + 14.0 * hv) as u8)));
                // Symbolkachel
                let tile_r = Rect::from_min_size(rect.min + Vec2::new(18.0, 18.0), Vec2::splat(52.0));
                let tile_col = Color32::from_white_alpha(14).lerp_to_gamma(th_blue(), a_on * 0.9);
                p.rect_filled(tile_r, Rounding::same(14.0), tile_col);
                p.text(tile_r.center(), Align2::CENTER_CENTER, m.icon, FontId::proportional(26.0), Color32::WHITE.gamma_multiply(0.6 + 0.4 * a_on));
                p.text(rect.min + Vec2::new(84.0, 30.0), Align2::LEFT_CENTER, lang::tr(m.title), FontId::proportional(16.5), Color32::WHITE);
                let (st_txt, st_col) = if on { ("An", GREEN) } else { ("Aus", TEXT_DIM) };
                p.text(rect.min + Vec2::new(84.0, 52.0), Align2::LEFT_CENTER, lang::tr(st_txt), FontId::proportional(12.5), st_col);
                // Beschreibung (umgebrochen)
                let g = p.layout(lang::tr(m.desc), FontId::proportional(12.5), TEXT_DIM, rect.width() - 36.0);
                p.galley(rect.min + Vec2::new(18.0, 82.0), g, TEXT_DIM);
                // Schalter oben rechts
                let sw = Rect::from_min_size(Pos2::new(rect.right() - 18.0 - 46.0, rect.top() + 22.0), Vec2::new(46.0, 26.0));
                let r2 = ui.interact(sw.expand(4.0), egui::Id::new(("mod_sw", i)), egui::Sense::click());
                let p = ui.painter();
                p.rect_filled(sw, Rounding::same(13.0), Color32::from_white_alpha(30).lerp_to_gamma(th_blue(), a_on));
                p.circle_filled(Pos2::new(sw.left() + 13.0 + (sw.width() - 26.0) * a_on, sw.center().y), 9.5, Color32::WHITE);
                if r2.clicked() {
                    self.settings.set_module(m.id, !on);
                }
                // Einstellungen
                let br = Rect::from_min_size(Pos2::new(rect.right() - 18.0 - 110.0, rect.bottom() - 14.0 - 28.0), Vec2::new(110.0, 28.0));
                let r3 = ui.interact(br, egui::Id::new(("mod_set", i)), egui::Sense::click());
                let p = ui.painter();
                p.rect_filled(br, Rounding::same(8.0), Color32::from_white_alpha(if r3.hovered() { 26 } else { 14 }));
                p.text(br.center(), Align2::CENTER_CENTER, format!("{}  {}", ic::GEAR_SIX, lang::tr("Einstellungen")), FontId::proportional(12.5), Color32::WHITE);
                if r3.clicked() {
                    open_cat = Some(m.cat);
                }
            }
            let rows = (MODULES.len() + cols - 1) / cols;
            ui.allocate_space(Vec2::new(avail, rows as f32 * (ch + gap)));
        });
        if let Some(c) = open_cat {
            self.page = Page::Settings;
            self.set_cat = c;
            self.page_since = t;
        }
    }
}
