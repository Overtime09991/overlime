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
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 150.0), egui::Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, Rounding::same(22.0), Color32::from_rgb(12, 34, 82));
            let a = (t * 0.4) as f32;
            p.circle_filled(rect.right_top() + Vec2::new(-80.0 + 20.0 * a.sin(), 40.0), 170.0, th_blue().gamma_multiply(0.38));
            p.circle_filled(rect.right_bottom() + Vec2::new(-200.0 + 30.0 * (a * 0.8).cos(), 40.0), 130.0, Color32::from_rgb(110, 70, 240).gamma_multiply(0.25));
            p.circle_filled(rect.left_bottom() + Vec2::new(60.0, 60.0), 110.0, th_blue_light().gamma_multiply(0.12));
            let (hi, line) = if let Some(c) = ready.first() {
                (format!("{} ist bereit", c.name), "Drück auf Verbinden und leg los.".to_string())
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
                    &if pad_ok { pad_name.clone() } else { "Tastatur möglich".to_string() },
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
                    check_row(ui, pad_ok, "Controller angeschlossen", "Optional – es geht auch mit Tastatur");
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
            section(ui, &format!("{}  Schnellstart", ic::ROCKET_LAUNCH), |ui| {
                step_row(ui, "1", "Konsole einschalten, Remote Play aktivieren (Einstellungen > System > Remote Play).");
                step_row(ui, "2", "In OVERLIME bei der Konsole auf „Koppeln“ klicken und dem Pop-up folgen.");
                step_row(ui, "3", "Danach „Verbinden“ klicken – fertig.");
            });
            ui.add_space(6.0);
            section(ui, &format!("{}  Steuerung", ic::GAME_CONTROLLER), |ui| {
                ui.label(RichText::new("Controller (PS5, PS4, Xbox, andere – auch per Bluetooth) wird automatisch erkannt, inklusive Vibration.").color(TEXT_DIM));
                ui.label(RichText::new("Menüs: Steuerkreuz oder linker Stick = Auswahl, Kreuz-Taste = Bestätigen, L1/R1 = Seite wechseln.").color(TEXT_DIM).size(12.5));
                ui.add_space(4.0);
                egui::Grid::new("keys").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
                    for (i, act) in keys::ACTIONS.iter().enumerate() {
                        ui.label(RichText::new(keys::pretty(&self.settings.keys[i])).monospace().color(th_blue_light()));
                        ui.label(act.label);
                        ui.end_row();
                    }
                    for (k, v) in [
                        ("Esc", "Menü im Stream (beendet nichts aus Versehen)"),
                        ("F1", "Tastenbelegung im Stream anzeigen"),
                        ("F10", "Statusanzeige ein/aus"),
                        ("F11", "Vollbild"),
                        ("F5", "Konsolen neu suchen"),
                        ("Strg + 1 bis 4", "Seite wechseln"),
                    ] {
                        ui.label(RichText::new(k).monospace().color(GREEN));
                        ui.label(v);
                        ui.end_row();
                    }
                });
            });
            ui.add_space(6.0);
            section(ui, &format!("{}  Probleme lösen", ic::WRENCH), |ui| {
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
                if ui.add(egui::Button::new("Protokoll-Ordner öffnen").fill(th_card_hover()).rounding(10.0)).clicked() {
                    if let Some(p) = log_path().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
                        let _ = std::fs::create_dir_all(&p);
                        let _ = std::process::Command::new("explorer").arg(p).spawn();
                    }
                }
            });
            ui.add_space(6.0);
            section(ui, &format!("{}  Über OVERLIME", ic::INFO), |ui| {
                ui.label(RichText::new(format!("Version {VERSION}  •  PS4 & PS5  •  bis 1080p60 (Sony-Limit), Hochskalierung auf 4K")).color(TEXT_DIM));
                ui.label(RichText::new("Der Stream-Kern basiert auf dem Open-Source-Projekt chiaki-ng (AGPL-3.0).").color(TEXT_DIM).size(12.5));
            });
        });
    }
}

impl App {
    /// Erster Start: Anmeldung bei PlayStation. Das Konto wird gespeichert.
    pub fn login_screen(&mut self, ctx: &egui::Context, t: f64) {
        egui::CentralPanel::default().frame(egui::Frame::none().fill(th_bg())).show(ctx, |ui| {
            let r = ui.max_rect();
            let p = ui.painter();
            let a = (t * 0.3) as f32;
            p.circle_filled(Pos2::new(r.width() * (0.2 + 0.05 * a.sin()), r.height() * 0.25), 320.0, th_blue().gamma_multiply(0.10));
            p.circle_filled(Pos2::new(r.width() * (0.85 + 0.04 * (a * 1.2).cos()), r.height() * 0.8), 360.0, Color32::from_rgb(100, 60, 230).gamma_multiply(0.08));

            ui.vertical_centered(|ui| {
                ui.add_space((r.height() * 0.5 - 250.0).max(10.0));
                ui.label(RichText::new("OVERLIME").font(FontId::proportional(44.0)).strong().color(Color32::WHITE));
                ui.label(RichText::new("REMOTE PLAY").font(FontId::proportional(14.0)).color(th_blue_light()));
                ui.add_space(18.0);
                egui::Frame::none()
                    .fill(th_card())
                    .rounding(22.0)
                    .stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.4)))
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
                        if ui.add(big_button("Bei PlayStation anmelden", th_card_hover()).min_size(Vec2::new(444.0, 46.0))).clicked() {
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
                        } else if ui.add_enabled(code.is_some(), big_button("Anmelden", th_blue()).min_size(Vec2::new(444.0, 48.0))).clicked() {
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
                        ui.vertical_centered(|ui| {
                            if ui.add(egui::Button::new(RichText::new("Später anmelden").color(TEXT_DIM)).frame(false)).clicked() {
                                self.skip_login = true;
                            }
                        });
                        ui.label(RichText::new("Dein Passwort gibst du nur auf der Seite von Sony ein. Die App speichert nur deine Konto-Nummer, kein Passwort.").color(TEXT_DIM).size(12.0));
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

impl App {
    pub fn diag_page(&mut self, ui: &mut egui::Ui, t: f64) {
        let online = self.local_ip != "?";
        let found = self.consoles.len();
        let first = self.consoles.first().cloned();
        let paired_any = self.consoles.iter().any(|c| self.paired_for(c).is_some());
        let psn_ok = psn::id_from_b64(&self.settings.psn_account).is_some();
        let pad_ok = self.pad.connected();
        let ps = self.page_since;
        let _ = (t, ps);

        ui.horizontal(|ui| {
            ui.label(RichText::new("Diagnose").size(26.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Button::new(format!("{} Problem melden", ic::BUG)).fill(th_card_hover()).rounding(10.0)).clicked() {
                    ui.ctx().copy_text(self.diag_report());
                    let url = format!(
                        "https://github.com/Overtime09991/overlime/issues/new?title={}&body={}",
                        psn::enc(&format!("Problem mit OVERLIME {VERSION}")),
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
        ui.label(RichText::new("Prüft, ob alles bereit ist, und zeigt dir bei Problemen, woran es liegt.").color(TEXT_DIM).size(13.0));
        ui.add_space(8.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
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
                status_row(ui, if pad_ok { 0 } else { 1 }, "Controller", if pad_ok { "Ein Controller ist verbunden." } else { "Kein Controller erkannt. Das ist optional, es geht auch mit Tastatur." });
                ui.add_space(6.0);
                let (us, ut) = if self.update.is_some() { (1, "Ein Update ist verfügbar.") } else if self.update_checked { (0, "OVERLIME ist auf dem neuesten Stand.") } else { (1, "Noch nicht geprüft.") };
                status_row(ui, us, "Version", &format!("{VERSION}. {ut}"));
            });
            ui.add_space(6.0);
            section(ui, &format!("{}  Controller", ic::GAME_CONTROLLER), |ui| {
                let conn = self.pad.connected();
                let inp = self.pad.poll();
                let ex = self.pad.poll_extra();
                ui.label(RichText::new("Live-Test: Drücke Tasten und bewege Sticks. Was hier reagiert, kommt auch im Spiel an.").color(TEXT_DIM).size(12.5));
                ui.add_space(4.0);
                pad_visual(ui, &inp, &ex, conn);
                ui.ctx().request_repaint();
                ui.add_space(8.0);
                let devs = self.pad.devices();
                if devs.is_empty() {
                    status_row(ui, 1, "Kein Gerät gefunden", "Windows meldet keinen Controller. Per Bluetooth koppeln (Windows: Einstellungen > Bluetooth und Geräte), dann hier „Neu suchen“ klicken.");
                } else {
                    for (name, usable, vid, pid) in &devs {
                        status_row(ui, if *usable { 0 } else { 1 }, &format!("{name}  ({vid:04x}:{pid:04x})"), if *usable { "Als Gamepad erkannt." } else { "Gerät gefunden, aber nicht als Gamepad nutzbar." });
                        ui.add_space(4.0);
                    }
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(format!("{} Controller neu suchen", ic::ARROWS_CLOCKWISE)).fill(th_card_hover()).rounding(10.0)).clicked() {
                        self.pad.rescan();
                        self.toasts.push(("Controller wird neu gesucht.".to_string(), 0, self.now));
                    }
                });
                ui.add_space(4.0);
                ui.label(RichText::new("PS4-Controller koppeln: Share- und PS-Taste halten, bis die Leiste schnell blinkt. PS5-Controller: Create- und PS-Taste halten. Danach in Windows unter Bluetooth hinzufügen.").color(TEXT_DIM).size(12.5));
            });
            ui.add_space(6.0);
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
                                let r = match format!("{addr}:9295").parse::<std::net::SocketAddr>() {
                                    Ok(sa) => match std::net::TcpStream::connect_timeout(&sa, Duration::from_millis(2500)) {
                                        Ok(_) => (true, "Der Remote-Play-Port der Konsole ist erreichbar.".to_string()),
                                        Err(e) => (false, format!("Der Port ist nicht erreichbar ({:?}). Ist Remote Play an der Konsole aktiviert und die Konsole an? Blockiert eine Firewall?", e.kind())),
                                    },
                                    Err(_) => (false, "Ungültige Adresse".to_string()),
                                };
                                let _ = tx.send(Msg::DiagPort(r.0, r.1));
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
            });
            ui.add_space(6.0);
            section(ui, &format!("{}  Wenn etwas nicht klappt", ic::INFO), |ui| {
                ui.label(RichText::new("Klicke oben auf „Problem melden“. OVERLIME kopiert einen Bericht (ohne IP-Adressen und Passwörter) und öffnet die Meldeseite auf GitHub. Dort fügst du ihn mit Strg+V ein und beschreibst kurz, was passiert ist.").color(TEXT_DIM).size(13.0));
            });
        });
    }
}
