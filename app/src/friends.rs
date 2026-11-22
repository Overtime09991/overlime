//! Seite „Freunde“: dein Code, Freundesliste, Sprachraum und Einladungen.

use super::*;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use voice::{Event as VEvent, Friend};

pub struct Invite {
    pub from: String,
    pub name: String,
    pub room: String,
    pub at: f64,
}

fn avatar(p: &egui::Painter, c: Pos2, r: f32, name: &str, col: Color32, ring: Option<f32>) {
    if let Some(a) = ring {
        p.circle_filled(c, r + 6.0 * a, GREEN.gamma_multiply(0.18 * a));
        p.circle_stroke(c, r + 3.0, Stroke::new(2.0, GREEN.gamma_multiply(a)));
    }
    p.circle_filled(c, r, col);
    let ini = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    p.text(c, Align2::CENTER_CENTER, ini, FontId::proportional(r * 0.95), Color32::WHITE);
}

fn name_color(id: &str) -> Color32 {
    let h: u32 = id.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let pal = [Color32::from_rgb(60, 120, 230), Color32::from_rgb(130, 90, 230), Color32::from_rgb(40, 160, 140), Color32::from_rgb(200, 100, 120), Color32::from_rgb(200, 140, 60)];
    pal[(h as usize) % pal.len()]
}

impl App {
    /// Gespeicherte Freunde (Code, Name)
    pub fn friends_saved(&self) -> Vec<(String, String)> {
        self.settings.voice_friends.split(';').filter_map(|e| {
            let (id, name) = e.split_once('|').unwrap_or((e, ""));
            if voice::valid_id(id) { Some((id.to_string(), name.to_string())) } else { None }
        }).collect()
    }

    fn friends_store(&mut self, list: &[(String, String)]) {
        self.settings.voice_friends = list.iter().map(|(i, n)| format!("{}|{}", i, n.replace([';', '|'], ""))).collect::<Vec<_>>().join(";");
        if let Some(v) = &self.voice {
            v.set_friends(list.iter().map(|(i, _)| i.clone()).collect());
        }
    }

    fn blocked_ids(&self) -> Vec<String> {
        self.settings.voice_blocked.split(',').filter(|s| voice::valid_id(s)).map(|s| s.to_string()).collect()
    }

    /// Startet oder beendet die Verbindung zum Sprachserver je nach Modul und Einstellung. Einmal pro Bild aufrufen.
    pub fn voice_tick(&mut self, ctx: &egui::Context) {
        let want = self.settings.module_on("friends") && self.settings.voice_online;
        if want && self.voice.is_none() {
            if self.settings.voice_secret.is_empty() {
                self.settings.voice_secret = B64.encode(voice::new_secret());
            }
            if let Some(sec) = B64.decode(&self.settings.voice_secret).ok().and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok()) {
                if self.settings.voice_name.trim().is_empty() {
                    self.settings.voice_name = if self.settings.psn_name.is_empty() { "Spieler".to_string() } else { self.settings.psn_name.clone() };
                }
                let v = voice::Voice::start(sec, &self.settings.voice_name, &self.settings.voice_server);
                v.set_friends(self.friends_saved().into_iter().map(|(i, _)| i).collect());
                v.set_blocked(self.blocked_ids());
                self.voice = Some(v);
            }
        }
        if !want && self.voice.is_some() {
            self.voice = None;
            self.v_status = "Offline".into();
            self.v_presence.clear();
            self.v_room = None;
        }
        let Some(v) = self.voice.as_ref() else { return };
        let now = ctx.input(|i| i.time);
        let mut start_audio = false;
        let mut stop_audio = false;
        for e in v.poll() {
            match e {
                VEvent::Connected(_) => self.v_status = "Verbunden".into(),
                VEvent::Disconnected(r) => {
                    self.v_status = r;
                    self.v_presence.clear();
                }
                VEvent::Presence(l) => {
                    // Namen der Freunde merken
                    let mut saved = self.friends_saved();
                    let mut changed = false;
                    for f in &l {
                        if let Some(s) = saved.iter_mut().find(|(i, _)| *i == f.id) {
                            if s.1 != f.name && !f.name.is_empty() {
                                s.1 = f.name.clone();
                                changed = true;
                            }
                        }
                    }
                    self.v_presence = l;
                    if changed {
                        self.settings.voice_friends = saved.iter().map(|(i, n)| format!("{}|{}", i, n.replace([';', '|'], ""))).collect::<Vec<_>>().join(";");
                    }
                }
                VEvent::Room { room, members } => {
                    if room.is_empty() {
                        if self.v_room.is_some() {
                            stop_audio = true;
                        }
                        self.v_room = None;
                    } else {
                        if self.v_room.is_none() {
                            start_audio = true;
                        }
                        self.v_room = Some((room, members));
                    }
                }
                VEvent::Invite { from, name, room } => {
                    if !self.blocked_ids().contains(&from) {
                        self.v_invites.retain(|i| i.from != from);
                        self.v_invites.push(Invite { from, name: name.clone(), room, at: now });
                        self.toasts.push((format!("{name} lädt dich in einen Sprachraum ein"), 2, self.now));
                    }
                }
                VEvent::Error(m) => self.v_status = m,
            }
        }
        self.v_invites.retain(|i| now - i.at < 120.0);
        let v = self.voice.as_ref().unwrap();
        if self.v_room.is_none() && v.connected() && !self.v_demo_joined && std::env::args().any(|a| a == "--demo-room") {
            self.v_demo_joined = true;
            v.join(None);
        }
        if start_audio {
            if let Err(e) = v.start_audio(&self.settings.mic_device, self.settings.mic_gain, self.settings.mic_gate, self.settings.chat_denoise, &self.settings.audio_device, self.settings.chat_vol) {
                self.toasts.push((format!("Sprachchat: {e}"), 1, self.now));
            }
        }
        if stop_audio {
            v.stop_audio();
        }
        // Sprechen: offenes Mikrofon oder Taste B halten
        if self.v_room.is_some() {
            let b_down = !ctx.wants_keyboard_input() && ctx.input(|i| i.key_down(Key::B));
            let talk = if self.settings.chat_ptt { b_down } else { !self.v_muted };
            v.set_talking(talk);
            if !self.settings.chat_ptt && !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(Key::B)) {
                self.v_muted = !self.v_muted;
                self.toasts.push((if self.v_muted { "Sprachchat: stumm".to_string() } else { "Sprachchat: Mikrofon an".to_string() }, 0, self.now));
            }
            v.update_mic(self.settings.mic_gain, self.settings.mic_gate, self.settings.chat_denoise);
            v.set_volume(self.settings.chat_vol);
            ctx.request_repaint_after(Duration::from_millis(60));
        }
    }

    /// Einladungen als Hinweis oben mittig (auf jeder Seite und im Stream).
    pub fn invites_overlay(&mut self, ctx: &egui::Context) {
        if self.v_invites.is_empty() {
            return;
        }
        let mut accept: Option<usize> = None;
        let mut decline: Option<usize> = None;
        egui::Area::new(egui::Id::new("v_invites")).order(egui::Order::Tooltip).anchor(Align2::CENTER_TOP, [0.0, 54.0]).show(ctx, |ui| {
            for (i, inv) in self.v_invites.iter().enumerate() {
                egui::Frame::none().fill(th_card()).rounding(14.0).stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.7))).inner_margin(egui::Margin::symmetric(16.0, 12.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(ic::MICROPHONE).size(20.0).color(th_blue_light()));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("{} lädt dich ein", inv.name)).size(14.5).strong());
                            ui.label(RichText::new("Sprachraum beitreten").color(TEXT_DIM).size(12.0));
                        });
                        ui.add_space(14.0);
                        if ui.add(egui::Button::new(RichText::new("Beitreten").color(Color32::WHITE)).fill(th_blue()).rounding(10.0)).clicked() {
                            accept = Some(i);
                        }
                        if ui.add(egui::Button::new("Ablehnen").rounding(10.0)).clicked() {
                            decline = Some(i);
                        }
                    });
                });
                ui.add_space(6.0);
            }
        });
        if let Some(i) = accept {
            let inv = self.v_invites.remove(i);
            if let Some(v) = &self.voice {
                v.join(Some(inv.room));
            }
        } else if let Some(i) = decline {
            self.v_invites.remove(i);
        }
    }

    /// Kleine Anzeige im Stream: wer ist im Raum, wer spricht.
    pub fn voice_stream_overlay(&self, ctx: &egui::Context) {
        let (Some(v), Some((_, members))) = (self.voice.as_ref(), self.v_room.as_ref()) else { return };
        let speaking = v.speaking_idx();
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("v_overlay")));
        let sc = ctx.screen_rect();
        let mut y = sc.top() + 64.0;
        for m in members {
            let me = m.id == v.my_id;
            let talk = if me { v.talking() && v.mic_level() > 0.01 } else { speaking.contains(&m.idx) };
            let g = p.layout_no_wrap(m.name.clone(), FontId::proportional(13.0), Color32::WHITE);
            let r = Rect::from_min_size(Pos2::new(sc.right() - 24.0 - g.size().x - 44.0, y), Vec2::new(g.size().x + 44.0, 28.0));
            p.rect_filled(r, Rounding::same(14.0), Color32::from_black_alpha(if talk { 190 } else { 120 }));
            p.circle_filled(Pos2::new(r.left() + 14.0, r.center().y), 5.0, if talk { GREEN } else { Color32::from_white_alpha(70) });
            p.galley(Pos2::new(r.left() + 26.0, r.center().y - g.size().y / 2.0), g, Color32::WHITE);
            y += 32.0;
        }
    }

    pub fn friends_page(&mut self, ui: &mut egui::Ui, t: f64) {
        if !self.settings.module_on("friends") {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(ic::PUZZLE_PIECE).size(54.0).color(Color32::from_white_alpha(60)));
                ui.add_space(8.0);
                ui.label(RichText::new("Das Modul „Freunde & Sprache“ ist ausgeschaltet").size(20.0));
                ui.add_space(10.0);
                if ui.add(egui::Button::new("Einschalten").fill(th_blue()).rounding(10.0).min_size(Vec2::new(160.0, 38.0))).clicked() {
                    self.settings.set_module("friends", true);
                }
            });
            return;
        }
        if !self.settings.voice_online {
            ui.label(RichText::new("Freunde & Sprache").size(28.0));
            ui.add_space(8.0);
            egui::Frame::none().fill(th_card()).rounding(16.0).inner_margin(egui::Margin::same(20.0)).show(ui, |ui| {
                ui.set_max_width(620.0);
                ui.label(RichText::new(ic::USERS_THREE).size(40.0).color(th_blue_light()));
                ui.add_space(6.0);
                ui.label(RichText::new("Mit Freunden sprechen").size(20.0).strong());
                ui.add_space(4.0);
                ui.label(RichText::new("Du bekommst einen persönlichen Code, trägst die Codes deiner Freunde ein und sprichst in Sprachräumen mit ihnen, auch während du spielst. Gesehen wird nichts, nur gehört.").color(TEXT_DIM).size(13.5));
                ui.add_space(10.0);
                ui.label(RichText::new("Was dabei passiert").size(14.0).strong());
                for line in [
                    "OVERLIME verbindet sich mit dem Sprachserver von OVERLIME (voice.overlime.info).",
                    "Dorthin gehen dein Code, dein Anzeigename und die Codes deiner Freunde, damit ihr euch findet.",
                    "Deine Sprache wird nur durchgeleitet und nie aufgezeichnet oder gespeichert.",
                    "Du bist nur online, solange das hier eingeschaltet ist. Ausschalten geht jederzeit.",
                ] {
                    ui.label(RichText::new(format!("•  {}", lang::tr(line))).color(Color32::from_rgb(200, 210, 235)).size(13.0));
                }
                ui.add_space(12.0);
                if ui.add(big_button("Einschalten", th_blue()).min_size(Vec2::new(220.0, 46.0))).clicked() {
                    self.settings.voice_online = true;
                }
            });
            return;
        }
        let my_id = self.voice.as_ref().map(|v| v.my_id.clone()).unwrap_or_else(|| {
            B64.decode(&self.settings.voice_secret).ok().and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok()).map(|s| voice::id_from_secret(&s)).unwrap_or_default()
        });
        let connected = self.voice.as_ref().map(|v| v.connected()).unwrap_or(false);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Freunde & Sprache").size(28.0));
                ui.label(RichText::new("Sprich mit deinen Freunden, auch während du spielst. Gesehen wird nichts, nur gehört.").color(TEXT_DIM).size(13.0));
            });
        });
        ui.add_space(10.0);
        let mut leave = false;
        let mut start_room = false;
        let mut add_friend: Option<String> = None;
        let mut remove: Option<String> = None;
        let mut block: Option<String> = None;
        let mut unblock: Option<String> = None;
        let mut invite: Option<String> = None;
        let mut call: Option<String> = None;
        let mut apply_name = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let wide = ui.available_width() > 760.0;
            let left_w = if wide { (ui.available_width() - 16.0) * 0.56 } else { ui.available_width() };
            ui.horizontal_top(|ui| {
                // ---------------- links: Code, Freunde
                ui.vertical(|ui| {
                    ui.set_width(left_w);
                    egui::Frame::none().fill(th_card()).rounding(16.0).inner_margin(egui::Margin::same(16.0)).show(ui, |ui| {
                        ui.set_width(left_w - 32.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("DEIN CODE").size(11.5).color(th_blue_light()));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let (txt, col) = if connected { ("Verbunden".to_string(), GREEN) } else if self.settings.voice_online { (lang::tr(&self.v_status), YELLOW) } else { ("Offline".to_string(), TEXT_DIM) };
                                ui.label(RichText::new(txt).size(12.0).color(col));
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(if my_id.is_empty() { "…".to_string() } else { my_id.clone() }).monospace().size(28.0).color(Color32::WHITE));
                            if !my_id.is_empty() && ui.add(egui::Button::new(format!("{}  Kopieren", ic::COPY)).fill(th_card_hover()).rounding(10.0)).clicked() {
                                ui.ctx().copy_text(my_id.clone());
                                self.toasts.push(("Code kopiert. Schick ihn deinen Freunden.".to_string(), 2, self.now));
                            }
                        });
                        ui.label(RichText::new("Gib deinen Code weiter. Ihr seht euch erst online, wenn ihr beide den Code des anderen eingetragen habt.").color(TEXT_DIM).size(12.0));
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label("Dein Name");
                            ui.add(egui::TextEdit::singleline(&mut self.settings.voice_name).desired_width(170.0).char_limit(24));
                            if ui.add(egui::Button::new("Übernehmen").fill(th_card_hover()).rounding(10.0)).clicked() {
                                apply_name = true;
                            }
                        });
                        ui.add_space(4.0);
                        ui.checkbox(&mut self.settings.voice_online, "Online sein (Freunde sehen mich)");
                    });
                    ui.add_space(12.0);
                    egui::Frame::none().fill(th_card()).rounding(16.0).inner_margin(egui::Margin::same(16.0)).show(ui, |ui| {
                        ui.set_width(left_w - 32.0);
                        ui.label(RichText::new("Freund hinzufügen").size(15.0).strong());
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.v_add).hint_text("Code, zum Beispiel OL-ABCD-EFGH").desired_width(240.0).char_limit(14));
                            if ui.add(egui::Button::new(RichText::new("Hinzufügen").color(Color32::WHITE)).fill(th_blue()).rounding(10.0)).clicked() {
                                let id = self.v_add.trim().to_uppercase();
                                if !voice::valid_id(&id) {
                                    self.v_add_msg = "Das ist kein gültiger Code.".into();
                                } else if id == my_id {
                                    self.v_add_msg = "Das ist dein eigener Code.".into();
                                } else {
                                    add_friend = Some(id);
                                }
                            }
                        });
                        if !self.v_add_msg.is_empty() {
                            ui.label(RichText::new(lang::tr(&self.v_add_msg)).color(YELLOW).size(12.5));
                        }
                    });
                    ui.add_space(12.0);
                    ui.label(RichText::new("Deine Freunde").size(15.0).strong());
                    ui.add_space(4.0);
                    let saved = self.friends_saved();
                    if saved.is_empty() {
                        ui.label(RichText::new("Noch keine Freunde. Trag oben den Code eines Freundes ein und schick ihm deinen Code.").color(TEXT_DIM).size(12.5));
                    }
                    let pres: Vec<Friend> = self.v_presence.clone();
                    let blocked = self.blocked_ids();
                    let in_room = self.v_room.is_some();
                    for (id, name) in &saved {
                        let online = pres.iter().find(|f| f.id == *id);
                        let is_blocked = blocked.contains(id);
                        egui::Frame::none().fill(Color32::from_rgba_unmultiplied(255, 255, 255, 8)).rounding(12.0).inner_margin(egui::Margin::symmetric(12.0, 8.0)).show(ui, |ui| {
                            ui.set_width(left_w - 24.0);
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(Vec2::splat(38.0), egui::Sense::hover());
                                let shown = online.map(|f| f.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| if name.is_empty() { id.clone() } else { name.clone() });
                                avatar(ui.painter(), r.center(), 18.0, &shown, if online.is_some() { name_color(id) } else { Color32::from_white_alpha(30) }, None);
                                if online.is_some() {
                                    ui.painter().circle_filled(r.right_bottom() - Vec2::new(4.0, 4.0), 6.0, GREEN);
                                    ui.painter().circle_stroke(r.right_bottom() - Vec2::new(4.0, 4.0), 6.0, Stroke::new(2.0, th_card()));
                                }
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(shown).size(14.0).strong());
                                    let st = if is_blocked { lang::tr("Blockiert") } else if let Some(f) = online { if f.in_room { lang::tr("Im Sprachraum") } else { lang::tr("Online") } } else { lang::tr("Offline") };
                                    ui.label(RichText::new(format!("{}  •  {}", id, st)).color(TEXT_DIM).size(11.5));
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.add(egui::Button::new(ic::TRASH).rounding(8.0)).on_hover_text("Entfernen").clicked() {
                                        remove = Some(id.clone());
                                    }
                                    if ui.add(egui::Button::new(if is_blocked { ic::LOCK_OPEN } else { ic::PROHIBIT }).rounding(8.0)).on_hover_text(if is_blocked { "Entblocken" } else { "Blockieren" }).clicked() {
                                        if is_blocked { unblock = Some(id.clone()) } else { block = Some(id.clone()) }
                                    }
                                    if let (Some(_), false) = (online, is_blocked) {
                                        if in_room {
                                            if ui.add(egui::Button::new(RichText::new("Einladen").color(Color32::WHITE)).fill(th_blue()).rounding(8.0)).clicked() {
                                                invite = Some(id.clone());
                                            }
                                        } else if ui.add(egui::Button::new(RichText::new(format!("{}  Anrufen", ic::MICROPHONE)).color(Color32::WHITE)).fill(th_blue()).rounding(8.0)).clicked() {
                                            call = Some(id.clone());
                                        }
                                    }
                                });
                            });
                        });
                        ui.add_space(4.0);
                    }
                });
                // ---------------- rechts: Sprachraum
                if wide {
                    ui.add_space(16.0);
                }
                ui.vertical(|ui| {
                    let rw = if wide { ui.available_width().max(260.0) } else { left_w };
                    ui.set_width(rw);
                    if !wide {
                        ui.add_space(12.0);
                    }
                    let in_room = self.v_room.is_some();
                    egui::Frame::none().fill(th_card()).rounding(16.0).stroke(Stroke::new(1.0, if in_room { GREEN.gamma_multiply(0.5) } else { Color32::from_white_alpha(14) })).inner_margin(egui::Margin::same(16.0)).show(ui, |ui| {
                        ui.set_width(rw - 32.0);
                        ui.label(RichText::new(format!("{}  Sprachraum", ic::MICROPHONE)).size(15.0).strong());
                        if !in_room {
                            ui.add_space(4.0);
                            ui.label(RichText::new("Du bist in keinem Raum. Starte einen und lade Freunde ein, oder ruf einen Freund direkt an.").color(TEXT_DIM).size(12.5));
                            ui.add_space(8.0);
                            if ui.add_enabled(connected, big_button("Raum starten", th_blue()).min_size(Vec2::new(rw - 32.0, 44.0))).clicked() {
                                start_room = true;
                            }
                            if !connected {
                                ui.label(RichText::new("Keine Verbindung zum Sprachserver.").color(YELLOW).size(12.0));
                            }
                        } else if let (Some(v), Some((_, members))) = (self.voice.as_ref(), self.v_room.clone()) {
                            let speaking = v.speaking_idx();
                            ui.add_space(6.0);
                            for m in &members {
                                let me = m.id == v.my_id;
                                let talk = if me { v.talking() && v.mic_level() > 0.01 } else { speaking.contains(&m.idx) };
                                let a = ui.ctx().animate_bool_with_time(egui::Id::new(("v_talk", m.idx)), talk, 0.15);
                                ui.horizontal(|ui| {
                                    let (r, _) = ui.allocate_exact_size(Vec2::splat(44.0), egui::Sense::hover());
                                    avatar(ui.painter(), r.center(), 18.0, &m.name, name_color(&m.id), Some(a));
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(format!("{}{}", m.name, if me { " (du)" } else { "" })).size(14.0).strong());
                                        ui.label(RichText::new(if talk { lang::tr("spricht") } else { lang::tr("hört zu") }).color(if talk { GREEN } else { TEXT_DIM }).size(11.5));
                                    });
                                });
                                ui.add_space(2.0);
                            }
                            ui.add_space(8.0);
                            // Mikrofon
                            let talking = v.talking();
                            if self.settings.chat_ptt {
                                ui.label(RichText::new(format!("{} Taste B gedrückt halten zum Sprechen", ic::KEYBOARD)).color(if talking { GREEN } else { TEXT_DIM }).size(12.5));
                            } else if ui.add(egui::Button::new(RichText::new(if self.v_muted { format!("{}  Stumm (B)", ic::MICROPHONE_SLASH) } else { format!("{}  Mikrofon an (B)", ic::MICROPHONE) }).color(Color32::WHITE)).fill(if self.v_muted { RED.gamma_multiply(0.7) } else { th_card_hover() }).rounding(10.0).min_size(Vec2::new(rw - 32.0, 38.0))).clicked() {
                                self.v_muted = !self.v_muted;
                            }
                            let (rb, _) = ui.allocate_exact_size(Vec2::new(rw - 32.0, 8.0), egui::Sense::hover());
                            ui.painter().rect_filled(rb, Rounding::same(4.0), Color32::from_white_alpha(16));
                            let lv = if talking { v.mic_level().sqrt().clamp(0.0, 1.0) } else { 0.0 };
                            ui.painter().rect_filled(Rect::from_min_size(rb.min, Vec2::new((rb.width() * lv).max(3.0), rb.height())), Rounding::same(4.0), GREEN);
                            if let Some(e) = v.mic_info_error() {
                                ui.label(RichText::new(format!("Mikrofon: {e}")).color(RED).size(12.0));
                            }
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label("Lautstärke");
                                ui.spacing_mut().slider_width = (rw - 190.0).max(80.0);
                                ui.add(egui::Slider::new(&mut self.settings.chat_vol, 0.0..=2.0).custom_formatter(|x, _| format!("{} %", (x * 100.0).round() as i32)));
                            });
                            ui.add_space(8.0);
                            if ui.add(egui::Button::new(RichText::new("Raum verlassen").color(Color32::WHITE)).fill(RED.gamma_multiply(0.8)).rounding(10.0).min_size(Vec2::new(rw - 32.0, 38.0))).clicked() {
                                leave = true;
                            }
                        }
                    });
                    ui.add_space(12.0);
                    egui::Frame::none().fill(th_card()).rounding(16.0).inner_margin(egui::Margin::same(16.0)).show(ui, |ui| {
                        ui.set_width(rw - 32.0);
                        ui.label(RichText::new("Einstellungen").size(14.0).strong());
                        ui.add_space(4.0);
                        let mut m = if self.settings.chat_ptt { 1 } else { 0 };
                        segmented(ui, &mut m, &["Mikrofon offen", "Taste B halten"]);
                        self.settings.chat_ptt = m == 1;
                        ui.add_space(6.0);
                        ui.checkbox(&mut self.settings.chat_denoise, "Rauschfilter (KI): Tastatur, Lüfter und Hintergrund herausfiltern");
                        ui.label(RichText::new("Ein kleines neuronales Netz (RNNoise) trennt deine Stimme vom Rauschen, ähnlich wie Krisp. Es läuft auf deinem PC, nichts geht an einen Dienst.").color(TEXT_DIM).size(11.5));
                        ui.add_space(4.0);
                        ui.label(RichText::new("Mikrofon, Lautstärke und Rauschsperre stellst du unter Einstellungen > Sprache ein.").color(TEXT_DIM).size(11.5));
                    });
                });
            });
            let _ = t;
        });

        // Aktionen auswerten
        if apply_name {
            self.settings.voice_name = self.settings.voice_name.trim().to_string();
            if let Some(v) = &self.voice {
                v.set_name(&self.settings.voice_name);
            }
            self.toasts.push(("Name übernommen.".to_string(), 2, self.now));
        }
        if let Some(id) = add_friend {
            let mut list = self.friends_saved();
            if list.iter().any(|(i, _)| *i == id) {
                self.v_add_msg = "Der Freund ist schon in deiner Liste.".into();
            } else {
                list.push((id, String::new()));
                self.friends_store(&list);
                self.v_add.clear();
                self.v_add_msg.clear();
            }
        }
        if let Some(id) = remove {
            let list: Vec<(String, String)> = self.friends_saved().into_iter().filter(|(i, _)| *i != id).collect();
            self.friends_store(&list);
        }
        if let Some(id) = block {
            let mut b = self.blocked_ids();
            b.push(id);
            self.settings.voice_blocked = b.join(",");
            if let Some(v) = &self.voice {
                v.set_blocked(self.blocked_ids());
            }
        }
        if let Some(id) = unblock {
            let b: Vec<String> = self.blocked_ids().into_iter().filter(|i| *i != id).collect();
            self.settings.voice_blocked = b.join(",");
            if let Some(v) = &self.voice {
                v.set_blocked(self.blocked_ids());
            }
        }
        if start_room {
            if let Some(v) = &self.voice {
                v.join(None);
            }
        }
        if let Some(id) = call {
            if let Some(v) = &self.voice {
                v.join(None);
                self.v_pending_invite = Some(id);
            }
        }
        if let Some(id) = invite {
            if let Some(v) = &self.voice {
                v.invite(&id);
                self.toasts.push(("Einladung gesendet.".to_string(), 2, self.now));
            }
        }
        if leave {
            if let Some(v) = &self.voice {
                v.leave();
            }
            self.v_room = None;
        }
        // Anruf: sobald der Raum steht, den Freund einladen
        if self.v_room.is_some() {
            if let Some(id) = self.v_pending_invite.take() {
                if let Some(v) = &self.voice {
                    v.invite(&id);
                    self.toasts.push(("Einladung gesendet.".to_string(), 2, self.now));
                }
            }
        }
    }
}
