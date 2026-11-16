//! Aufnahme: hört global auf Mausklicks und macht bei jedem Klick einen Screenshot.
//!
//! Aufbau:
//! - Ein Thread hört mit `rdev` auf alle Maus-Ereignisse im System.
//! - Bei einem Klick (während die Aufnahme läuft) geht eine Nachricht an einen Arbeits-Thread.
//! - Der Arbeits-Thread macht den Screenshot, markiert die Klick-Stelle und speichert den Schritt.
//!
//! So bleibt der Maus-Hook schnell, und die Maus ruckelt nicht.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use image::{Rgba, RgbaImage};

/// Ein aufgezeichneter Schritt.
#[derive(Clone, Debug)]
pub struct Step {
    pub nr: usize,
    pub time: DateTime<Local>,
    pub button: MouseButton,
    /// Klick-Position in Bildschirm-Koordinaten.
    pub x: i32,
    pub y: i32,
    /// Fenster unter dem Mauszeiger (falls erkennbar).
    pub window_title: String,
    pub app_name: String,
    /// Pfad zum gespeicherten Screenshot (JPEG).
    pub image_path: PathBuf,
    /// Eigene Notiz der Benutzerin / des Benutzers.
    pub comment: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other,
}

impl MouseButton {
    pub fn label(self) -> &'static str {
        match self {
            MouseButton::Left => "Linksklick",
            MouseButton::Right => "Rechtsklick",
            MouseButton::Middle => "Mittelklick",
            MouseButton::Other => "Klick",
        }
    }
}

/// Zustand, den GUI und Hintergrund-Threads gemeinsam nutzen.
#[derive(Default)]
pub struct Shared {
    pub recording: AtomicBool,
    pub steps: Mutex<Vec<Step>>,
    pub session_dir: Mutex<Option<PathBuf>>,
    pub last_error: Mutex<Option<String>>,
    /// Rechteck des eigenen Fensters in Bildschirm-Pixeln (x, y, Breite, Höhe).
    /// Klicks darin werden ignoriert.
    pub own_rect: Mutex<Option<(i32, i32, i32, i32)>>,
    /// Wird gesetzt, damit die GUI nach einem neuen Schritt neu zeichnet.
    pub repaint: Mutex<Option<Box<dyn Fn() + Send>>>,
}

impl Shared {
    fn set_error(&self, msg: String) {
        eprintln!("Fehler: {msg}");
        *self.last_error.lock().unwrap() = Some(msg);
        self.request_repaint();
    }

    fn request_repaint(&self) {
        if let Some(f) = self.repaint.lock().unwrap().as_ref() {
            f();
        }
    }
}

struct Click {
    x: i32,
    y: i32,
    button: MouseButton,
    time: DateTime<Local>,
}

/// Startet den globalen Maus-Listener und den Arbeits-Thread.
/// Muss nur einmal beim Programmstart aufgerufen werden.
pub fn start_background(shared: Arc<Shared>) {
    let (tx, rx) = mpsc::channel::<Click>();

    {
        let shared = shared.clone();
        thread::Builder::new()
            .name("screenshot-worker".into())
            .spawn(move || worker(shared, rx))
            .expect("Arbeits-Thread konnte nicht gestartet werden");
    }

    thread::Builder::new()
        .name("mouse-listener".into())
        .spawn(move || listen(shared, tx))
        .expect("Maus-Listener konnte nicht gestartet werden");
}

fn listen(shared: Arc<Shared>, tx: Sender<Click>) {
    // rdev liefert bei Klicks keine Position mit, darum merken wir uns die letzte Mausposition.
    let mut last_pos = (0.0_f64, 0.0_f64);
    let listener_shared = shared.clone();

    let result = rdev::listen(move |event| match event.event_type {
        rdev::EventType::MouseMove { x, y } => last_pos = (x, y),
        rdev::EventType::ButtonPress(btn) => {
            if !listener_shared.recording.load(Ordering::Relaxed) {
                return;
            }
            let button = match btn {
                rdev::Button::Left => MouseButton::Left,
                rdev::Button::Right => MouseButton::Right,
                rdev::Button::Middle => MouseButton::Middle,
                rdev::Button::Unknown(_) => MouseButton::Other,
            };
            let _ = tx.send(Click {
                x: last_pos.0.round() as i32,
                y: last_pos.1.round() as i32,
                button,
                time: Local::now(),
            });
        }
        _ => {}
    });

    if let Err(e) = result {
        shared.set_error(format!(
            "Maus-Klicks können nicht abgehört werden ({e:?}). \
             Unter macOS: Bedienungshilfen-Rechte erteilen. Unter Linux: X11 wird benötigt."
        ));
    }
}

fn worker(shared: Arc<Shared>, rx: Receiver<Click>) {
    let own_pid = std::process::id();

    for click in rx {
        // Klicks auf das eigene Fenster (Start/Stopp-Knöpfe usw.) nicht aufzeichnen.
        let in_own_rect = shared
            .own_rect
            .lock()
            .unwrap()
            .is_some_and(|(x, y, w, h)| {
                click.x >= x && click.y >= y && click.x < x + w && click.y < y + h
            });
        if in_own_rect {
            continue;
        }
        let window = window_at(click.x, click.y);
        if window.as_ref().is_some_and(|w| w.pid == own_pid) {
            continue;
        }

        let Some(dir) = shared.session_dir.lock().unwrap().clone() else {
            continue;
        };

        let nr = shared.steps.lock().unwrap().len() + 1;
        // Freien Dateinamen suchen (nach dem Löschen eines Schritts kann die Nummer schon belegt sein).
        let image_path = (nr..)
            .map(|n| dir.join(format!("schritt_{n:03}.jpg")))
            .find(|p| !p.exists())
            .expect("unendlicher Iterator");

        match capture_and_save(click.x, click.y, &image_path) {
            Ok(()) => {
                let (window_title, app_name) = window
                    .map(|w| (w.title, w.app_name))
                    .unwrap_or_default();
                shared.steps.lock().unwrap().push(Step {
                    nr,
                    time: click.time,
                    button: click.button,
                    x: click.x,
                    y: click.y,
                    window_title,
                    app_name,
                    image_path,
                    comment: String::new(),
                });
                shared.request_repaint();
            }
            Err(e) => shared.set_error(format!("Screenshot fehlgeschlagen: {e:#}")),
        }
    }
}

/// Macht einen Screenshot des Bildschirms, auf dem geklickt wurde,
/// markiert die Klick-Stelle und speichert das Bild als JPEG.
fn capture_and_save(x: i32, y: i32, path: &Path) -> Result<()> {
    let monitor = xcap::Monitor::from_point(x, y)
        .or_else(|_| {
            xcap::Monitor::all()?
                .into_iter()
                .next()
                .ok_or(xcap::XCapError::new("Kein Bildschirm gefunden"))
        })
        .context("Bildschirm nicht gefunden")?;

    let mut img = monitor.capture_image().context("Bild aufnehmen")?;

    // Position relativ zum Bildschirm. Falls das Bild eine andere Grösse hat als die
    // gemeldete Bildschirmgrösse (z. B. wegen Skalierung), entsprechend umrechnen.
    let mx = monitor.x().unwrap_or(0);
    let my = monitor.y().unwrap_or(0);
    let mw = monitor.width().unwrap_or(img.width()).max(1);
    let mh = monitor.height().unwrap_or(img.height()).max(1);
    let rx = (x - mx) as f32 * img.width() as f32 / mw as f32;
    let ry = (y - my) as f32 * img.height() as f32 / mh as f32;

    let scale = (img.width() as f32 / 1920.0).max(1.0);
    draw_marker(&mut img, rx, ry, scale);

    let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
    let file = std::fs::File::create(path).with_context(|| format!("{}", path.display()))?;
    let mut writer = std::io::BufWriter::new(file);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 85)
        .encode_image(&rgb)
        .context("JPEG speichern")?;
    Ok(())
}

/// Zeichnet einen roten Ring um die Klick-Stelle.
fn draw_marker(img: &mut RgbaImage, cx: f32, cy: f32, scale: f32) {
    let outer = 22.0 * scale;
    let inner = 17.0 * scale;
    let border = 1.5 * scale; // weisser Rand aussen und innen für Kontrast
    let red = Rgba([230, 30, 30, 255]);
    let white = Rgba([255, 255, 255, 255]);

    let r = (outer + border).ceil() as i32;
    let (w, h) = (img.width() as i32, img.height() as i32);
    for dy in -r..=r {
        for dx in -r..=r {
            let px = cx as i32 + dx;
            let py = cy as i32 + dy;
            if px < 0 || py < 0 || px >= w || py >= h {
                continue;
            }
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            let color = if d >= inner && d <= outer {
                Some(red)
            } else if (d > outer && d <= outer + border) || (d < inner && d >= inner - border) {
                Some(white)
            } else {
                None
            };
            if let Some(c) = color {
                img.put_pixel(px as u32, py as u32, c);
            }
        }
    }
    // Kleiner Punkt in der Mitte.
    let dot = (3.0 * scale) as i32;
    for dy in -dot..=dot {
        for dx in -dot..=dot {
            let px = cx as i32 + dx;
            let py = cy as i32 + dy;
            if dx * dx + dy * dy <= dot * dot && px >= 0 && py >= 0 && px < w && py < h {
                img.put_pixel(px as u32, py as u32, red);
            }
        }
    }
}

struct WindowInfo {
    pid: u32,
    title: String,
    app_name: String,
}

/// Sucht das oberste sichtbare Fenster an der Position (x, y).
fn window_at(x: i32, y: i32) -> Option<WindowInfo> {
    let windows = xcap::Window::all().ok()?;
    windows
        .into_iter()
        .filter(|w| !w.is_minimized().unwrap_or(false))
        .filter(|w| {
            let (Ok(wx), Ok(wy), Ok(ww), Ok(wh)) = (w.x(), w.y(), w.width(), w.height()) else {
                return false;
            };
            ww > 0 && wh > 0 && x >= wx && y >= wy && x < wx + ww as i32 && y < wy + wh as i32
        })
        .max_by_key(|w| w.z().unwrap_or(i32::MIN))
        .map(|w| WindowInfo {
            pid: w.pid().unwrap_or(0),
            title: w.title().unwrap_or_default(),
            app_name: w.app_name().unwrap_or_default(),
        })
        .or_else(|| {
            // Rückfall: aktives Fenster.
            active_win_pos_rs::get_active_window().ok().map(|w| WindowInfo {
                pid: w.process_id as u32,
                title: w.title,
                app_name: w.app_name,
            })
        })
}

/// Legt einen neuen Ordner für eine Aufnahme an.
pub fn new_session_dir(base: &Path) -> Result<PathBuf> {
    let name = format!("Aufnahme_{}", Local::now().format("%Y-%m-%d_%H-%M-%S"));
    let dir = base.join(name);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Ordner {} anlegen", dir.display()))?;
    Ok(dir)
}

/// Standard-Ordner: Dokumente/Schrittaufzeichner
pub fn default_base_dir() -> PathBuf {
    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Schrittaufzeichner")
}
