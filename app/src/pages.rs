//! Übersichts- und Hilfeseite.
use super::*;

pub fn tile(ui: &mut egui::Ui, w: f32, title: &str, value: &str, sub: &str, color: Color32, ent: f32) {
    let (rect0, resp) = ui.allocate_exact_size(Vec2::new(w, 104.0), egui::Sense::hover());
    let h = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.15);
    let rect = rect0.translate(Vec2::new(0.0, (1.0 - ent) * 18.0 - 3.0 * h));
    let p = ui.painter().with_clip_rect(rect0.expand(24.0));
    let al = ent;
    // Schatten beim Heben
    if h > 0.01 {
        p.rect_filled(rect.translate(Vec2::new(0.0, 5.0)).expand(1.0), Rounding::same(17.0), Color32::from_black_alpha((70.0 * h * al) as u8));
    }
    p.rect_filled(rect, Rounding::same(16.0), th_card().lerp_to_gamma(th_card_hover(), h).gamma_multiply(al));
    if h > 0.01 {
        p.rect_stroke(rect, Rounding::same(16.0), Stroke::new(1.2, color.gamma_multiply(0.55 * h * al)));
    }
    let bar_h = 72.0 + 8.0 * h;
    p.rect_filled(Rect::from_min_size(rect.min + Vec2::new(0.0, (104.0 - bar_h) / 2.0), Vec2::new(4.0, bar_h)), Rounding::same(2.0), color.gamma_multiply(al));
    p.circle_filled(rect.right_top() + Vec2::new(-10.0, 10.0), 46.0 + 6.0 * h, color.gamma_multiply((0.10 + 0.06 * h) * al));
    p.text(rect.min + Vec2::new(20.0, 14.0), Align2::LEFT_TOP, title, FontId::proportional(12.5), TEXT_DIM.gamma_multiply(al));
    let vsize = if value.chars().count() > 9 { 21.0 } else { 28.0 };
    p.text(rect.min + Vec2::new(20.0, 36.0 + (28.0 - vsize) / 2.0), Align2::LEFT_TOP, value, FontId::proportional(vsize), Color32::WHITE.gamma_multiply(al));
    p.text(rect.min + Vec2::new(20.0, 76.0), Align2::LEFT_TOP, sub, FontId::proportional(12.0), color.gamma_multiply(al));
}

pub fn check_row(ui: &mut egui::Ui, done: bool, title: &str, sub: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(26.0), egui::Sense::hover());
        let c = r.center();
        if done {
            ui.painter().circle_filled(c, 12.0, GREEN.gamma_multiply(0.25));
            ui.painter().line_segment([c + Vec2::new(-5.0, 0.0), c + Vec2::new(-1.5, 4.0)], Stroke::new(2.4, GREEN));
            ui.painter().line_segment([c + Vec2::new(-1.5, 4.0), c + Vec2::new(5.5, -4.5)], Stroke::new(2.4, GREEN));
        } else {
            ui.painter().circle_stroke(c, 11.0, Stroke::new(1.6, TEXT_DIM));
        }
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(14.5).strong().color(if done { Color32::WHITE } else { Color32::from_rgb(200, 210, 235) }));
            ui.label(RichText::new(sub).size(12.5).color(TEXT_DIM));
        });
    });
}

impl App {
    pub fn news_page(&mut self, ui: &mut egui::Ui, t: f64) {
        let _ = t;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Neuigkeiten").size(26.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.news_loading {
                    ui.spinner();
                } else if ui.add(egui::Button::new(format!("{} Aktualisieren", ic::ARROWS_CLOCKWISE)).fill(th_card_hover()).rounding(10.0)).clicked() {
                    self.load_news();
                }
            });
        });
        ui.label(RichText::new("Das ist neu in OVERLIME, Version für Version.").color(TEXT_DIM).size(13.0));
        ui.add_space(8.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut list = self.news.clone();
            if list.is_empty() {
                // Rückfall (offline): Neuerungen der installierten Version
                list.push(update::NewsItem { version: VERSION.to_string(), date: String::new(), items: splash::NEWS.iter().map(|s| s.to_string()).collect() });
            }
            if !self.news_err.is_empty() {
                ui.label(RichText::new("Neuigkeiten konnten nicht geladen werden (offline?).").color(YELLOW).size(12.5));
                ui.add_space(4.0);
            }
            let total = list.len();
            let shown = self.news_shown.max(1).min(total);
            let mut more = false;
            for (idx, n) in list.iter().enumerate().take(shown) {
                let newer = update::is_newer(&n.version, VERSION);
                let current = n.version == VERSION;
                let first = idx == 0;
                egui::Frame::none()
                    .fill(if first { th_blue().gamma_multiply(0.14) } else { th_card() })
                    .rounding(16.0)
                    .stroke(Stroke::new(1.0, if first { th_blue().gamma_multiply(0.45) } else { Color32::from_white_alpha(10) }))
                    .inner_margin(egui::Margin::same(18.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("Version {}", n.version)).size(18.0).strong());
                            if current {
                                pill(ui, "Installiert", GREEN);
                            } else if newer {
                                pill(ui, "Update verfügbar", YELLOW);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if !n.date.is_empty() {
                                    ui.label(RichText::new(&n.date).color(TEXT_DIM).size(12.5));
                                }
                            });
                        });
                        ui.add_space(6.0);
                        for it in &n.items {
                            ui.horizontal_top(|ui| {
                                let (r, _) = ui.allocate_exact_size(Vec2::new(10.0, 18.0), egui::Sense::hover());
                                ui.painter().circle_filled(r.center(), 3.0, th_blue_light());
                                ui.add(egui::Label::new(RichText::new(it).size(13.5)).wrap());
                            });
                        }
                    });
                ui.add_space(8.0);
            }
            if total > shown {
                ui.vertical_centered(|ui| {
                    let rest = total - shown;
                    let label = if shown == 1 { format!("Ältere Versionen anzeigen ({rest})") } else { format!("Noch mehr anzeigen ({rest} weitere)") };
                    if ui.add(egui::Button::new(label).fill(th_card_hover()).rounding(10.0).min_size(Vec2::new(260.0, 38.0))).clicked() {
                        more = true;
                    }
                });
            } else if total > 1 {
                ui.vertical_centered(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Weniger anzeigen").color(TEXT_DIM)).frame(false)).clicked() {
                        self.news_shown = 1;
                    }
                });
            }
            if more {
                self.news_shown = shown + 3;
            }
        });
    }
}

impl App {
    pub fn home_page(&mut self, ui: &mut egui::Ui, t: f64) {
        let ready: Vec<Console> = self.consoles.iter().filter(|c| c.state == State::Ready).cloned().collect();
        let found = self.consoles.len();
        let paired_found = self.consoles.iter().filter(|c| self.paired_for(c).is_some()).count();
        let have_psn = psn::id_from_b64(&self.settings.psn_account).is_some();
        let pad_ok = self.pad.connected();
        let pad_name: String = {
            let n = self.pad.name();
            if n.chars().count() > 22 { n.chars().take(21).collect::<String>() + "…" } else { n }
        };

        let ps = self.page_since;
        let ent = [fade(t, ps + 0.05, 0.5), fade(t, ps + 0.13, 0.5), fade(t, ps + 0.21, 0.5), fade(t, ps + 0.29, 0.5), fade(t, ps + 0.37, 0.55), fade(t, ps + 0.45, 0.55)];
        let hero_console = self.consoles.iter().find(|c| c.state == State::Ready).or(self.consoles.first()).map(|c| (c.is_ps5, c.state == State::Ready));
        let found_a = ui.ctx().animate_value_with_time(egui::Id::new("anim_found"), found as f32, 0.7).round() as i32;
        let paired_a = ui.ctx().animate_value_with_time(egui::Id::new("anim_paired"), paired_found as f32, 0.7).round() as i32;
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Hero
            let w = ui.available_width();
            let hero_target: Option<Console> = ready.first().cloned().or_else(|| self.consoles.first().cloned());
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, if hero_target.is_some() { 190.0 } else { 150.0 }), egui::Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, Rounding::same(22.0), Color32::from_rgb(12, 34, 82));
            let a = (t * 0.4) as f32;
            p.circle_filled(rect.right_top() + Vec2::new(-80.0 + 20.0 * a.sin(), 40.0), 170.0, th_blue().gamma_multiply(0.38));
            p.circle_filled(rect.right_bottom() + Vec2::new(-200.0 + 30.0 * (a * 0.8).cos(), 40.0), 130.0, Color32::from_rgb(110, 70, 240).gamma_multiply(0.25));
            p.circle_filled(rect.left_bottom() + Vec2::new(60.0, 60.0), 110.0, th_blue_light().gamma_multiply(0.12));
            let (hi, line) = if let Some(c) = ready.first() {
                let line = if self.demo_mode {
                    "Das ist eine Demo – starte den Teststream mit einem Klick."
                } else if self.paired_for(c).is_none() {
                    "Einmal koppeln, danach startest du mit einem Klick."
                } else {
                    "Drück auf Spielen und leg los."
                };
                (format!("{} ist bereit", c.name), line.to_string())
            } else if found > 0 {
                ("Konsole im Ruhemodus".to_string(), "Wecke sie auf – dann kann es losgehen.".to_string())
            } else {
                ("Suche nach deiner Konsole …".to_string(), "Konsole einschalten und mit dem Netzwerk verbinden.".to_string())
            };
            p.text(rect.min + Vec2::new(30.0, 26.0), Align2::LEFT_TOP, "WILLKOMMEN BEI OVERLIME", FontId::proportional(12.5), th_blue_light());
            p.text(rect.min + Vec2::new(30.0, 48.0), Align2::LEFT_TOP, hi, FontId::proportional(32.0), Color32::WHITE);
            p.text(rect.min + Vec2::new(30.0, 94.0), Align2::LEFT_TOP, line, FontId::proportional(15.0), Color32::from_rgb(190, 205, 235));
            let q = format!(
                "{}  •  {} FPS  •  {} Mbit/s",
                ["540p", "720p", "1080p"][self.settings.resolution],
                [30, 60][self.settings.fps],
                self.settings.bitrate as i32
            );
            p.text(rect.min + Vec2::new(30.0, 120.0), Align2::LEFT_TOP, q, FontId::proportional(12.5), th_blue_light());
            if let Some((ps5, on)) = hero_console {
                let sc = 0.95 + 0.04 * ((t * 1.3).sin() as f32);
                paint_console(&p, rect.right_center() + Vec2::new(-130.0, 0.0), 1.15 * sc, ps5, on, t, 0.0);
            }

            // Haupt-Knopf: je nach Zustand spielen, aufwecken oder koppeln
            if let Some(c) = hero_target {
                let paired = self.paired_for(&c);
                let busy = self.starting.is_some() || self.session.is_some();
                let (label, kind): (String, u8) = if self.demo_mode {
                    (format!("{}  Demo starten", ic::PLAY), 0)
                } else {
                    match (&paired, c.state == State::Standby) {
                        (None, _) => (format!("{}  Konsole koppeln", ic::LINK), 1),
                        (Some(_), true) => (format!("{}  Aufwecken", ic::POWER), 2),
                        (Some(_), false) => (if busy { "Verbinde …".to_string() } else { format!("{}  Spielen", ic::PLAY) }, 3),
                    }
                };
                let br = Rect::from_min_size(rect.min + Vec2::new(30.0, 140.0), Vec2::new(200.0, 40.0));
                if ui.put(br, egui::Button::new(RichText::new(label.as_str()).size(16.0).strong().color(Color32::from_rgb(10, 30, 80))).fill(Color32::WHITE).rounding(12.0).min_size(Vec2::new(200.0, 40.0))).clicked() && !busy {
                    match (kind, paired) {
                        (0, _) | (1, _) => self.open_dialog(&c),
                        (2, Some(pr)) => self.wake(&c, &pr),
                        (3, Some(pr)) => self.connect(&c, &pr),
                        _ => {}
                    }
                }
            }

            ui.add_space(14.0);

            // Kacheln
            let tw = ((ui.available_width() - 30.0) / 4.0).max(120.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                tile(ui, tw, "KONSOLEN IM NETZ", &found_a.to_string(), if found > 0 { "gefunden" } else { "suche …" }, th_blue_light(), ent[0]);
                tile(ui, tw, "GEKOPPELT", &paired_a.to_string(), if paired_found > 0 { "bereit zum Streamen" } else { "noch keine" }, GREEN, ent[1]);
                tile(
                    ui,
                    tw,
                    "CONTROLLER",
                    if pad_ok { "Erkannt" } else { "Keiner" },
                    &if pad_ok { pad_name.clone() } else { "Bitte anschließen".to_string() },
                    if pad_ok { GREEN } else { YELLOW },
                    ent[2],
                );
                let online = self.local_ip != "?";
                tile(ui, tw, "NETZWERK", if online { "Online" } else { "Offline" }, "lokales Netzwerk", if online { GREEN } else { RED }, ent[3]);
            });

            ui.add_space(14.0);

            ui.columns(2, |cols| {
                cols[0].set_opacity(ent[4]);
                cols[1].set_opacity(ent[5]);
                // Erste Schritte
                egui::Frame::none().fill(th_card()).rounding(18.0).inner_margin(egui::Margin::same(20.0)).show(&mut cols[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Erste Schritte").size(17.0).strong());
                    ui.add_space(6.0);
                    check_row(ui, found > 0, "Konsole gefunden", "Konsole an oder im Ruhemodus, gleiches Netzwerk");
                    ui.add_space(4.0);
                    check_row(ui, have_psn, "PlayStation-Konto verbunden", "Einmalige Anmeldung beim Koppeln");
                    ui.add_space(4.0);
                    check_row(ui, !self.paired.is_empty(), "Konsole gekoppelt", "Code von „Gerät koppeln“ eingeben");
                    ui.add_space(4.0);
                    check_row(ui, pad_ok, "Controller angeschlossen", "Zum Spielen nötig (PS5, PS4, Xbox oder anderer)");
                });

                // Schnellstart
                egui::Frame::none().fill(th_card()).rounding(18.0).inner_margin(egui::Margin::same(20.0)).show(&mut cols[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Schnellstart").size(17.0).strong());
                    ui.add_space(6.0);
                    let list = self.consoles.clone();
                    let mut shown = false;
                    for c in list {
                        let Some(pr) = self.paired_for(&c) else { continue };
                        shown = true;
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&c.name).size(15.0).strong());
                                let (txt, col) = if c.state == State::Ready { ("Bereit", GREEN) } else { ("Ruhemodus", YELLOW) };
                                ui.label(RichText::new(txt).size(12.5).color(col));
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if c.state == State::Standby {
                                    if ui.add(big_button("Aufwecken", th_blue()).min_size(Vec2::new(130.0, 38.0))).clicked() {
                                        self.wake(&c, &pr);
                                    }
                                } else {
                                    let busy = self.starting.is_some();
                                    let label_s = format!("{}  Verbinden", ic::PLAY);
                                    let label: &str = if busy { "Verbinde …" } else { &label_s };
                                    if ui.add_enabled(!busy, big_button(label, th_blue()).min_size(Vec2::new(130.0, 38.0))).clicked() {
                                        self.connect(&c, &pr);
                                    }
                                }
                            });
                        });
                        ui.add_space(4.0);
                    }
                    if !shown {
                        ui.label(RichText::new("Sobald eine Konsole gekoppelt ist, startest du hier mit einem Klick.").color(TEXT_DIM).size(13.5));
                        ui.add_space(6.0);
                        if ui.add(egui::Button::new("Zu den Konsolen").fill(th_card_hover()).rounding(10.0)).clicked() {
                            self.page = Page::Consoles;
                        }
                    }
                    if !self.status.is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new(&self.status).color(TEXT_DIM).size(12.5));
                    }
                });
            });
            ui.add_space(12.0);
            // Neu in OVERLIME
            {
                let (ver, date, items): (String, String, Vec<String>) = match self.news.first() {
                    Some(n) => (n.version.clone(), n.date.clone(), n.items.iter().take(4).cloned().collect()),
                    None => (VERSION.to_string(), String::new(), splash::NEWS.iter().map(|s| s.to_string()).collect()),
                };
                let mut open_news = false;
                egui::Frame::none().fill(th_card()).rounding(18.0).inner_margin(egui::Margin::same(20.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}  Neu in OVERLIME {ver}", ic::NEWSPAPER)).size(17.0).strong());
                        if !date.is_empty() {
                            ui.label(RichText::new(date).color(TEXT_DIM).size(12.5));
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add(egui::Button::new("Alle Neuigkeiten").fill(th_card_hover()).rounding(10.0)).clicked() {
                                open_news = true;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    for it in items {
                        ui.horizontal_top(|ui| {
                            let (r, _) = ui.allocate_exact_size(Vec2::new(10.0, 18.0), egui::Sense::hover());
                            ui.painter().circle_filled(r.center(), 3.0, th_blue_light());
                            ui.add(egui::Label::new(RichText::new(it).size(13.5)).wrap());
                        });
                    }
                });
                if open_news {
                    self.page = Page::News;
                }
                ui.add_space(8.0);
            }
            ui.label(
                RichText::new("Tipp: Für die beste Qualität die Konsole per LAN-Kabel anschließen und am PC 5-GHz-WLAN oder Kabel nutzen.")
                    .color(TEXT_DIM)
                    .size(12.5),
            );
        });
    }

    /// Hilfe & Diagnose in einer Seite mit Reitern.
    pub fn help_page(&mut self, ui: &mut egui::Ui, _t: f64) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Hilfe & Diagnose").size(26.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let rep_btn = ui.add(egui::Button::new(format!("{} Problem melden", ic::BUG)).fill(th_card_hover()).rounding(10.0));
                if rep_btn.clicked() && !RELAY_URL.is_empty() {
                    self.report_open = true;
                    self.report_state = 0;
                }
                if rep_btn.clicked() && RELAY_URL.is_empty() {
                    ui.ctx().copy_text(self.diag_report());
                    let url = format!(
                        "https://github.com/Overtime09991/overlime/issues/new?title={}&body={}",
                        psn::enc(&lang::tr(&format!("Problem mit OVERLIME {VERSION}"))),
                        psn::enc("Bitte hier den Bericht aus der Zwischenablage einfügen (Strg+V) und kurz beschreiben, was nicht klappt.\n\nWas habe ich getan?\n\nWas ist passiert?\n")
                    );
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    self.toasts.push(("Bericht kopiert. Auf der GitHub-Seite mit Strg+V einfügen.".to_string(), 0, self.now));
                }
                if ui.add(egui::Button::new(format!("{} Bericht kopieren", ic::COPY)).fill(th_card_hover()).rounding(10.0)).clicked() {
                    ui.ctx().copy_text(self.diag_report());
                    self.toasts.push(("Bericht in die Zwischenablage kopiert.".to_string(), 2, self.now));
                }
            });
        });
        ui.label(RichText::new("Prüft, ob alles bereit ist, erklärt den Start und hilft bei Problemen.").color(TEXT_DIM).size(13.0));
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            let tabs = [
                (ic::STETHOSCOPE, "Prüfung"),
                (ic::GAME_CONTROLLER, "Controller"),
                (ic::ROCKET_LAUNCH, "Anleitung"),
                (ic::QUESTION, "Fragen"),
                (ic::INFO, "Über"),
            ];
            for (i, (icn, name)) in tabs.iter().enumerate() {
                let sel = self.help_tab == i;
                let b = egui::Button::new(RichText::new(format!("{icn}  {name}")).size(13.5).color(if sel { Color32::WHITE } else { TEXT_DIM }))
                    .fill(if sel { th_blue() } else { th_card() })
                    .rounding(16.0)
                    .min_size(Vec2::new(0.0, 32.0));
                if ui.add(b).clicked() {
                    self.help_tab = i;
                }
            }
        });
        ui.add_space(10.0);
        egui::ScrollArea::vertical().show(ui, |ui| match self.help_tab {
            0 => self.tab_check(ui),
            1 => self.tab_pad(ui),
            2 => self.tab_guide(ui),
            3 => self.tab_faq(ui),
            _ => self.tab_about(ui),
        });
    }

    fn tab_check(&mut self, ui: &mut egui::Ui) {
        let online = self.local_ip != "?";
        let found = self.consoles.len();
        let first = self.consoles.first().cloned();
        let paired_any = self.consoles.iter().any(|c| self.paired_for(c).is_some());
        let psn_ok = psn::id_from_b64(&self.settings.psn_account).is_some();
        let pad_ok = self.pad.connected();
        let problems = [online, found > 0, psn_ok, paired_any].iter().filter(|b| !**b).count();

        // Zusammenfassung
        let (col, icn, head, sub) = if problems == 0 {
            (GREEN, ic::CHECK_CIRCLE, "Alles bereit".to_string(), "Netzwerk, Konsole und Konto sind in Ordnung.".to_string())
        } else {
            (RED, ic::WARNING_CIRCLE, format!("{problems} {} brauchen Aufmerksamkeit", if problems == 1 { "Punkt" } else { "Punkte" }), "Unten steht bei jedem Punkt, was du tun kannst.".to_string())
        };
        egui::Frame::none().fill(col.gamma_multiply(0.14)).rounding(14.0).stroke(Stroke::new(1.0, col.gamma_multiply(0.5))).inner_margin(egui::Margin::same(16.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(icn).size(30.0).color(col));
                ui.vertical(|ui| {
                    ui.label(RichText::new(head).size(18.0).strong());
                    ui.label(RichText::new(sub).color(TEXT_DIM).size(13.0));
                });
            });
        });
        ui.add_space(10.0);

        section(ui, &format!("{}  Prüfung", ic::STETHOSCOPE), |ui| {
            status_row(ui, if online { 0 } else { 2 }, "Netzwerk", if online { "Der PC ist mit einem Netzwerk verbunden." } else { "Kein Netzwerk. Mit WLAN oder LAN verbinden." });
            ui.add_space(6.0);
            status_row(ui, if found > 0 { 0 } else { 2 }, "Konsole gefunden", if found > 0 { "Mindestens eine Konsole antwortet im Netzwerk." } else { "Keine Konsole gefunden. Einschalten (oder Ruhemodus mit Netzwerk), gleiches Netzwerk, Firewall-Abfrage erlauben." });
            ui.add_space(6.0);
            let st = match first.as_ref().map(|c| c.state.clone()) { Some(State::Ready) => 0, Some(State::Standby) => 1, Some(State::Unknown) => 1, None => 2 };
            let st_txt = match first.as_ref().map(|c| c.state.clone()) {
                Some(State::Ready) => "Die Konsole ist an und bereit.",
                Some(State::Standby) => "Die Konsole ist im Ruhemodus. Mit „Aufwecken“ starten (dafür Remote Play und Netzwerk-Aufwecken aktivieren).",
                _ => "Kein Zustand bekannt.",
            };
            status_row(ui, st, "Konsole bereit", st_txt);
            ui.add_space(6.0);
            status_row(ui, if psn_ok { 0 } else { 2 }, "PlayStation-Konto", if psn_ok { "Angemeldet." } else { "Nicht angemeldet. Beim Koppeln einmalig anmelden." });
            ui.add_space(6.0);
            status_row(ui, if paired_any { 0 } else { 2 }, "Konsole gekoppelt", if paired_any { "Die Konsole ist gekoppelt." } else { "Noch nicht gekoppelt. Bei der Konsole auf „Koppeln“ klicken." });
            ui.add_space(6.0);
            status_row(ui, if pad_ok { 0 } else { 1 }, "Controller", if pad_ok { "Ein Controller ist verbunden." } else { "Kein Controller erkannt. Zum Spielen schließt du einen an (USB oder Bluetooth)." });
            ui.add_space(6.0);
            let (us, ut) = if self.update.is_some() { (1, "Ein Update ist verfügbar.") } else if self.update_checked { (0, "OVERLIME ist auf dem neuesten Stand.") } else { (1, "Noch nicht geprüft.") };
            status_row(ui, us, "Version", &format!("{VERSION}. {ut}"));
        });
        ui.add_space(8.0);
        section(ui, &format!("{}  Verbindungstest", ic::WIFI_HIGH), |ui| {
            ui.label(RichText::new("Prüft, ob der Remote-Play-Port der Konsole erreichbar ist. Die Konsole muss dafür an sein.").color(TEXT_DIM).size(12.5));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let can = first.is_some() && !self.diag_busy;
                if ui.add_enabled(can, big_button("Test starten", th_blue()).min_size(Vec2::new(160.0, 40.0))).clicked() {
                    if let Some(c) = first.clone() {
                        self.diag_busy = true;
                        self.diag_port = None;
                        let tx = self.tx.clone();
                        let addr = c.addr.clone();
                        std::thread::spawn(move || {
                            let sa = format!("{addr}:9295").parse::<std::net::SocketAddr>();
                            let Ok(sa) = sa else {
                                let _ = tx.send(Msg::DiagPort(false, "Ungültige Adresse".to_string()));
                                return;
                            };
                            let mut times: Vec<f32> = Vec::new();
                            let mut fails = 0u32;
                            let mut last_err = String::new();
                            let n = 12u32;
                            for _ in 0..n {
                                let t0 = Instant::now();
                                match std::net::TcpStream::connect_timeout(&sa, Duration::from_millis(1500)) {
                                    Ok(_) => times.push(t0.elapsed().as_secs_f32() * 1000.0),
                                    Err(e) => {
                                        fails += 1;
                                        last_err = format!("{:?}", e.kind());
                                    }
                                }
                                std::thread::sleep(Duration::from_millis(80));
                            }
                            if times.is_empty() {
                                let _ = tx.send(Msg::DiagPort(false, format!("Der Port ist nicht erreichbar ({last_err}). Ist Remote Play an der Konsole aktiviert und die Konsole an? Blockiert eine Firewall?")));
                                return;
                            }
                            let avg = times.iter().sum::<f32>() / times.len() as f32;
                            let var = times.iter().map(|t| (t - avg) * (t - avg)).sum::<f32>() / times.len() as f32;
                            let _ = tx.send(Msg::DiagPort(true, "Der Remote-Play-Port der Konsole ist erreichbar.".to_string()));
                            let _ = tx.send(Msg::NetTest(NetResult { avg, jitter: var.sqrt(), fails, n }));
                        });
                    }
                }
                if self.diag_busy {
                    ui.spinner();
                    ui.label(RichText::new("Teste …").color(TEXT_DIM));
                }
            });
            if first.is_none() {
                ui.label(RichText::new("Zuerst muss eine Konsole gefunden werden.").color(YELLOW).size(12.5));
            }
            if let Some((ok, txt)) = &self.diag_port {
                ui.add_space(6.0);
                status_row(ui, if *ok { 0 } else { 2 }, if *ok { "Erreichbar" } else { "Nicht erreichbar" }, txt);
            }
            if let Some(r) = self.net_res.clone() {
                let (res, fps, mbit, label) = net_recommend(&r);
                ui.add_space(8.0);
                egui::Frame::none().fill(th_bg().gamma_multiply(0.6)).rounding(12.0).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(format!("Verbindung zur Konsole: {label}")).size(16.0).strong());
                    ui.label(RichText::new(format!("Antwortzeit Ø {:.1} ms  •  Schwankung {:.1} ms  •  Ausfälle {} von {}", r.avg, r.jitter, r.fails, r.n)).color(TEXT_DIM).size(12.5));
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("Empfehlung: {}, {} FPS, {} Mbit/s", ["540p", "720p", "1080p"][res], [30, 60][fps], mbit as i32)).size(14.0));
                    ui.label(RichText::new("Der Test misst Antwortzeit und Schwankung, nicht die Geschwindigkeit deiner Leitung. Bei Rucklern die Bitrate weiter senken.").color(TEXT_DIM).size(12.0));
                    ui.add_space(6.0);
                    if ui.add(egui::Button::new(format!("{} Empfehlung übernehmen", ic::CHECK)).fill(th_blue()).rounding(10.0)).clicked() {
                        self.settings.resolution = res;
                        self.settings.fps = fps;
                        self.settings.bitrate = mbit;
                        self.toasts.push(("Empfehlung übernommen.".to_string(), 2, self.now));
                    }
                });
            }
            if let Some(h) = self.net_hint {
                ui.add_space(8.0);
                egui::Frame::none().fill(YELLOW.gamma_multiply(0.12)).rounding(12.0).stroke(Stroke::new(1.0, YELLOW.gamma_multiply(0.5))).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Beim letzten Spiel gingen viele Pakete verloren").size(15.0).strong());
                    ui.label(RichText::new(format!("Mit {} Mbit/s läuft es vermutlich ruhiger.", h as i32)).color(TEXT_DIM).size(13.0));
                    ui.add_space(6.0);
                    if ui.add(egui::Button::new(format!("{} Bitrate auf {} Mbit/s senken", ic::CHECK, h as i32)).fill(th_blue()).rounding(10.0)).clicked() {
                        self.settings.bitrate = h;
                        self.net_hint = None;
                        self.toasts.push(("Bitrate gesenkt.".to_string(), 2, self.now));
                    }
                });
            }
        });
    }

    fn tab_pad(&mut self, ui: &mut egui::Ui) {
        section(ui, &format!("{}  Controller-Test", ic::GAME_CONTROLLER), |ui| {
            let conn = self.pad.connected();
            let inp = self.pad.poll();
            let ex = self.pad.poll_extra();
            ui.label(RichText::new("Drücke Tasten und bewege Sticks. Was hier reagiert, kommt auch im Spiel an. Die App selbst lässt sich nicht mit dem Controller bedienen. Nachbau-Controller und fremde Marken werden meist automatisch erkannt, sonst hilft „Controller einrichten“.").color(TEXT_DIM).size(12.5));
            ui.add_space(4.0);
            pad_visual(ui, &inp, &ex, conn);
            ui.ctx().request_repaint();
            ui.add_space(8.0);
            if self.wiz.is_some() {
                let (mut cancel, mut skip, mut done) = (false, false, false);
                if let Some(w) = self.wiz.as_mut() {
                    w.poll();
                    ui.ctx().request_repaint();
                    egui::Frame::none().fill(th_bg().gamma_multiply(0.6)).rounding(14.0).stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.6))).inner_margin(egui::Margin::same(18.0)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(format!("Controller einrichten: {}", w.name)).size(16.0).strong());
                        let total = pad::WIZ_STEPS.len();
                        ui.add(egui::ProgressBar::new(w.step as f32 / total as f32).desired_height(8.0));
                        ui.add_space(6.0);
                        if w.finished {
                            done = true;
                        } else {
                            ui.label(RichText::new(format!("Schritt {} von {}", w.step + 1, total)).color(TEXT_DIM).size(12.5));
                            ui.label(RichText::new(format!("Drücke jetzt: {}", w.prompt())).size(20.0).strong().color(th_blue_light()));
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if ui.add(egui::Button::new("Überspringen").fill(th_card_hover()).rounding(10.0)).clicked() {
                                    skip = true;
                                }
                                if ui.add(egui::Button::new("Abbrechen").fill(th_card_hover()).rounding(10.0)).clicked() {
                                    cancel = true;
                                }
                            });
                        }
                    });
                    if skip {
                        w.skip();
                    }
                }
                if done {
                    let r = self.wiz.as_ref().map(|w| w.finish());
                    match r {
                        Some(Ok(())) => {
                            self.toasts.push(("Controller eingerichtet und gespeichert.".to_string(), 2, self.now));
                            self.wiz = None;
                            self.pad.rescan();
                        }
                        Some(Err(e)) => {
                            self.toasts.push((e, 1, self.now));
                            self.wiz = None;
                        }
                        None => {}
                    }
                }
                if cancel {
                    self.wiz = None;
                }
                ui.add_space(8.0);
            }
            let devs = self.pad.devices();
            if devs.is_empty() {
                status_row(ui, 1, "Kein Gerät gefunden", "Windows meldet keinen Controller. Per USB-Kabel anschließen oder per Bluetooth koppeln (Windows: Einstellungen > Bluetooth und Geräte), dann „Neu suchen“ klicken. Ohne Bluetooth am PC hilft ein USB-Kabel oder ein Bluetooth-Stick.");
            } else {
                for (i, (name, usable, vid, pid)) in devs.iter().enumerate() {
                    status_row(ui, if *usable { 0 } else { 1 }, &format!("{name}  ({vid:04x}:{pid:04x})"), if *usable { "Als Gamepad erkannt." } else { "Gerät gefunden, aber die Tasten sind noch nicht zugeordnet. Mit „Einrichten“ geht es in einer Minute." });
                    if ui.add(egui::Button::new(format!("{} Controller einrichten", ic::SLIDERS)).fill(th_card_hover()).rounding(10.0)).clicked() {
                        self.wiz = self.pad.wizard_start(i as i32);
                        if self.wiz.is_none() {
                            self.toasts.push(("Das Gerät lässt sich nicht öffnen.".to_string(), 1, self.now));
                        }
                    }
                    ui.add_space(6.0);
                }
            }
            ui.add_space(4.0);
            if ui.add(egui::Button::new(format!("{} Controller neu suchen", ic::ARROWS_CLOCKWISE)).fill(th_card_hover()).rounding(10.0)).clicked() {
                self.pad.rescan();
                self.toasts.push(("Controller wird neu gesucht.".to_string(), 0, self.now));
            }
        });
        ui.add_space(8.0);
        section(ui, &format!("{}  Controller koppeln", ic::BLUETOOTH), |ui| {
            step_row(ui, "1", "PS4-Controller: Share- und PS-Taste halten, bis die Leiste schnell blinkt. PS5-Controller: Create- und PS-Taste halten.");
            step_row(ui, "2", "In Windows: Einstellungen > Bluetooth und Geräte > Gerät hinzufügen > Bluetooth.");
            step_row(ui, "3", "Hier „Controller neu suchen“ klicken. Alternativ einfach per USB-Kabel anschließen.");
        });
    }

    fn tab_guide(&mut self, ui: &mut egui::Ui) {
        section(ui, &format!("{}  Schnellstart", ic::ROCKET_LAUNCH), |ui| {
            step_row(ui, "1", "Konsole einschalten, Remote Play aktivieren (Einstellungen > System > Remote Play).");
            step_row(ui, "2", "In OVERLIME bei der Konsole auf „Koppeln“ klicken und dem Pop-up folgen.");
            step_row(ui, "3", "Danach „Verbinden“ klicken – fertig.");
            ui.add_space(4.0);
            ui.label(RichText::new("Tipp: Mit dem Symbol neben der Uhr (Einstellungen > App & Updates) weckt ein Klick die Konsole und verbindet automatisch.").color(TEXT_DIM).size(12.5));
        });
        ui.add_space(8.0);
        section(ui, &format!("{}  Kurztasten", ic::KEYBOARD), |ui| {
            ui.label(RichText::new("Gespielt wird mit einem Controller (PS5, PS4, Xbox oder andere, automatisch erkannt, inklusive Vibration). Diese Kurztasten gibt es:").color(TEXT_DIM));
            ui.add_space(4.0);
            egui::Grid::new("keys").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
                for (k, v) in [
                    ("Esc", "Menü im Stream (beendet nichts aus Versehen)"),
                    ("F9", "Aufnahme starten/beenden (MP4 in Videos\\OVERLIME)"),
                    ("F10", "Statusanzeige ein/aus"),
                    ("F11", "Vollbild"),
                    ("F5", "Konsolen neu suchen"),
                    ("Strg + 1 bis 5", "Seite wechseln"),
                ] {
                    ui.label(RichText::new(k).monospace().color(GREEN));
                    ui.label(v);
                    ui.end_row();
                }
            });
        });
    }

    fn tab_faq(&mut self, ui: &mut egui::Ui) {
        section(ui, &format!("{}  Häufige Fragen", ic::WRENCH), |ui| {
            for (q, a) in [
                ("Konsole wird nicht gefunden", "Gleiches Netzwerk? Konsole an oder im Ruhemodus mit „Mit dem Netzwerk verbunden bleiben“. Firewall-Abfrage von Windows erlauben."),
                ("Kopplung klappt nicht", "Der 8-stellige Code gilt nur kurz. „Gerät koppeln“ an der Konsole neu öffnen und den neuen Code eingeben."),
                ("Bild ruckelt oder hat Verzögerung", "Konsole per LAN-Kabel, PC per Kabel oder 5 GHz. In den Einstellungen Bitrate senken oder auf 720p stellen."),
                ("Kein Ton", "Standard-Wiedergabegerät in Windows prüfen. In der Lautstärke-Mischung von Windows nachsehen, ob OVERLIME nicht stummgeschaltet ist."),
                ("Controller wird nicht erkannt", "Im Reiter „Controller“ den Live-Test öffnen. Ohne Bluetooth am PC das USB-Kabel nutzen. Danach „Neu suchen“ klicken."),
                ("Windows blockiert die App", "Bei „Intelligente App-Kontrolle“ oder SmartScreen: Die App ist noch nicht signiert. Bei SmartScreen „Weitere Informationen“ und „Trotzdem ausführen“ wählen."),
                ("Anmeldung bei PlayStation schlägt fehl", "Die ganze Adresse aus der Adressleiste kopieren (sie beginnt mit https:// und enthält code=). Wenn sie abgelaufen ist, im Browser neu anmelden."),
            ] {
                egui::CollapsingHeader::new(RichText::new(q).strong().size(14.5)).show(ui, |ui| {
                    ui.label(RichText::new(a).color(TEXT_DIM).size(13.0));
                    ui.add_space(4.0);
                });
            }
        });
        ui.add_space(8.0);
        section(ui, &format!("{}  Wenn etwas nicht klappt", ic::BUG), |ui| {
            ui.label(RichText::new("Klicke oben auf „Problem melden“. OVERLIME kopiert einen Bericht (ohne IP-Adressen und Passwörter) und öffnet die Meldeseite auf GitHub. Dort fügst du ihn mit Strg+V ein und beschreibst kurz, was passiert ist.").color(TEXT_DIM).size(13.0));
            ui.add_space(6.0);
            if ui.add(egui::Button::new("Protokoll-Ordner öffnen").fill(th_card_hover()).rounding(10.0)).clicked() {
                if let Some(p) = log_path().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
                    let _ = std::fs::create_dir_all(&p);
                    let _ = std::process::Command::new("explorer").arg(p).spawn();
                }
            }
        });
    }

    fn tab_about(&mut self, ui: &mut egui::Ui) {
        section(ui, &format!("{}  Über OVERLIME", ic::INFO), |ui| {
            ui.label(RichText::new(format!("Version {VERSION}  •  PS4 & PS5  •  bis 1080p60 (Sony-Limit), Hochskalierung auf 4K")).color(TEXT_DIM));
            ui.label(RichText::new("Der Stream-Kern basiert auf dem Open-Source-Projekt chiaki-ng (AGPL-3.0).").color(TEXT_DIM).size(12.5));
            ui.label(RichText::new("Nicht mit Sony verbunden. „PlayStation“ ist eine Marke von Sony Interactive Entertainment.").color(TEXT_DIM).size(12.5));
            ui.add_space(6.0);
            if ui.add(egui::Button::new(format!("{} Tour wiederholen", ic::ROCKET_LAUNCH)).fill(th_card_hover()).rounding(10.0)).clicked() {
                self.tour = Some(0);
            }
            ui.add_space(4.0);
            if ui.add(egui::Button::new(format!("{} Projektseite auf GitHub", ic::GITHUB_LOGO)).fill(th_card_hover()).rounding(10.0)).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab("https://github.com/Overtime09991/overlime"));
            }
        });
    }
}

impl App {
    /// Öffnet Sonys Anmeldeseite und vergisst alte Adressen, damit kein verbrauchter Code eingefügt wird.
    fn open_psn_login(&mut self, ctx: &egui::Context) {
        self.login_old_clip = winui::clipboard_text().trim().to_string();
        self.login_pasted.clear();
        self.login_error.clear();
        ctx.open_url(egui::OpenUrl::new_tab(psn::login_url()));
    }

    /// Erster Start: geführter Assistent (Willkommen, Konto verbinden, Adresse einfügen).
    pub fn login_screen(&mut self, ctx: &egui::Context, t: f64) {
        // Zwischenablage beobachten: Sony-Adresse wird automatisch erkannt
        if self.login_step == 2 && self.login_pasted.is_empty() && t - self.clip_seen > 0.6 {
            self.clip_seen = t;
            let c = winui::clipboard_text();
            if c.contains("code=") && c.trim() != self.login_old_clip && psn::parse_code(&c).is_some() {
                self.login_pasted = c.trim().to_string();
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(300));
        egui::Area::new(egui::Id::new("lang_sw")).order(egui::Order::Foreground).anchor(Align2::RIGHT_TOP, [-16.0, 40.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (i, name) in ["DE", "EN"].iter().enumerate() {
                    let sel = self.settings.language == i;
                    let b = egui::Button::new(RichText::new(*name).size(12.5).color(if sel { Color32::WHITE } else { TEXT_DIM })).fill(if sel { th_blue() } else { th_card_hover() }).rounding(10.0).min_size(Vec2::new(36.0, 26.0));
                    if ui.add(b).clicked() {
                        self.settings.language = i;
                    }
                }
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::none().fill(th_bg())).show(ctx, |ui| {
            let r = ui.max_rect();
            let p = ui.painter();
            let a = (t * 0.3) as f32;
            p.circle_filled(Pos2::new(r.width() * (0.2 + 0.05 * a.sin()), r.height() * 0.25), 320.0, th_blue().gamma_multiply(0.10));
            p.circle_filled(Pos2::new(r.width() * (0.85 + 0.04 * (a * 1.2).cos()), r.height() * 0.8), 360.0, Color32::from_rgb(100, 60, 230).gamma_multiply(0.08));

            ui.vertical_centered(|ui| {
                ui.add_space((r.height() * 0.5 - 285.0).max(10.0));
                ui.label(RichText::new("OVERLIME").font(FontId::proportional(44.0)).strong().color(Color32::WHITE));
                ui.label(RichText::new("REMOTE PLAY").font(FontId::proportional(14.0)).color(th_blue_light()));
                ui.add_space(14.0);
                // Fortschritt
                ui.horizontal(|ui| {
                    let w = 3.0 * 34.0 + 2.0 * 8.0;
                    ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                    for i in 0..3 {
                        let (rc, _) = ui.allocate_exact_size(Vec2::new(34.0, 6.0), egui::Sense::hover());
                        let k = ui.ctx().animate_value_with_time(egui::Id::new(("lg_dot", i)), if i <= self.login_step { 1.0 } else { 0.0 }, 0.3);
                        let col = TEXT_DIM.gamma_multiply(0.25).lerp_to_gamma(th_blue_light(), k);
                        ui.painter().rect_filled(rc, Rounding::same(3.0), col);
                    }
                });
                ui.add_space(14.0);
                egui::Frame::none()
                    .fill(th_card())
                    .rounding(22.0)
                    .stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.4)))
                    .inner_margin(egui::Margin::same(30.0))
                    .show(ui, |ui| {
                        ui.set_width(500.0);
                        match self.login_step {
                            0 => {
                                ui.label(RichText::new("Willkommen!").size(26.0).strong());
                                ui.add_space(4.0);
                                ui.label(RichText::new("In zwei Minuten bist du startklar: einmal mit deinem PlayStation-Konto anmelden, danach spielst du von deinem PC aus.").color(TEXT_DIM).size(14.5));
                                ui.add_space(14.0);
                                for (icn, title, sub) in [
                                    (ic::MONITOR_PLAY, "Bis 1080p mit 60 FPS", "Scharf hochgerechnet auf deinen Bildschirm"),
                                    (ic::GAME_CONTROLLER, "Dein Controller, voll unterstützt", "Vibration, Touchpad und Bewegungssensor"),
                                    (ic::POWER, "PS5 per Klick aufwecken", "Auch mit Symbol neben der Uhr"),
                                ] {
                                    ui.horizontal(|ui| {
                                        let (rc, _) = ui.allocate_exact_size(Vec2::splat(38.0), egui::Sense::hover());
                                        ui.painter().rect_filled(rc, Rounding::same(10.0), th_blue().gamma_multiply(0.25));
                                        ui.painter().text(rc.center(), Align2::CENTER_CENTER, icn, FontId::proportional(20.0), th_blue_light());
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(title).strong().size(14.5));
                                            ui.label(RichText::new(sub).color(TEXT_DIM).size(12.5));
                                        });
                                    });
                                    ui.add_space(6.0);
                                }
                                ui.add_space(10.0);
                                if ui.add(big_button("Los geht's", th_blue()).min_size(Vec2::new(440.0, 48.0))).clicked() {
                                    self.login_step = 1;
                                }
                                ui.add_space(8.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new("Nicht mit Sony verbunden. „PlayStation“ ist eine Marke von Sony Interactive Entertainment.").color(TEXT_DIM).size(11.5));
                                });
                            }
                            1 => {
                                ui.label(RichText::new("Konto verbinden").size(24.0).strong());
                                ui.add_space(4.0);
                                ui.label(RichText::new("Die App braucht deine PlayStation-Konto-Nummer, damit die Konsole dich erkennt. Du meldest dich dafür auf der Seite von Sony an.").color(TEXT_DIM).size(14.5));
                                ui.add_space(14.0);
                                if ui.add(big_button("Bei PlayStation anmelden", th_blue()).min_size(Vec2::new(440.0, 50.0))).clicked() {
                                    self.open_psn_login(ctx);
                                    self.login_step = 2;
                                    self.clip_seen = 0.0;
                                }
                                ui.add_space(12.0);
                                egui::Frame::none().fill(th_bg().gamma_multiply(0.6)).rounding(12.0).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                                    ui.set_width(416.0);
                                    ui.horizontal_top(|ui| {
                                        ui.label(RichText::new(ic::LOCK_KEY).size(20.0).color(GREEN));
                                        ui.add(egui::Label::new(RichText::new("Dein Passwort gibst du nur bei Sony ein. OVERLIME speichert nur die Konto-Nummer, nie dein Passwort.").color(TEXT_DIM).size(12.5)).wrap());
                                    });
                                });
                            }
                            _ => {
                                ui.label(RichText::new("Adresse einfügen").size(24.0).strong());
                                ui.add_space(4.0);
                                ui.label(RichText::new("Nach der Anmeldung zeigt der Browser eine Seite mit einer langen Adresse. Kopiere sie aus der Adressleiste (Strg + L, dann Strg + C).").color(TEXT_DIM).size(14.5));
                                ui.add_space(12.0);
                                let code = psn::parse_code(&self.login_pasted);
                                egui::Frame::none()
                                    .fill(th_bg().gamma_multiply(0.7))
                                    .rounding(12.0)
                                    .stroke(Stroke::new(1.0, if code.is_some() { GREEN.gamma_multiply(0.7) } else { th_blue().gamma_multiply(0.35) }))
                                    .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                                    .show(ui, |ui| {
                                        ui.set_width(416.0);
                                        ui.horizontal(|ui| {
                                            ui.add(egui::TextEdit::singleline(&mut self.login_pasted).frame(false).hint_text("Adresse hier einfügen").desired_width(330.0));
                                            if code.is_some() {
                                                ui.label(RichText::new(ic::CHECK_CIRCLE).size(20.0).color(GREEN));
                                            } else if ui.add(egui::Button::new(RichText::new("Einfügen").size(13.0)).fill(th_card_hover()).rounding(8.0)).clicked() {
                                                self.login_pasted = winui::clipboard_text().trim().to_string();
                                            }
                                        });
                                    });
                                if code.is_some() {
                                    ui.label(RichText::new("Adresse erkannt. Jetzt anmelden.").color(GREEN).size(13.0));
                                }
                                if !self.login_error.is_empty() {
                                    ui.label(RichText::new(&self.login_error).color(RED).size(13.0));
                                }
                                ui.add_space(10.0);
                                if self.login_busy {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label(RichText::new("Melde an …").color(TEXT_DIM));
                                    });
                                } else if ui.add_enabled(code.is_some(), big_button("Anmelden", th_blue()).min_size(Vec2::new(440.0, 50.0))).clicked() {
                                    if let Some(code) = code {
                                        self.login_busy = true;
                                        self.login_error.clear();
                                        let tx = self.tx.clone();
                                        std::thread::spawn(move || {
                                            let m = match psn::account_id_from_code(&code) {
                                                Ok((id, name, tokens)) => Msg::PsnOk(id, name, tokens),
                                                Err(e) => Msg::PsnErr(e),
                                            };
                                            let _ = tx.send(m);
                                        });
                                    }
                                }
                                ui.add_space(4.0);
                                if ui.add(egui::Button::new(RichText::new("Browser erneut öffnen").color(TEXT_DIM).size(12.5)).frame(false)).clicked() {
                                    self.open_psn_login(ctx);
                                }
                            }
                        }
                    });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let w = if self.login_step > 0 { 330.0 } else { 250.0 };
                    ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                    if self.login_step > 0 && ui.add(egui::Button::new(RichText::new("Zurück").color(TEXT_DIM)).frame(false)).clicked() {
                        self.login_step -= 1;
                        self.login_error.clear();
                    }
                    if ui.add(egui::Button::new(RichText::new("Später anmelden").color(TEXT_DIM)).frame(false)).clicked() {
                        self.skip_login = true;
                        self.relogin = false;
                    }
                    if ui.add(egui::Button::new(RichText::new("Demo ausprobieren").color(th_blue_light())).frame(false)).clicked() {
                        self.skip_login = true;
                        self.enter_demo();
                        self.page = Page::Consoles;
                    }
                });
            });
        });
    }
}

fn status_row(ui: &mut egui::Ui, state: u8, title: &str, sub: &str) {
    // state: 0 ok, 1 Hinweis, 2 Fehler
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(26.0), egui::Sense::hover());
        let c = r.center();
        let col = match state { 0 => GREEN, 1 => YELLOW, _ => RED };
        ui.painter().circle_filled(c, 12.0, col.gamma_multiply(0.22));
        match state {
            0 => {
                ui.painter().line_segment([c + Vec2::new(-5.0, 0.0), c + Vec2::new(-1.5, 4.0)], Stroke::new(2.4, col));
                ui.painter().line_segment([c + Vec2::new(-1.5, 4.0), c + Vec2::new(5.5, -4.5)], Stroke::new(2.4, col));
            }
            1 => {
                ui.painter().text(c, Align2::CENTER_CENTER, "!", FontId::proportional(15.0), col);
            }
            _ => {
                ui.painter().line_segment([c + Vec2::new(-4.5, -4.5), c + Vec2::new(4.5, 4.5)], Stroke::new(2.4, col));
                ui.painter().line_segment([c + Vec2::new(4.5, -4.5), c + Vec2::new(-4.5, 4.5)], Stroke::new(2.4, col));
            }
        }
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(14.5).strong());
            ui.label(RichText::new(sub).size(12.5).color(TEXT_DIM));
        });
    });
}

/// Live-Ansicht eines Controllers (Tasten, Sticks, Trigger, Touchpad, Bewegung).
pub fn pad_visual(ui: &mut egui::Ui, inp: &pad::Input, ex: &pad::Extra, connected: bool) {
    let w = ui.available_width().min(560.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 250.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, Rounding::same(18.0), th_bg());
    p.rect_stroke(rect, Rounding::same(18.0), Stroke::new(1.0, Color32::from_white_alpha(18)));
    let on = th_blue_light();
    let off = Color32::from_white_alpha(26);
    let bt = |m: u32| inp.buttons & m != 0;
    let c = rect.center();
    if !connected {
        p.text(c, Align2::CENTER_CENTER, "Kein Controller erkannt", FontId::proportional(16.0), TEXT_DIM);
        return;
    }
    // Schultertasten und Trigger
    for (i, (m, tv)) in [(1u32 << 8, inp.l2), (1u32 << 9, inp.r2)].iter().enumerate() {
        let x = if i == 0 { rect.min.x + 60.0 } else { rect.max.x - 60.0 - 90.0 };
        let sh = Rect::from_min_size(Pos2::new(x, rect.min.y + 14.0), Vec2::new(90.0, 14.0));
        p.rect_filled(sh, Rounding::same(7.0), if bt(*m) { on } else { off });
        let tr = Rect::from_min_size(Pos2::new(x, rect.min.y + 34.0), Vec2::new(90.0, 12.0));
        p.rect_filled(tr, Rounding::same(6.0), off);
        p.rect_filled(Rect::from_min_size(tr.min, Vec2::new(90.0 * (*tv as f32 / 255.0), 12.0)), Rounding::same(6.0), on);
        p.text(Pos2::new(x + 45.0, rect.min.y + 54.0), Align2::CENTER_CENTER, if i == 0 { "L1 / L2" } else { "R1 / R2" }, FontId::proportional(10.5), TEXT_DIM);
    }
    // Steuerkreuz
    let dc = Pos2::new(rect.min.x + 100.0, rect.min.y + 100.0);
    for (m, dx, dy) in [(1u32 << 6, 0.0, -24.0), (1u32 << 7, 0.0, 24.0), (1u32 << 4, -24.0, 0.0), (1u32 << 5, 24.0, 0.0)] {
        p.rect_filled(Rect::from_center_size(dc + Vec2::new(dx, dy), Vec2::splat(22.0)), Rounding::same(6.0), if bt(m) { on } else { off });
    }
    // Aktionstasten (Kreuz unten, Kreis rechts, Quadrat links, Dreieck oben)
    let fc = Pos2::new(rect.max.x - 100.0, rect.min.y + 100.0);
    let r = 11.0;
    p.circle_filled(fc + Vec2::new(0.0, 24.0), r, if bt(1 << 0) { on } else { off });
    p.circle_filled(fc + Vec2::new(24.0, 0.0), r, if bt(1 << 1) { on } else { off });
    p.circle_filled(fc + Vec2::new(-24.0, 0.0), r, if bt(1 << 2) { on } else { off });
    p.circle_filled(fc + Vec2::new(0.0, -24.0), r, if bt(1 << 3) { on } else { off });
    let ic_col = Color32::from_white_alpha(170);
    let xs = Stroke::new(1.6, ic_col);
    let cx = fc + Vec2::new(0.0, 24.0);
    p.line_segment([cx + Vec2::new(-4.0, -4.0), cx + Vec2::new(4.0, 4.0)], xs);
    p.line_segment([cx + Vec2::new(4.0, -4.0), cx + Vec2::new(-4.0, 4.0)], xs);
    p.circle_stroke(fc + Vec2::new(24.0, 0.0), 4.5, xs);
    p.rect_stroke(Rect::from_center_size(fc + Vec2::new(-24.0, 0.0), Vec2::splat(9.0)), Rounding::ZERO, xs);
    let tc = fc + Vec2::new(0.0, -24.0);
    p.add(Shape::closed_line(vec![tc + Vec2::new(0.0, -5.0), tc + Vec2::new(5.0, 4.0), tc + Vec2::new(-5.0, 4.0)], xs));
    // Sticks
    for (i, (sx, sy, m)) in [(inp.lx, inp.ly, 1u32 << 10), (inp.rx, inp.ry, 1u32 << 11)].iter().enumerate() {
        let sc = Pos2::new(rect.min.x + w * if i == 0 { 0.33 } else { 0.67 }, rect.min.y + 170.0);
        p.circle_filled(sc, 36.0, off);
        p.circle_stroke(sc, 36.0, Stroke::new(1.0, Color32::from_white_alpha(40)));
        let dot = sc + Vec2::new(*sx as f32 / 32768.0 * 28.0, *sy as f32 / 32768.0 * 28.0);
        p.circle_filled(dot, 12.0, if bt(*m) { on } else { Color32::from_white_alpha(150) });
    }
    // Mitte: Share, PS, Options, Touchpad
    let mid = Pos2::new(c.x, rect.min.y + 70.0);
    let tp = Rect::from_center_size(mid, Vec2::new(120.0, 52.0));
    p.rect_filled(tp, Rounding::same(8.0), if bt(1 << 14) { on.gamma_multiply(0.6) } else { off });
    for f in 0..2 {
        if ex.touch_down[f] {
            p.circle_filled(Pos2::new(tp.min.x + ex.touch_x[f] * tp.width(), tp.min.y + ex.touch_y[f] * tp.height()), 6.0, Color32::WHITE);
        }
    }
    p.circle_filled(Pos2::new(c.x, rect.min.y + 118.0), 8.0, if bt(1 << 15) { on } else { off });
    p.circle_filled(Pos2::new(c.x - 44.0, rect.min.y + 118.0), 6.0, if bt(1 << 13) { on } else { off });
    p.circle_filled(Pos2::new(c.x + 44.0, rect.min.y + 118.0), 6.0, if bt(1 << 12) { on } else { off });
    p.text(Pos2::new(c.x - 44.0, rect.min.y + 134.0), Align2::CENTER_CENTER, "Share", FontId::proportional(10.0), TEXT_DIM);
    p.text(Pos2::new(c.x + 44.0, rect.min.y + 134.0), Align2::CENTER_CENTER, "Options", FontId::proportional(10.0), TEXT_DIM);
    // Bewegung
    let motion = if ex.has_motion { format!("Bewegung: Gyro {:+.1} {:+.1} {:+.1}", ex.gyro[0], ex.gyro[1], ex.gyro[2]) } else { "Bewegung: nicht verfügbar".to_string() };
    p.text(Pos2::new(c.x, rect.max.y - 14.0), Align2::CENTER_CENTER, motion, FontId::monospace(11.0), TEXT_DIM);
}
