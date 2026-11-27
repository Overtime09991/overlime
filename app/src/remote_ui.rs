//! Bereich „Von unterwegs“ auf der Konsolen-Seite.

use super::*;

impl App {
    pub fn remote_search(&mut self) {
        if self.remote_busy {
            return;
        }
        self.remote_busy = true;
        self.remote_error.clear();
        if std::env::args().any(|a| a == "--demo-remote") {
            self.remote_busy = false;
            self.remote_devices = Some(vec![
                remote::Device { name: "PS5-Wohnzimmer".to_string(), ps5: true, uid: [1; 32] },
                remote::Device { name: "Haupt-PS4".to_string(), ps5: false, uid: [b'A'; 32] },
            ]);
            return;
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::RemoteList(remote::devices()));
        });
    }

    pub fn remote_card(&mut self, ui: &mut egui::Ui) {
        let demo = std::env::args().any(|a| a == "--demo-remote");
        if self.demo_mode && !demo {
            return;
        }
        ui.add_space(14.0);
        egui::Frame::none().fill(th_card()).rounding(18.0).inner_margin(egui::Margin::same(18.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(ic::GLOBE).size(20.0).color(th_blue_light()));
                ui.label(RichText::new("Von unterwegs spielen").size(17.0).strong());
                ui.label(RichText::new("Experimentell").size(11.5).color(TEXT_DIM));
            });
            ui.label(RichText::new("Verbindet dich über dein PSN-Konto mit deiner Konsole zu Hause, ohne VPN und ohne Einstellungen im Router. Die Konsole muss an oder im Ruhemodus sein und Remote Play erlauben.").size(12.5).color(TEXT_DIM));
            ui.add_space(8.0);
            if !(demo || remote::logged_in()) {
                if ui.add(big_button(&format!("{}  Bei PSN anmelden", ic::SIGN_IN), th_blue())).clicked() {
                    // der bekannte Anmelde-Assistent, direkt beim Öffnen des Browsers
                    self.relogin = true;
                    self.skip_login = false;
                    self.login_step = 1;
                    self.login_pasted.clear();
                    self.login_error.clear();
                }
                ui.label(RichText::new("Du meldest dich selbst auf Sonys Seite an. Die App sieht dein Passwort nie. Gespeichert wird die Anmeldung nur verschlüsselt auf diesem PC. Das ist kein von Sony erlaubter Weg für fremde Programme und kann jederzeit aufhören zu funktionieren.").size(11.5).color(TEXT_DIM));
                return;
            }
            ui.horizontal(|ui| {
                if self.remote_busy {
                    ui.spinner();
                    ui.label(RichText::new("Frage PSN …").color(TEXT_DIM));
                } else if ui.add(egui::Button::new(format!("{} Konsolen aus meinem Konto laden", ic::ARROWS_CLOCKWISE)).fill(th_card_hover()).rounding(10.0)).clicked() {
                    self.remote_search();
                }
                if ui.add(egui::Button::new(RichText::new("Von PSN abmelden").color(TEXT_DIM).size(12.5)).frame(false)).clicked() {
                    remote::logout();
                    self.remote_devices = None;
                    self.remote_error.clear();
                }
            });
            if !self.remote_error.is_empty() {
                ui.label(RichText::new(&self.remote_error).color(RED).size(12.5));
            }
            let Some(list) = self.remote_devices.clone() else { return };
            if list.len() == 1 {
                ui.label(RichText::new("Keine PS5 mit erlaubtem Remote Play in deinem Konto gefunden. Eine PS4 erreichst du über „Haupt-PS4“.").size(12.0).color(TEXT_DIM));
            }
            for d in list {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(ic::GAME_CONTROLLER).size(18.0));
                    ui.label(RichText::new(&d.name).size(14.5));
                    ui.label(RichText::new(if d.ps5 { "PS5" } else { "PS4" }).size(11.5).color(TEXT_DIM));
                    let idle = self.starting.is_none() && self.session.is_none();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(idle, egui::Button::new(format!("{} Verbinden", ic::PLAY)).fill(th_blue()).rounding(10.0)).clicked() {
                            self.remote_connect(&d);
                        }
                    });
                });
            }
            if self.starting.is_some() {
                ui.label(RichText::new("Der Verbindungsaufbau über PSN kann bis zu einer Minute dauern.").size(12.0).color(TEXT_DIM));
            }
        });
    }

    fn remote_connect(&mut self, d: &remote::Device) {
        if std::env::args().any(|a| a == "--demo-remote") {
            return;
        }
        let addr = remote::address(&d.uid);
        let console = Console { addr: addr.clone(), name: d.name.clone(), id: addr.clone(), is_ps5: d.ps5, state: State::Unknown, system_version: String::new(), running_app: String::new() };
        // Die Kopplungs-Schlüssel holt sich der Kern bei dieser Art von Verbindung selbst über PSN
        let paired = Paired { console_id: addr, ps5: d.ps5, nickname: d.name.clone(), rp_regist_key: [0; 16], rp_key: [0; 16] };
        self.connect(&console, &paired);
    }
}
