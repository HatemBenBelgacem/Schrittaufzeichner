// Unter Windows kein zusätzliches Konsolenfenster öffnen (nur im Release-Build).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod recorder;
mod report;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui;
use recorder::Shared;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Schrittaufzeichner")
            .with_inner_size([460.0, 560.0])
            .with_min_inner_size([360.0, 300.0])
            .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "Schrittaufzeichner",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

fn load_icon() -> egui::IconData {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .expect("Icon ist ein gültiges PNG")
        .into_rgba8();
    let (width, height) = img.dimensions();
    egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    }
}

struct App {
    shared: Arc<Shared>,
    base_dir: PathBuf,
    title: String,
    status: String,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let shared = Arc::new(Shared::default());
        let ctx = cc.egui_ctx.clone();
        *shared.repaint.lock().unwrap() = Some(Box::new(move || ctx.request_repaint()));
        recorder::start_background(shared.clone());

        Self {
            shared,
            base_dir: recorder::default_base_dir(),
            title: "Schritt-Anleitung".to_string(),
            status: "Bereit. Klicke auf «Aufnahme starten».".to_string(),
        }
    }

    fn is_recording(&self) -> bool {
        self.shared.recording.load(Ordering::Relaxed)
    }

    fn has_session(&self) -> bool {
        self.shared.session_dir.lock().unwrap().is_some()
    }

    fn start_new(&mut self) {
        match recorder::new_session_dir(&self.base_dir) {
            Ok(dir) => {
                self.shared.steps.lock().unwrap().clear();
                *self.shared.last_error.lock().unwrap() = None;
                self.status = format!("Aufnahme läuft – Ordner: {}", dir.display());
                *self.shared.session_dir.lock().unwrap() = Some(dir);
                self.shared.recording.store(true, Ordering::Relaxed);
            }
            Err(e) => self.status = format!("Fehler: {e:#}"),
        }
    }

    fn stop_and_save(&mut self) {
        self.shared.recording.store(false, Ordering::Relaxed);
        let Some(dir) = self.shared.session_dir.lock().unwrap().clone() else {
            return;
        };
        let steps = self.shared.steps.lock().unwrap().clone();
        if steps.is_empty() {
            self.status = "Keine Schritte aufgezeichnet.".into();
            return;
        }
        match report::write_report(&dir, &self.title, &steps) {
            Ok(path) => {
                self.status = format!("Bericht gespeichert: {}", path.display());
                let _ = open::that_detached(&path);
            }
            Err(e) => self.status = format!("Fehler beim Speichern: {e:#}"),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let recording = self.is_recording();

        // Eigene Fensterposition an den Recorder melden (damit Klicks hier ignoriert werden).
        let own_rect = ui.ctx().input(|i| {
            let vp = i.viewport();
            let ppp = vp.native_pixels_per_point.unwrap_or(1.0);
            vp.outer_rect.map(|r| {
                (
                    (r.min.x * ppp).round() as i32,
                    (r.min.y * ppp).round() as i32,
                    (r.width() * ppp).round() as i32,
                    (r.height() * ppp).round() as i32,
                )
            })
        });
        *self.shared.own_rect.lock().unwrap() = own_rect;

        egui::Panel::top("steuerung").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Titel:");
                ui.text_edit_singleline(&mut self.title);
            });
            ui.horizontal(|ui| {
                ui.label("Speicherort:");
                if ui.add_enabled(!recording, egui::Button::new("Ändern…")).clicked()
                    && let Some(dir) = rfd::FileDialog::new()
                        .set_directory(&self.base_dir)
                        .pick_folder()
                {
                    self.base_dir = dir;
                }
                ui.add(egui::Label::new(self.base_dir.display().to_string()).truncate())
                    .on_hover_text(self.base_dir.display().to_string());
            });
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if !recording {
                    if ui.button("⏺ Aufnahme starten").clicked() {
                        self.start_new();
                    }
                    if self.has_session() && ui.button("▶ Fortsetzen").clicked() {
                        self.shared.recording.store(true, Ordering::Relaxed);
                        self.status = "Aufnahme läuft weiter.".into();
                    }
                } else if ui.button("⏸ Pause").clicked() {
                    self.shared.recording.store(false, Ordering::Relaxed);
                    self.status = "Pausiert.".into();
                }
                if ui
                    .add_enabled(self.has_session(), egui::Button::new("⏹ Stopp & Bericht speichern"))
                    .clicked()
                {
                    self.stop_and_save();
                }
            });

            ui.add_space(4.0);
            if recording {
                ui.colored_label(egui::Color32::from_rgb(220, 40, 40), "● Aufnahme läuft – jeder Klick wird festgehalten");
            }
            ui.label(&self.status);
            if let Some(err) = self.shared.last_error.lock().unwrap().as_ref() {
                ui.colored_label(egui::Color32::from_rgb(200, 60, 0), err);
            }
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let mut steps = self.shared.steps.lock().unwrap();
            ui.heading(format!("Schritte ({})", steps.len()));
            ui.separator();

            let mut delete: Option<usize> = None;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for (i, step) in steps.iter_mut().enumerate() {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.strong(format!("{}.", step.nr));
                                ui.label(report::step_sentence(step));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("🗑").on_hover_text("Schritt löschen").clicked() {
                                        delete = Some(i);
                                    }
                                    if ui.small_button("🖼").on_hover_text("Bild öffnen").clicked() {
                                        let _ = open::that_detached(&step.image_path);
                                    }
                                    ui.weak(step.time.format("%H:%M:%S").to_string());
                                });
                            });
                            ui.add(
                                egui::TextEdit::multiline(&mut step.comment)
                                    .hint_text("Notiz zu diesem Schritt (optional)")
                                    .desired_rows(1)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                    }
                });

            if let Some(i) = delete {
                let removed = steps.remove(i);
                let _ = std::fs::remove_file(&removed.image_path);
                // Nummern neu vergeben (Bilddateien behalten ihren Namen).
                for (n, s) in steps.iter_mut().enumerate() {
                    s.nr = n + 1;
                }
            }
        });
    }
}
