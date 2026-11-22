//! Galerie: alle Aufnahmen und Screenshots an einem Ort, mit Vorschaubildern, Abspielen in der App und Teilen.

use super::*;
use std::path::PathBuf;

pub enum Thumb {
    Loading,
    Ready(egui::TextureHandle, i64),
    Failed,
}

pub struct ThumbMsg {
    pub path: PathBuf,
    pub img: Option<(usize, usize, Vec<u8>, i64)>,
}

fn downscale(src: &[u8], w: usize, h: usize, tw: usize) -> (usize, usize, Vec<u8>) {
    if w <= tw {
        return (w, h, src.to_vec());
    }
    let th = (h * tw / w).max(1);
    let mut out = vec![0u8; tw * th * 4];
    for y in 0..th {
        let sy = y * h / th;
        let sy2 = ((y * h + h / 2) / th).min(h - 1);
        for x in 0..tw {
            let sx = x * w / tw;
            let sx2 = ((x * w + w / 2) / tw).min(w - 1);
            for c in 0..4 {
                let a = src[(sy * w + sx) * 4 + c] as u32;
                let b = src[(sy * w + sx2) * 4 + c] as u32;
                let d = src[(sy2 * w + sx) * 4 + c] as u32;
                let e = src[(sy2 * w + sx2) * 4 + c] as u32;
                out[(y * tw + x) * 4 + c] = ((a + b + d + e) / 4) as u8;
            }
        }
    }
    (tw, th, out)
}

fn fmt_time(ms: i64) -> String {
    let s = (ms.max(0) / 1000) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn fmt_size(b: u64) -> String {
    if b >= 1_073_741_824 {
        format!("{:.1} GB", b as f64 / 1_073_741_824.0)
    } else {
        format!("{:.1} MB", b as f64 / 1_048_576.0)
    }
}

/// Startet einen Windows-Befehl ohne sichtbares Fenster.
fn run_hidden(cmd: &str, args: &[&str]) {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new(cmd).args(args).creation_flags(0x0800_0000).spawn();
}

impl App {
    fn gallery_items(&self) -> Vec<MediaItem> {
        self.gal_items
            .iter()
            .filter(|m| match self.gal_filter {
                1 => m.video,
                2 => !m.video,
                _ => true,
            })
            .cloned()
            .collect()
    }

    fn gallery_poll_thumbs(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.gal_rx.try_recv() {
            self.gal_inflight = self.gal_inflight.saturating_sub(1);
            match m.img {
                Some((w, h, rgba, dur)) => {
                    let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                    let tex = ctx.load_texture(format!("th_{}", m.path.display()), img, TextureOptions::LINEAR);
                    self.gal_thumbs.insert(m.path, Thumb::Ready(tex, dur));
                }
                None => {
                    self.gal_thumbs.insert(m.path, Thumb::Failed);
                }
            }
        }
    }

    fn gallery_request_thumb(&mut self, m: &MediaItem) {
        if self.gal_thumbs.contains_key(&m.path) || self.gal_inflight >= 3 {
            return;
        }
        self.gal_thumbs.insert(m.path.clone(), Thumb::Loading);
        self.gal_inflight += 1;
        let tx = self.gal_tx.clone();
        let p = m.path.clone();
        let video = m.video;
        std::thread::spawn(move || {
            let img = if video {
                player::thumbnail(&p, 480).map(|t| (t.w, t.h, t.rgba, t.duration_ms))
            } else {
                load_png(&p).ok().map(|ci| {
                    let (w, h) = (ci.size[0], ci.size[1]);
                    let rgba: Vec<u8> = ci.pixels.iter().flat_map(|c| c.to_array()).collect();
                    let (tw, th, d) = downscale(&rgba, w, h, 480);
                    (tw, th, d, 0)
                })
            };
            let _ = tx.send(ThumbMsg { path: p, img });
        });
    }

    pub fn gallery_page(&mut self, ui: &mut egui::Ui, t: f64) {
        let ctx = ui.ctx().clone();
        if !self.settings.module_on("gallery") {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(ic::PUZZLE_PIECE).size(54.0).color(Color32::from_white_alpha(60)));
                ui.add_space(8.0);
                ui.label(RichText::new("Das Modul „Galerie“ ist ausgeschaltet").size(20.0));
                ui.add_space(10.0);
                if ui.add(egui::Button::new("Einschalten").fill(th_blue()).rounding(10.0).min_size(Vec2::new(160.0, 38.0))).clicked() {
                    self.settings.set_module("gallery", true);
                }
            });
            return;
        }
        self.gallery_poll_thumbs(&ctx);
        if t - self.gal_t > 2.0 {
            self.gal_t = t;
            self.gal_items = list_media(500);
        }
        let total: u64 = self.gal_items.iter().map(|m| m.size).sum();
        let n_video = self.gal_items.iter().filter(|m| m.video).count();
        let n_photo = self.gal_items.len() - n_video;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Galerie").size(28.0));
                ui.label(RichText::new("Deine Aufnahmen und Screenshots, direkt in der App ansehen, abspielen und teilen.").color(TEXT_DIM).size(13.0));
            });
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            segmented(ui, &mut self.gal_filter, &["Alle", "Videos", "Bilder"]);
            ui.add_space(12.0);
            ui.label(RichText::new(format!("{} Videos  •  {} Bilder  •  {}", n_video, n_photo, fmt_size(total))).color(TEXT_DIM).size(12.5));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Button::new(format!("{}  Ordner", ic::FOLDER_OPEN)).fill(th_card_hover()).rounding(10.0).min_size(Vec2::new(0.0, 34.0))).clicked() {
                    if let Ok(base) = std::env::var("USERPROFILE") {
                        let d = std::path::PathBuf::from(base).join(if self.gal_filter == 2 { "Pictures" } else { "Videos" }).join("OVERLIME");
                        let _ = std::fs::create_dir_all(&d);
                        let _ = std::process::Command::new("explorer").arg(d).spawn();
                    }
                }
            });
        });
        ui.add_space(12.0);

        let items = self.gallery_items();
        if items.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(ic::FILM_STRIP).size(54.0).color(Color32::from_white_alpha(60)));
                ui.add_space(8.0);
                ui.label(RichText::new("Noch nichts hier").size(20.0));
                ui.label(RichText::new("Im Stream: F9 nimmt ein Video auf, F12 macht einen Screenshot. Beides erscheint hier.").color(TEXT_DIM).size(13.0));
            });
            return;
        }

        let mut open: Option<PathBuf> = None;
        let mut del: Option<PathBuf> = None;
        let mut share: Option<PathBuf> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let gap = 14.0;
            let avail = ui.available_width();
            let cols = (((avail + gap) / (250.0 + gap)).floor() as usize).max(1);
            let cw = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
            let th = cw * 9.0 / 16.0;
            let card_h = th + 56.0;
            let mut last_group = "";
            let mut col = 0usize;
            let group_of = |s: u64| if s < 86400 { "Heute" } else if s < 2 * 86400 { "Gestern" } else if s < 7 * 86400 { "Diese Woche" } else { "Älter" };
            let mut y_cursor = ui.cursor().top();
            let x0 = ui.cursor().left();
            let mut row_top = y_cursor;
            let mut rows_total = 0.0f32;
            let _ = (&mut y_cursor, &mut rows_total);
            for (idx, m) in items.iter().enumerate() {
                let g = group_of(m.age_s);
                if g != last_group {
                    // neue Gruppe: Überschrift in eigener Zeile
                    if col != 0 {
                        row_top += card_h + gap;
                        col = 0;
                    }
                    let hr = Rect::from_min_size(Pos2::new(x0, row_top + if last_group.is_empty() { 0.0 } else { 6.0 }), Vec2::new(avail, 26.0));
                    ui.painter().text(hr.left_center(), Align2::LEFT_CENTER, lang::tr(g), FontId::proportional(15.0), Color32::from_rgb(190, 205, 235));
                    row_top = hr.bottom() + 6.0;
                    last_group = g;
                }
                let x = x0 + col as f32 * (cw + gap);
                let rect = Rect::from_min_size(Pos2::new(x, row_top), Vec2::new(cw, card_h));
                // nur sichtbare Karten zeichnen und Vorschaubilder anfordern
                let vis = ui.clip_rect().expand(300.0).intersects(rect);
                if vis {
                    self.gallery_request_thumb(m);
                    let resp = ui.interact(rect, egui::Id::new(("gal_card", idx)), egui::Sense::click());
                    let hv = ctx.animate_bool_with_time(egui::Id::new(("gal_hv", idx)), resp.hovered(), 0.14);
                    let p = ui.painter();
                    let lift = rect.translate(Vec2::new(0.0, -3.0 * hv));
                    p.rect_filled(lift.expand(1.0), Rounding::same(17.0), Color32::from_black_alpha((50.0 * hv) as u8));
                    p.rect_filled(lift, Rounding::same(16.0), th_card());
                    let tr = Rect::from_min_size(lift.min, Vec2::new(cw, th));
                    let top_round = Rounding { nw: 16.0, ne: 16.0, sw: 0.0, se: 0.0 };
                    match self.gal_thumbs.get(&m.path) {
                        Some(Thumb::Ready(tex, _)) => {
                            let ts = tex.size_vec2();
                            let (ra, rt) = (tr.width() / tr.height(), ts.x / ts.y);
                            let uv = if rt > ra {
                                let k = ra / rt;
                                Rect::from_min_max(Pos2::new((1.0 - k) / 2.0, 0.0), Pos2::new((1.0 + k) / 2.0, 1.0))
                            } else {
                                let k = rt / ra;
                                Rect::from_min_max(Pos2::new(0.0, (1.0 - k) / 2.0), Pos2::new(1.0, (1.0 + k) / 2.0))
                            };
                            egui::Image::new((tex.id(), ts)).uv(uv).rounding(top_round).paint_at(ui, tr);
                        }
                        Some(Thumb::Failed) => {
                            p.rect_filled(tr, top_round, Color32::from_rgb(24, 30, 52));
                            p.text(tr.center(), Align2::CENTER_CENTER, if m.video { ic::FILM_STRIP } else { ic::IMAGES }, FontId::proportional(34.0), Color32::from_white_alpha(70));
                        }
                        _ => {
                            p.rect_filled(tr, top_round, Color32::from_rgb(24, 30, 52));
                            let sh = ((t * 0.9) % 1.0) as f32;
                            let sx = tr.left() + (tr.width() + 80.0) * sh - 40.0;
                            p.rect_filled(Rect::from_min_max(Pos2::new(sx.max(tr.left()), tr.top()), Pos2::new((sx + 80.0).min(tr.right()), tr.bottom())), Rounding::ZERO, Color32::from_white_alpha(10));
                        }
                    }
                    let p = ui.painter();
                    // Abdunklung beim Darüberfahren und Abspiel-Symbol bei Videos
                    if hv > 0.0 {
                        p.rect_filled(tr, top_round, Color32::from_black_alpha((70.0 * hv) as u8));
                    }
                    if m.video {
                        let c = tr.center();
                        let r = 22.0 + 4.0 * hv;
                        p.circle_filled(c, r, Color32::from_black_alpha((120.0 + 60.0 * hv) as u8));
                        p.circle_stroke(c, r, Stroke::new(1.5, Color32::from_white_alpha((120.0 + 100.0 * hv) as u8)));
                        p.text(c + Vec2::new(1.5, 0.0), Align2::CENTER_CENTER, ic::PLAY, FontId::proportional(20.0), Color32::WHITE);
                        if let Some(Thumb::Ready(_, dur)) = self.gal_thumbs.get(&m.path) {
                            if *dur > 0 {
                                let txt = fmt_time(*dur);
                                let g = p.layout_no_wrap(txt.clone(), FontId::monospace(12.0), Color32::WHITE);
                                let br = Rect::from_min_size(tr.right_bottom() - Vec2::new(g.size().x + 20.0, 28.0), Vec2::new(g.size().x + 12.0, 20.0));
                                p.rect_filled(br, Rounding::same(6.0), Color32::from_black_alpha(170));
                                p.text(br.center(), Align2::CENTER_CENTER, txt, FontId::monospace(12.0), Color32::WHITE);
                            }
                        }
                    }
                    // Fußzeile
                    p.text(Pos2::new(lift.left() + 14.0, lift.top() + th + 18.0), Align2::LEFT_CENTER, age_text(m.age_s), FontId::proportional(14.0), Color32::WHITE);
                    p.text(Pos2::new(lift.left() + 14.0, lift.top() + th + 38.0), Align2::LEFT_CENTER, format!("{}  •  {}", if m.video { lang::tr("Video") } else { lang::tr("Screenshot") }, fmt_size(m.size)), FontId::proportional(12.0), TEXT_DIM);
                    // Aktionen beim Darüberfahren
                    if hv > 0.01 {
                        let mut small = |ui: &mut egui::Ui, icon: &str, idn: u32, c: Pos2| -> bool {
                            let br = Rect::from_center_size(c, Vec2::splat(30.0));
                            let r = ui.interact(br, egui::Id::new(("gal_act", idx, idn)), egui::Sense::click());
                            let h = r.hovered();
                            ui.painter().circle_filled(c, 15.0, Color32::from_black_alpha(if h { 215 } else { 150 }));
                            ui.painter().text(c, Align2::CENTER_CENTER, icon, FontId::proportional(15.0), Color32::WHITE.gamma_multiply(hv));
                            r.clicked()
                        };
                        if small(ui, ic::SHARE_NETWORK, 1, tr.right_top() + Vec2::new(-24.0, 24.0)) {
                            share = Some(m.path.clone());
                        } else if small(ui, ic::TRASH, 2, tr.right_top() + Vec2::new(-60.0, 24.0)) {
                            del = Some(m.path.clone());
                        } else if resp.clicked() {
                            open = Some(m.path.clone());
                        }
                    } else if resp.clicked() {
                        open = Some(m.path.clone());
                    }
                }
                col += 1;
                if col >= cols {
                    col = 0;
                    row_top += card_h + gap;
                }
            }
            if col != 0 {
                row_top += card_h + gap;
            }
            let used = row_top - ui.cursor().top();
            ui.allocate_space(Vec2::new(avail, used.max(10.0) + 10.0));
        });
        if let Some(p) = open {
            self.open_viewer(&ctx, p);
        }
        if let Some(p) = share {
            self.viewer_open_for_share(&ctx, p);
        }
        if let Some(p) = del {
            self.media_del = Some(p);
        }
        self.gallery_delete_dialog(&ctx);
    }

    /// Sicherheitsabfrage vor dem Löschen.
    fn gallery_delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = self.media_del.clone() else { return };
        if self.page != Page::Gallery && self.viewer.is_none() {
            return;
        }
        let mut close = false;
        egui::Area::new(egui::Id::new("gal_del")).order(egui::Order::Tooltip).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            egui::Frame::none().fill(th_card()).rounding(16.0).stroke(Stroke::new(1.0, Color32::from_white_alpha(40))).inner_margin(egui::Margin::same(20.0)).show(ui, |ui| {
                ui.set_width(340.0);
                ui.label(RichText::new("Datei löschen?").size(18.0).strong());
                ui.add_space(4.0);
                ui.label(RichText::new(path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).color(TEXT_DIM).size(13.0));
                ui.label(RichText::new("Das kann nicht rückgängig gemacht werden.").color(TEXT_DIM).size(12.5));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Löschen").color(Color32::WHITE)).fill(RED.gamma_multiply(0.85)).rounding(10.0).min_size(Vec2::new(120.0, 36.0))).clicked() {
                        let _ = std::fs::remove_file(&path);
                        self.gal_thumbs.remove(&path);
                        self.gal_t = -100.0;
                        self.media_t = -100.0;
                        if self.viewer.as_ref() == Some(&path) {
                            self.close_viewer();
                        }
                        close = true;
                    }
                    if ui.add(egui::Button::new("Abbrechen").rounding(10.0).min_size(Vec2::new(110.0, 36.0))).clicked() {
                        close = true;
                    }
                });
            });
        });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            close = true;
        }
        if close {
            self.media_del = None;
        }
    }

    pub fn open_viewer(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.close_viewer();
        let video = path.extension().map(|e| e.to_string_lossy().to_lowercase() == "mp4").unwrap_or(false);
        self.viewer_err.clear();
        self.viewer_video = video;
        self.viewer = Some(path.clone());
        self.vp_ctrl_t = ctx.input(|i| i.time);
        self.share_open = false;
        if video {
            match player::Player::open(&path, self.settings.volume, &self.settings.audio_device) {
                Ok(p) => self.player = Some(p),
                Err(e) => self.viewer_err = e,
            }
        }
    }

    fn viewer_open_for_share(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.open_viewer(ctx, path);
        self.share_open = true;
    }

    pub fn close_viewer(&mut self) {
        self.trim = None;
        self.viewer = None;
        self.viewer_tex = None;
        self.player = None;
        self.player_tex = None;
        self.share_open = false;
    }

    /// Vollbild-Anzeige für Screenshots und Videos.
    pub fn media_viewer(&mut self, ctx: &egui::Context) {
        let Some(cur) = self.viewer.clone() else { return };
        let now = ctx.input(|i| i.time);
        let items: Vec<MediaItem> = {
            let all = list_media(500);
            let f = self.gal_filter;
            let v: Vec<MediaItem> = all.into_iter().filter(|m| match f {
                1 => m.video,
                2 => !m.video,
                _ => true,
            }).collect();
            if v.iter().any(|m| m.path == cur) { v } else { list_media(500) }
        };
        let idx = items.iter().position(|m| m.path == cur);
        let video = self.viewer_video;
        let mut close = false;
        let mut nav: i32 = 0; // +1 = nächstes (älter), -1 = vorheriges (neuer)
        let mut toggle = false;
        let mut seek_rel: i64 = 0;
        let mut vol_rel: f32 = 0.0;
        let mut do_share = false;
        let mut do_delete = false;
        let mut speed_set: Option<f32> = None;
        let mut step: i64 = 0;
        let mut trim_toggle = false;
        let mut trim_start = false;
        let mut trim_end = false;
        let mut trim_save = false;
        let mut trim_cancel = false;
        ctx.input(|i| {
            if i.key_pressed(Key::Escape) && !self.share_open {
                close = true;
            }
            if video {
                if i.key_pressed(Key::Space) {
                    toggle = true;
                }
                if i.key_pressed(Key::ArrowLeft) {
                    seek_rel = -5000;
                }
                if i.key_pressed(Key::ArrowRight) {
                    seek_rel = 5000;
                }
                if i.key_pressed(Key::ArrowUp) {
                    vol_rel = 0.05;
                }
                if i.key_pressed(Key::ArrowDown) {
                    vol_rel = -0.05;
                }
                if i.key_pressed(Key::PageDown) {
                    nav = 1;
                }
                if i.key_pressed(Key::PageUp) {
                    nav = -1;
                }
                if i.key_pressed(Key::Comma) {
                    step = -1;
                }
                if i.key_pressed(Key::Period) {
                    step = 1;
                }
                if i.key_pressed(Key::I) {
                    trim_start = true;
                }
                if i.key_pressed(Key::O) {
                    trim_end = true;
                }
            } else {
                if i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::PageUp) {
                    nav = -1;
                }
                if i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::PageDown) {
                    nav = 1;
                }
            }
            if i.pointer.delta() != Vec2::ZERO || i.pointer.any_click() {
                self.vp_ctrl_t = now;
            }
        });
        let _ = &mut do_share;

        // Foto laden / Video-Bild holen
        if !video {
            if self.viewer_tex.as_ref().map(|(p, _)| p != &cur).unwrap_or(true) && self.viewer_err.is_empty() {
                match load_png(&cur) {
                    Ok(img) => {
                        let tex = ctx.load_texture("viewer", img, TextureOptions::LINEAR);
                        self.viewer_tex = Some((cur.clone(), tex));
                    }
                    Err(e) => self.viewer_err = e,
                }
            }
        } else if let Some(pl) = self.player.as_mut() {
            if toggle {
                if pl.ended() {
                    pl.restart();
                } else {
                    let p = !pl.paused();
                    pl.set_paused(p);
                }
            }
            if seek_rel != 0 {
                let np = pl.position_ms() + seek_rel;
                pl.seek(np);
                self.vp_ctrl_t = now;
            }
            if vol_rel != 0.0 {
                self.settings.volume = (self.settings.volume + vol_rel).clamp(0.0, 1.5);
                pl.set_volume(self.settings.volume);
                self.vp_ctrl_t = now;
            }
            let dt = ctx.input(|i| i.unstable_dt).min(0.1);
            if let Some((w, h, rgba)) = pl.poll(dt) {
                let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                match self.player_tex.as_mut() {
                    Some(t) => t.set(img, TextureOptions::LINEAR),
                    None => self.player_tex = Some(ctx.load_texture("vplayer", img, TextureOptions::LINEAR)),
                }
            }
            ctx.request_repaint();
        }

        let screen = ctx.screen_rect();
        let controls_vis = now - self.vp_ctrl_t < 2.8 || self.share_open || !video || self.player.as_ref().map(|p| p.paused() || p.ended()).unwrap_or(true);
        let ctrl_a = ctx.animate_bool_with_time(egui::Id::new("vp_ctrl_a"), controls_vis, 0.2);
        let mut click_toggle = false;
        let mut seek_to: Option<i64> = None;
        let mut new_vol: Option<f32> = None;
        let mut copy_file = false;
        let mut copy_image = false;
        let mut copy_path = false;
        let mut send_discord = false;
        let mut show_folder = false;
        egui::Area::new(egui::Id::new("media_viewer")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
            let p = ui.painter().clone();
            p.rect_filled(rect, Rounding::ZERO, Color32::from_rgb(4, 6, 12));
            let area = Rect::from_min_max(rect.min + Vec2::new(60.0, 56.0), rect.max - Vec2::new(60.0, if video { 96.0 } else { 84.0 }));
            let tex_info = if video { self.player_tex.as_ref().map(|t| (t.id(), t.size_vec2())) } else { self.viewer_tex.as_ref().map(|(_, t)| (t.id(), t.size_vec2())) };
            let mut media_rect = area;
            if let Some((id, sz)) = tex_info {
                let sc = (area.width() / sz.x).min(area.height() / sz.y).min(if video { 4.0 } else { 2.0 });
                let r = Rect::from_center_size(area.center(), sz * sc);
                media_rect = r;
                p.image(id, r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            } else if !self.viewer_err.is_empty() {
                p.text(rect.center(), Align2::CENTER_CENTER, format!("{} {}", lang::tr("Kann nicht geöffnet werden:"), self.viewer_err), FontId::proportional(15.0), RED);
            } else {
                p.text(rect.center(), Align2::CENTER_CENTER, lang::tr("Lädt …"), FontId::proportional(15.0), Color32::from_white_alpha(120));
            }
            if video && resp.clicked() && ui.input(|i| i.pointer.interact_pos().map(|pp| media_rect.contains(pp)).unwrap_or(false)) {
                click_toggle = true;
            }
            // Obere Leiste
            let a = ctrl_a;
            let name = cur.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(rect.width(), 56.0)), Rounding::ZERO, Color32::from_black_alpha((120.0 * a) as u8));
            p.text(Pos2::new(rect.left() + 24.0, rect.top() + 28.0), Align2::LEFT_CENTER, &name, FontId::proportional(14.0), Color32::from_white_alpha((220.0 * a) as u8));
            if let Some(ix) = idx {
                p.text(Pos2::new(rect.center().x, rect.top() + 28.0), Align2::CENTER_CENTER, format!("{} / {}", ix + 1, items.len()), FontId::proportional(13.0), Color32::from_white_alpha((150.0 * a) as u8));
            }
            let round_btn = |ui: &mut egui::Ui, icon: &str, c: Pos2, r: f32, idn: &str, alpha: f32| -> bool {
                let br = Rect::from_center_size(c, Vec2::splat(r * 2.0));
                let resp = ui.interact(br, egui::Id::new(("vbtn", idn)), egui::Sense::click());
                let h = resp.hovered();
                ui.painter().circle_filled(c, r, Color32::from_white_alpha(((if h { 46.0 } else { 20.0 }) * alpha) as u8));
                ui.painter().text(c, Align2::CENTER_CENTER, icon, FontId::proportional(r * 0.95), Color32::WHITE.gamma_multiply(alpha));
                resp.clicked() && alpha > 0.2
            };
            if round_btn(ui, ic::X, Pos2::new(rect.right() - 34.0, rect.top() + 28.0), 17.0, "close", a) {
                close = true;
            }
            // Seitenpfeile
            for (d, icon, x) in [(-1i32, ic::CARET_LEFT, rect.left() + 30.0), (1, ic::CARET_RIGHT, rect.right() - 30.0)] {
                let target = idx.map(|ix| ix as i32 + d).filter(|n| *n >= 0 && (*n as usize) < items.len());
                if target.is_some() && round_btn(ui, icon, Pos2::new(x, rect.center().y), 22.0, if d < 0 { "prev" } else { "next" }, a) {
                    nav = d;
                }
            }
            // Untere Leiste
            let bar_h = if video { 84.0 } else { 70.0 };
            let bar = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - bar_h), rect.max);
            p.rect_filled(bar, Rounding::ZERO, Color32::from_black_alpha((150.0 * a) as u8));
            let mut cx = rect.left() + 28.0;
            let by = bar.bottom() - 30.0;
            if video {
                if let Some(pl) = self.player.as_ref() {
                    let dur = pl.duration_ms.max(1);
                    let pos = pl.position_ms().clamp(0, dur);
                    // Zeitleiste
                    let tl = Rect::from_min_max(Pos2::new(rect.left() + 28.0, bar.top() + 14.0), Pos2::new(rect.right() - 28.0, bar.top() + 24.0));
                    let hit = tl.expand2(Vec2::new(0.0, 8.0));
                    let r2 = ui.interact(hit, egui::Id::new("vp_seek"), egui::Sense::click_and_drag());
                    let frac = (pos as f32 / dur as f32).clamp(0.0, 1.0);
                    let hov = r2.hovered() || r2.dragged();
                    let th_h = if hov { 6.0 } else { 4.0 };
                    let line = Rect::from_center_size(tl.center(), Vec2::new(tl.width(), th_h));
                    p.rect_filled(line, Rounding::same(3.0), Color32::from_white_alpha((50.0 * a) as u8));
                    let fill = Rect::from_min_size(line.min, Vec2::new(line.width() * frac, th_h));
                    p.rect_filled(fill, Rounding::same(3.0), th_blue().gamma_multiply(a));
                    if let Some((ts, te)) = self.trim {
                        let (x0, x1) = (tl.left() + tl.width() * (ts as f32 / dur as f32), tl.left() + tl.width() * (te as f32 / dur as f32));
                        p.rect_filled(Rect::from_min_max(Pos2::new(x0, tl.center().y - 8.0), Pos2::new(x1.max(x0 + 2.0), tl.center().y + 8.0)), Rounding::same(3.0), YELLOW.gamma_multiply(0.45 * a));
                        p.rect_filled(Rect::from_center_size(Pos2::new(x0, tl.center().y), Vec2::new(3.0, 18.0)), Rounding::same(1.0), YELLOW.gamma_multiply(a));
                        p.rect_filled(Rect::from_center_size(Pos2::new(x1, tl.center().y), Vec2::new(3.0, 18.0)), Rounding::same(1.0), YELLOW.gamma_multiply(a));
                    }
                    if hov || a > 0.5 {
                        p.circle_filled(Pos2::new(fill.right(), tl.center().y), if hov { 8.0 } else { 6.0 }, Color32::WHITE.gamma_multiply(a));
                    }
                    if r2.clicked() || r2.dragged() {
                        if let Some(pp) = r2.interact_pointer_pos() {
                            let f = ((pp.x - tl.left()) / tl.width()).clamp(0.0, 1.0);
                            seek_to = Some((f * dur as f32) as i64);
                        }
                    }
                    // Abspielen / Pause
                    let playing = !pl.paused() && !pl.ended();
                    let icon = if pl.ended() { ic::ARROW_COUNTER_CLOCKWISE } else if playing { ic::PAUSE } else { ic::PLAY };
                    if round_btn(ui, icon, Pos2::new(cx + 18.0, by), 20.0, "play", a) {
                        click_toggle = true;
                    }
                    cx += 52.0;
                    p.text(Pos2::new(cx, by), Align2::LEFT_CENTER, format!("{} / {}", fmt_time(pos), fmt_time(dur)), FontId::monospace(13.0), Color32::from_white_alpha((230.0 * a) as u8));
                    // Lautstärke
                    let vx = cx + 150.0;
                    let vol = self.settings.volume;
                    p.text(Pos2::new(vx, by), Align2::LEFT_CENTER, if vol <= 0.01 { ic::SPEAKER_X } else { ic::SPEAKER_HIGH }, FontId::proportional(18.0), Color32::WHITE.gamma_multiply(a));
                    let vs = Rect::from_min_max(Pos2::new(vx + 30.0, by - 8.0), Pos2::new(vx + 130.0, by + 8.0));
                    let rv = ui.interact(vs, egui::Id::new("vp_vol"), egui::Sense::click_and_drag());
                    let vl = Rect::from_center_size(vs.center(), Vec2::new(vs.width(), 4.0));
                    p.rect_filled(vl, Rounding::same(2.0), Color32::from_white_alpha((50.0 * a) as u8));
                    let vf = (vol / 1.5).clamp(0.0, 1.0);
                    p.rect_filled(Rect::from_min_size(vl.min, Vec2::new(vl.width() * vf, 4.0)), Rounding::same(2.0), Color32::WHITE.gamma_multiply(a));
                    p.circle_filled(Pos2::new(vl.left() + vl.width() * vf, vs.center().y), 6.0, Color32::WHITE.gamma_multiply(a));
                    if rv.clicked() || rv.dragged() {
                        if let Some(pp) = rv.interact_pointer_pos() {
                            new_vol = Some(((pp.x - vl.left()) / vl.width()).clamp(0.0, 1.0) * 1.5);
                        }
                    }
                    // Einzelbild, Geschwindigkeit, Zuschneiden
                    let sx = vx + 150.0;
                    if round_btn(ui, ic::CARET_LEFT, Pos2::new(sx, by), 15.0, "stepb", a) {
                        step = -1;
                    }
                    if round_btn(ui, ic::CARET_RIGHT, Pos2::new(sx + 36.0, by), 15.0, "stepf", a) {
                        step = 1;
                    }
                    for (k, (label, v)) in [("0.25×", 0.25f32), ("0.5×", 0.5), ("1×", 1.0), ("2×", 2.0)].iter().enumerate() {
                        let r = Rect::from_center_size(Pos2::new(sx + 90.0 + k as f32 * 48.0, by), Vec2::new(44.0, 28.0));
                        let rr = ui.interact(r, egui::Id::new(("vspeed", k)), egui::Sense::click());
                        let on = (pl.speed - v).abs() < 0.01;
                        p.rect_filled(r, Rounding::same(8.0), if on { th_blue().gamma_multiply(a) } else { Color32::from_white_alpha(((if rr.hovered() { 34.0 } else { 16.0 }) * a) as u8) });
                        p.text(r.center(), Align2::CENTER_CENTER, *label, FontId::proportional(12.5), Color32::WHITE.gamma_multiply(a));
                        if rr.clicked() && a > 0.2 {
                            speed_set = Some(*v);
                        }
                    }
                    if round_btn(ui, ic::SCISSORS, Pos2::new(sx + 90.0 + 4.0 * 48.0 + 10.0, by), 16.0, "trim", a) {
                        trim_toggle = true;
                    }
                } else if !self.viewer_err.is_empty() {
                    // Fehlertext steht schon in der Mitte
                }
            }
            // Zuschneiden: kleines Fenster über der Leiste
            if let (true, Some((ts, te))) = (video, self.trim) {
                let w = 600.0f32.min(rect.width() - 40.0);
                let tr = Rect::from_center_size(Pos2::new(rect.center().x, bar.top() - 44.0), Vec2::new(w, 64.0));
                p.rect_filled(tr, Rounding::same(14.0), th_card().gamma_multiply(0.97));
                p.rect_stroke(tr, Rounding::same(14.0), Stroke::new(1.0, YELLOW.gamma_multiply(0.6)));
                p.text(Pos2::new(tr.left() + 16.0, tr.top() + 18.0), Align2::LEFT_CENTER, format!("{}  {} – {}  ({} s)", ic::SCISSORS, fmt_time(ts), fmt_time(te), ((te - ts).max(0) / 1000)), FontId::proportional(13.5), Color32::WHITE);
                p.text(Pos2::new(tr.left() + 16.0, tr.top() + 42.0), Align2::LEFT_CENTER, lang::tr("I = Anfang hier, O = Ende hier. Der Anfang rutscht auf das Schlüsselbild davor."), FontId::proportional(11.0), TEXT_DIM);
                let mk = |ui: &mut egui::Ui, label: String, x: f32, wd: f32, id: &str, fill: Color32| -> bool {
                    let r = Rect::from_min_size(Pos2::new(x, tr.top() + 10.0), Vec2::new(wd, 28.0));
                    let rr = ui.interact(r, egui::Id::new(("trimb", id)), egui::Sense::click());
                    ui.painter().rect_filled(r, Rounding::same(8.0), if rr.hovered() { fill.gamma_multiply(1.15) } else { fill });
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(12.5), Color32::WHITE);
                    rr.clicked()
                };
                let mut x = tr.right() - 12.0;
                x -= 96.0;
                if mk(ui, lang::tr("Abbrechen"), x, 96.0, "c", Color32::from_white_alpha(26)) {
                    trim_cancel = true;
                }
                x -= 142.0;
                if !self.trim_busy && mk(ui, lang::tr("Clip speichern"), x, 134.0, "s", GREEN.gamma_multiply(0.8)) {
                    trim_save = true;
                }
                x -= 92.0;
                if mk(ui, lang::tr("Ende (O)"), x, 84.0, "e", Color32::from_white_alpha(26)) {
                    trim_end = true;
                }
                x -= 100.0;
                if mk(ui, lang::tr("Anfang (I)"), x, 92.0, "b", Color32::from_white_alpha(26)) {
                    trim_start = true;
                }
            }
            // Aktionen rechts: Teilen, Im Ordner, Löschen
            let mut rx = rect.right() - 34.0;
            if round_btn(ui, ic::TRASH, Pos2::new(rx, by), 20.0, "del", a) {
                do_delete = true;
            }
            rx -= 50.0;
            if round_btn(ui, ic::FOLDER_OPEN, Pos2::new(rx, by), 20.0, "folder", a) {
                show_folder = true;
            }
            rx -= 50.0;
            let share_c = Pos2::new(rx, by);
            if round_btn(ui, ic::SHARE_NETWORK, share_c, 20.0, "share", a) {
                self.share_open = !self.share_open;
            }
            if !video {
                p.text(Pos2::new(cx, by), Align2::LEFT_CENTER, lang::tr("Pfeiltasten blättern, Esc schließt"), FontId::proportional(12.5), Color32::from_white_alpha((140.0 * a) as u8));
            }
            // Teilen-Menü
            if self.share_open {
                let menu_w = 372.0;
                let n_entries = (if video { 2.0 } else { 3.0 }) + if self.settings.module_on("discordsend") { 1.0 } else { 0.0 };
                let mh = 52.0 + n_entries * 50.0;
                let mr = Rect::from_min_size(Pos2::new((share_c.x + 30.0 - menu_w).max(rect.left() + 10.0), bar.top() - 10.0 - mh), Vec2::new(menu_w, mh));
                p.rect_filled(mr.expand(1.0), Rounding::same(17.0), Color32::from_black_alpha(120));
                p.rect_filled(mr, Rounding::same(16.0), th_card());
                p.rect_stroke(mr, Rounding::same(16.0), Stroke::new(1.0, Color32::from_white_alpha(36)));
                p.text(Pos2::new(mr.left() + 18.0, mr.top() + 22.0), Align2::LEFT_CENTER, lang::tr("Teilen"), FontId::proportional(15.0), Color32::WHITE);
                let mut ry = mr.top() + 44.0;
                let mut entries: Vec<(&str, &str, &str, u8)> = vec![
                    (ic::COPY, "Datei kopieren", "Dann mit Strg+V in Discord, WhatsApp oder E-Mail einfügen", 0),
                ];
                if !video {
                    entries.push((ic::IMAGES, "Bild kopieren", "Als Bild einfügen, zum Beispiel in Chat oder Bildbearbeitung", 1));
                }
                entries.push((ic::CLIPBOARD_TEXT, "Pfad kopieren", "Speicherort der Datei als Text", 2));
                if self.settings.module_on("discordsend") {
                    entries.push((ic::PAPER_PLANE_TILT, "An Discord senden", "In deinen eigenen Kanal (Webhook aus den Einstellungen)", 3));
                }
                for (icon, title, sub, kind) in entries {
                    let er = Rect::from_min_size(Pos2::new(mr.left() + 8.0, ry), Vec2::new(menu_w - 16.0, 46.0));
                    let rr = ui.interact(er, egui::Id::new(("share_e", kind)), egui::Sense::click());
                    if rr.hovered() {
                        p.rect_filled(er, Rounding::same(10.0), Color32::from_white_alpha(14));
                    }
                    p.text(er.left_center() + Vec2::new(12.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(20.0), th_blue_light());
                    p.text(er.left_top() + Vec2::new(46.0, 14.0), Align2::LEFT_CENTER, lang::tr(title), FontId::proportional(14.0), Color32::WHITE);
                    p.text(er.left_top() + Vec2::new(46.0, 32.0), Align2::LEFT_CENTER, lang::tr(sub), FontId::proportional(11.0), TEXT_DIM);
                    if rr.clicked() {
                        match kind {
                            0 => copy_file = true,
                            1 => copy_image = true,
                            3 => send_discord = true,
                            _ => copy_path = true,
                        }
                    }
                    ry += 50.0;
                }
                // Klick außerhalb schließt das Menü
                if resp.clicked() && ui.input(|i| i.pointer.interact_pos().map(|pp| !mr.contains(pp) && pp.distance(share_c) > 24.0).unwrap_or(false)) {
                    self.share_open = false;
                    click_toggle = false;
                }
            }
        });

        if let Some(pl) = self.player.as_mut() {
            if let Some(v) = speed_set {
                pl.set_speed(v);
            }
            if step != 0 {
                pl.step_frame(step as i64);
                self.vp_ctrl_t = now;
            }
        }
        if trim_toggle {
            self.trim = if self.trim.is_some() { None } else { self.player.as_ref().map(|p| (p.position_ms(), p.duration_ms)) };
        }
        if let (Some((ts, te)), Some(pl)) = (self.trim, self.player.as_ref()) {
            let pos = pl.position_ms();
            if trim_start {
                self.trim = Some((pos.min(te - 500).max(0), te));
            }
            if trim_end {
                self.trim = Some((ts, pos.max(ts + 500).min(pl.duration_ms)));
            }
        }
        if trim_cancel {
            self.trim = None;
        }
        if trim_save {
            if let Some((ts, te)) = self.trim {
                let inp = cur.clone();
                let stem = inp.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "clip".into());
                let dir = inp.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                let mut out = dir.join(format!("{stem}-Clip.mp4"));
                let mut n = 2;
                while out.exists() {
                    out = dir.join(format!("{stem}-Clip{n}.mp4"));
                    n += 1;
                }
                self.trim_busy = true;
                let tx = self.tx.clone();
                self.toasts.push(("Clip wird gespeichert …".to_string(), 0, now));
                std::thread::spawn(move || {
                    let name = out.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let m = match crate::player::trim(&inp, &out, ts, te) {
                        Ok(()) => Msg::Notice(format!("Clip gespeichert: {name}"), 2),
                        Err(e) => Msg::Notice(format!("Zuschneiden nicht möglich: {e}"), 1),
                    };
                    let _ = tx.send(m);
                });
                self.trim = None;
                self.gal_t = -100.0;
                self.media_t = -100.0;
            }
        }
        if click_toggle {
            if let Some(pl) = self.player.as_mut() {
                if pl.ended() {
                    pl.restart();
                } else {
                    let p = !pl.paused();
                    pl.set_paused(p);
                }
            }
        }
        if let (Some(ms), Some(pl)) = (seek_to, self.player.as_mut()) {
            pl.seek(ms);
        }
        if let Some(v) = new_vol {
            self.settings.volume = v;
            if let Some(pl) = self.player.as_mut() {
                pl.set_volume(v);
            }
        }
        let pth = cur.to_string_lossy().to_string();
        if copy_file {
            let q = pth.replace('\'', "''");
            run_hidden("powershell", &["-NoProfile", "-WindowStyle", "Hidden", "-Command", &format!("Set-Clipboard -LiteralPath '{q}'")]);
            self.toasts.push(("Datei kopiert. Jetzt mit Strg+V in Discord, WhatsApp oder E-Mail einfügen.".to_string(), 2, now));
            self.share_open = false;
        }
        if copy_image {
            let q = pth.replace('\'', "''");
            run_hidden(
                "powershell",
                &["-NoProfile", "-STA", "-WindowStyle", "Hidden", "-Command", &format!("Add-Type -AssemblyName System.Windows.Forms,System.Drawing; $i=[System.Drawing.Image]::FromFile('{q}'); [System.Windows.Forms.Clipboard]::SetImage($i)")],
            );
            self.toasts.push(("Bild kopiert. Jetzt mit Strg+V einfügen.".to_string(), 2, now));
            self.share_open = false;
        }
        if send_discord {
            self.share_open = false;
            if self.settings.discord_webhook.trim().is_empty() {
                self.toasts.push(("Trage zuerst in den Einstellungen unter Discord deine Webhook-Adresse ein.".to_string(), 1, now));
            } else {
                let hookurl = self.settings.discord_webhook.clone();
                let path = cur.clone();
                let tx = self.tx.clone();
                self.toasts.push(("Wird an Discord gesendet …".to_string(), 0, now));
                std::thread::spawn(move || {
                    let m = match crate::hook::send_file(&hookurl, &path, "") {
                        Ok(()) => Msg::Notice("An Discord gesendet.".to_string(), 2),
                        Err(e) => Msg::Notice(e, 1),
                    };
                    let _ = tx.send(m);
                });
            }
        }
        if copy_path {
            ctx.copy_text(pth.clone());
            self.toasts.push(("Pfad kopiert.".to_string(), 2, now));
            self.share_open = false;
        }
        if show_folder {
            let _ = std::process::Command::new("explorer").arg(format!("/select,{}", pth)).spawn();
        }
        if do_delete {
            self.media_del = Some(cur.clone());
        }
        if nav != 0 {
            if let Some(ix) = idx {
                let n = ix as i32 + nav;
                if n >= 0 && (n as usize) < items.len() {
                    let np = items[n as usize].path.clone();
                    self.open_viewer(ctx, np);
                }
            }
        }
        self.gallery_delete_dialog(ctx);
        if close {
            self.close_viewer();
        }
        let _ = (do_share, vol_rel);
    }
}
