//! Übersichts- und Hilfeseite.
use super::*;

pub fn tile(ui: &mut egui::Ui, w: f32, title: &str, value: &str, sub: &str, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 104.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, Rounding::same(16.0), CARD);
    p.rect_filled(Rect::from_min_size(rect.min + Vec2::new(0.0, 16.0), Vec2::new(4.0, 72.0)), Rounding::same(2.0), color);
    p.circle_filled(rect.right_top() + Vec2::new(-10.0, 10.0), 46.0, color.gamma_multiply(0.10));
    p.text(rect.min + Vec2::new(20.0, 14.0), Align2::LEFT_TOP, title, FontId::proportional(12.5), TEXT_DIM);
    let vsize = if value.chars().count() > 9 { 21.0 } else { 28.0 };
    p.text(rect.min + Vec2::new(20.0, 36.0 + (28.0 - vsize) / 2.0), Align2::LEFT_TOP, value, FontId::proportional(vsize), Color32::WHITE);
    p.text(rect.min + Vec2::new(20.0, 76.0), Align2::LEFT_TOP, sub, FontId::proportional(12.0), color);
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

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Hero
            let w = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 150.0), egui::Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, Rounding::same(22.0), Color32::from_rgb(12, 34, 82));
            let a = (t * 0.4) as f32;
            p.circle_filled(rect.right_top() + Vec2::new(-80.0 + 20.0 * a.sin(), 40.0), 170.0, BLUE.gamma_multiply(0.38));
            p.circle_filled(rect.right_bottom() + Vec2::new(-200.0 + 30.0 * (a * 0.8).cos(), 40.0), 130.0, Color32::from_rgb(110, 70, 240).gamma_multiply(0.25));
            p.circle_filled(rect.left_bottom() + Vec2::new(60.0, 60.0), 110.0, BLUE_LIGHT.gamma_multiply(0.12));
            let (hi, line) = if let Some(c) = ready.first() {
                (format!("{} ist bereit", c.name), "Drück auf Verbinden und leg los.".to_string())
            } else if found > 0 {
                ("Konsole im Ruhemodus".to_string(), "Wecke sie auf – dann kann es losgehen.".to_string())
            } else {
                ("Suche nach deiner Konsole …".to_string(), "Konsole einschalten und mit dem Netzwerk verbinden.".to_string())
            };
            p.text(rect.min + Vec2::new(30.0, 26.0), Align2::LEFT_TOP, "WILLKOMMEN BEI OVERLIME", FontId::proportional(12.5), BLUE_LIGHT);
            p.text(rect.min + Vec2::new(30.0, 48.0), Align2::LEFT_TOP, hi, FontId::proportional(32.0), Color32::WHITE);
            p.text(rect.min + Vec2::new(30.0, 94.0), Align2::LEFT_TOP, line, FontId::proportional(15.0), Color32::from_rgb(190, 205, 235));
            let q = format!(
                "{}  •  {} FPS  •  {} Mbit/s",
                ["540p", "720p", "1080p"][self.settings.resolution],
                [30, 60][self.settings.fps],
                self.settings.bitrate as i32
            );
            p.text(rect.min + Vec2::new(30.0, 120.0), Align2::LEFT_TOP, q, FontId::proportional(12.5), BLUE_LIGHT);

            ui.add_space(14.0);

            // Kacheln
            let tw = ((ui.available_width() - 30.0) / 4.0).max(120.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                tile(ui, tw, "KONSOLEN IM NETZ", &found.to_string(), if found > 0 { "gefunden" } else { "suche …" }, BLUE_LIGHT);
                tile(ui, tw, "GEKOPPELT", &paired_found.to_string(), if paired_found > 0 { "bereit zum Streamen" } else { "noch keine" }, GREEN);
                tile(
                    ui,
                    tw,
                    "CONTROLLER",
                    if pad_ok { "Erkannt" } else { "Keiner" },
                    &if pad_ok { pad_name.clone() } else { "Tastatur möglich".to_string() },
                    if pad_ok { GREEN } else { YELLOW },
                );
                let online = self.local_ip != "?";
                tile(ui, tw, "NETZWERK", if online { "Online" } else { "Offline" }, "lokales Netzwerk", if online { GREEN } else { RED });
            });

            ui.add_space(14.0);

            ui.columns(2, |cols| {
                // Erste Schritte
                egui::Frame::none().fill(CARD).rounding(18.0).inner_margin(egui::Margin::same(20.0)).show(&mut cols[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Erste Schritte").size(17.0).strong());
                    ui.add_space(6.0);
                    check_row(ui, found > 0, "Konsole gefunden", "Konsole an oder im Ruhemodus, gleiches Netzwerk");
                    ui.add_space(4.0);
                    check_row(ui, have_psn, "PlayStation-Konto verbunden", "Einmalige Anmeldung beim Koppeln");
                    ui.add_space(4.0);
                    check_row(ui, !self.paired.is_empty(), "Konsole gekoppelt", "Code von „Gerät koppeln“ eingeben");
                    ui.add_space(4.0);
                    check_row(ui, pad_ok, "Controller angeschlossen", "Optional – es geht auch mit Tastatur");
                });

                // Schnellstart
                egui::Frame::none().fill(CARD).rounding(18.0).inner_margin(egui::Margin::same(20.0)).show(&mut cols[1], |ui| {
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
                                    if ui.add(big_button("Aufwecken", BLUE).min_size(Vec2::new(130.0, 38.0))).clicked() {
                                        self.wake(&c, &pr);
                                    }
                                } else {
                                    let busy = self.starting.is_some();
                                    let label = if busy { "Verbinde …" } else { "▶  Verbinden" };
                                    if ui.add_enabled(!busy, big_button(label, BLUE).min_size(Vec2::new(130.0, 38.0))).clicked() {
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
                        if ui.add(egui::Button::new("Zu den Konsolen").fill(CARD_HOVER).rounding(10.0)).clicked() {
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
            ui.label(
                RichText::new("Tipp: Für die beste Qualität die Konsole per LAN-Kabel anschließen und am PC 5-GHz-WLAN oder Kabel nutzen.")
                    .color(TEXT_DIM)
                    .size(12.5),
            );
        });
    }

    pub fn help_page(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Hilfe & Infos").size(26.0).strong());
        ui.label(RichText::new("Alles, was du für den Start brauchst.").color(TEXT_DIM).size(13.0));
        ui.add_space(8.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            section(ui, "▶  Schnellstart", |ui| {
                step_row(ui, "1", "Konsole einschalten, Remote Play aktivieren (Einstellungen > System > Remote Play).");
                step_row(ui, "2", "In OVERLIME bei der Konsole auf „Koppeln“ klicken und dem Pop-up folgen.");
                step_row(ui, "3", "Danach „Verbinden“ klicken – fertig.");
            });
            ui.add_space(6.0);
            section(ui, "🎮  Steuerung", |ui| {
                ui.label(RichText::new("Controller (PS5, PS4, Xbox, andere – auch per Bluetooth) wird automatisch erkannt, inklusive Vibration.").color(TEXT_DIM));
                ui.label(RichText::new("Menüs: Steuerkreuz oder linker Stick = Auswahl, Kreuz-Taste = Bestätigen, L1/R1 = Seite wechseln.").color(TEXT_DIM).size(12.5));
                ui.add_space(4.0);
                egui::Grid::new("keys").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
                    for (k, v) in [
                        ("W A S D", "Linker Stick"),
                        ("Pfeiltasten", "Steuerkreuz"),
                        ("Leertaste / E / Q / R", "Kreuz / Kreis / Quadrat / Dreieck"),
                        ("Z / X", "L1 / R1"),
                        ("Shift / Strg", "L2 / R2"),
                        ("Tab / Rücktaste / F2", "Options / Share / PS-Taste"),
                        ("F11", "Vollbild"),
                        ("Esc", "Stream beenden"),
                    ] {
                        ui.label(RichText::new(k).monospace().color(BLUE_LIGHT));
                        ui.label(v);
                        ui.end_row();
                    }
                });
            });
            ui.add_space(6.0);
            section(ui, "🛠  Probleme lösen", |ui| {
                for (q, a) in [
                    ("Konsole wird nicht gefunden", "Gleiches Netzwerk? Konsole an oder im Ruhemodus mit „Mit dem Netzwerk verbunden bleiben“. Firewall-Abfrage von Windows erlauben."),
                    ("Kopplung klappt nicht", "Der 8-stellige Code gilt nur kurz. „Gerät koppeln“ an der Konsole neu öffnen und den neuen Code eingeben."),
                    ("Bild ruckelt oder hat Verzögerung", "Konsole per LAN-Kabel, PC per Kabel oder 5 GHz. In den Einstellungen Bitrate senken oder auf 720p stellen."),
                    ("Kein Ton", "Standard-Wiedergabegerät in Windows prüfen."),
                ] {
                    ui.label(RichText::new(q).strong().size(14.0));
                    ui.label(RichText::new(a).color(TEXT_DIM).size(13.0));
                    ui.add_space(4.0);
                }
                ui.add_space(4.0);
                if ui.add(egui::Button::new("Protokoll-Ordner öffnen").fill(CARD_HOVER).rounding(10.0)).clicked() {
                    if let Some(p) = log_path().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
                        let _ = std::fs::create_dir_all(&p);
                        let _ = std::process::Command::new("explorer").arg(p).spawn();
                    }
                }
            });
            ui.add_space(6.0);
            section(ui, "ℹ  Über OVERLIME", |ui| {
                ui.label(RichText::new(format!("Version {VERSION}  •  PS4 & PS5  •  bis 1080p60 (Sony-Limit), Hochskalierung auf 4K")).color(TEXT_DIM));
                ui.label(RichText::new("Der Stream-Kern basiert auf dem Open-Source-Projekt chiaki-ng (AGPL-3.0).").color(TEXT_DIM).size(12.5));
            });
        });
    }
}

impl App {
    /// Erster Start: Anmeldung bei PlayStation. Das Konto wird gespeichert.
    pub fn login_screen(&mut self, ctx: &egui::Context, t: f64) {
        egui::CentralPanel::default().frame(egui::Frame::none().fill(BG)).show(ctx, |ui| {
            let r = ui.max_rect();
            let p = ui.painter();
            let a = (t * 0.3) as f32;
            p.circle_filled(Pos2::new(r.width() * (0.2 + 0.05 * a.sin()), r.height() * 0.25), 320.0, BLUE.gamma_multiply(0.10));
            p.circle_filled(Pos2::new(r.width() * (0.85 + 0.04 * (a * 1.2).cos()), r.height() * 0.8), 360.0, Color32::from_rgb(100, 60, 230).gamma_multiply(0.08));

            ui.vertical_centered(|ui| {
                ui.add_space((r.height() * 0.5 - 250.0).max(10.0));
                ui.label(RichText::new("OVERLIME").font(FontId::proportional(44.0)).strong().color(Color32::WHITE));
                ui.label(RichText::new("REMOTE PLAY").font(FontId::proportional(14.0)).color(BLUE_LIGHT));
                ui.add_space(18.0);
                egui::Frame::none()
                    .fill(CARD)
                    .rounding(22.0)
                    .stroke(Stroke::new(1.0, BLUE.gamma_multiply(0.4)))
                    .inner_margin(egui::Margin::same(28.0))
                    .show(ui, |ui| {
                        ui.set_width(500.0);
                        ui.label(RichText::new("Willkommen!").size(24.0).strong());
                        ui.label(RichText::new("Melde dich einmal mit deinem PlayStation-Konto an. Danach merkt sich die App das und fragt nie wieder.").color(TEXT_DIM));
                        ui.add_space(10.0);
                        step_row(ui, "1", "Auf „Bei PlayStation anmelden“ klicken und im Browser anmelden.");
                        step_row(ui, "2", "Danach die komplette Adresse aus der Adressleiste kopieren.");
                        step_row(ui, "3", "Hier einfügen und auf „Anmelden“ klicken.");
                        ui.add_space(8.0);
                        if ui.add(big_button("Bei PlayStation anmelden", CARD_HOVER).min_size(Vec2::new(444.0, 46.0))).clicked() {
                            ctx.open_url(egui::OpenUrl::new_tab(psn::login_url()));
                        }
                        ui.add(egui::TextEdit::singleline(&mut self.login_pasted).hint_text("Adresse hier einfügen (…redirect?code=…)").desired_width(444.0));
                        if !self.login_error.is_empty() {
                            ui.label(RichText::new(&self.login_error).color(RED).size(13.0));
                        }
                        let code = psn::parse_code(&self.login_pasted);
                        ui.add_space(4.0);
                        if self.login_busy {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new("Melde an …").color(TEXT_DIM));
                            });
                        } else if ui.add_enabled(code.is_some(), big_button("Anmelden", BLUE).min_size(Vec2::new(444.0, 48.0))).clicked() {
                            if let Some(code) = code {
                                self.login_busy = true;
                                self.login_error.clear();
                                let tx = self.tx.clone();
                                std::thread::spawn(move || {
                                    let m = match psn::account_id_from_code(&code) {
                                        Ok((id, name)) => Msg::PsnOk(id, name),
                                        Err(e) => Msg::PsnErr(e),
                                    };
                                    let _ = tx.send(m);
                                });
                            }
                        }
                        ui.add_space(6.0);
                        ui.label(RichText::new("Dein Passwort gibst du nur auf der Seite von Sony ein. Die App speichert nur deine Konto-Nummer, kein Passwort.").color(TEXT_DIM).size(12.0));
                    });
            });
        });
    }
}
