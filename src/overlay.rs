use crossbeam_channel::{unbounded, Sender};
#[cfg(feature = "overlay-ui")]
use crossbeam_channel::Receiver;
#[cfg(feature = "overlay-ui")]
use eframe::{egui, NativeOptions};
#[cfg(feature = "overlay-ui")]
use parking_lot::Mutex;
#[cfg(feature = "overlay-ui")]
use std::sync::Arc;
#[cfg(feature = "overlay-ui")]
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum OverlayCmd {
    ShowRecording,
    ShowLatched,
    ShowError(String),
    PushRms(f32),
    /// A replace-map rule fired — flash a green icon next to the wave.
    ReplacementHit,
    Hide,
    Quit,
}

#[derive(Clone)]
pub struct OverlayHandle(pub Sender<OverlayCmd>);

impl OverlayHandle {
    pub fn show_recording(&self) { let _ = self.0.send(OverlayCmd::ShowRecording); }
    pub fn show_latched(&self)   { let _ = self.0.send(OverlayCmd::ShowLatched); }
    pub fn show_error(&self, m: String) { let _ = self.0.send(OverlayCmd::ShowError(m)); }
    pub fn push_rms(&self, r: f32) { let _ = self.0.send(OverlayCmd::PushRms(r)); }
    pub fn replacement_hit(&self) { let _ = self.0.send(OverlayCmd::ReplacementHit); }
    pub fn hide(&self)  { let _ = self.0.send(OverlayCmd::Hide); }
    pub fn quit(&self)  { let _ = self.0.send(OverlayCmd::Quit); }
}

/// No-op overlay when the feature is disabled. Drains commands silently so
/// the rest of the app keeps the same `OverlayHandle` API.
#[cfg(not(feature = "overlay-ui"))]
pub fn spawn() -> OverlayHandle {
    let (tx, rx) = unbounded::<OverlayCmd>();
    std::thread::spawn(move || while rx.recv().is_ok() {});
    OverlayHandle(tx)
}

#[cfg(feature = "overlay-ui")]
#[derive(Clone)]
enum View {
    Hidden,
    Recording {
        since: Instant,
        latched: bool,
        ready: bool,
    },
    Error { msg: String, until: Instant },
}

#[cfg(feature = "overlay-ui")]
struct App {
    rx: Receiver<OverlayCmd>,
    view: Arc<Mutex<View>>,
    bars: Arc<Mutex<Vec<f32>>>,
    /// Exponentially-decaying peak, used to auto-normalize bar heights.
    peak: Arc<Mutex<f32>>,
    /// When set, a replacement fired at this instant — flash an icon for ~1s.
    replacement_at: Arc<Mutex<Option<Instant>>>,
}

#[cfg(feature = "overlay-ui")]
impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0] // transparent — wgpu compositor handles this on Windows
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        static FIRST: std::sync::Once = std::sync::Once::new();
        FIRST.call_once(|| log::info!("overlay: first update() call — window is rendering"));

        // Drain commands.
        while let Ok(cmd) = self.rx.try_recv() {
            match cmd {
                OverlayCmd::ShowRecording => {
                    *self.view.lock() = View::Recording {
                        since: Instant::now(),
                        latched: false,
                        ready: false,
                    };
                    self.bars.lock().clear();
                    *self.peak.lock() = 0.05;
                }
                OverlayCmd::ShowLatched => {
                    *self.view.lock() = View::Recording {
                        since: Instant::now(),
                        latched: true,
                        ready: false,
                    };
                    self.bars.lock().clear();
                    *self.peak.lock() = 0.05;
                }
                OverlayCmd::ShowError(m) => {
                    *self.view.lock() = View::Error {
                        msg: m,
                        until: Instant::now() + Duration::from_millis(1500),
                    };
                }
                OverlayCmd::PushRms(r) => {
                    // First RMS after ShowRecording means the mic callback is
                    // actually firing -- flip view to "ready".
                    {
                        let mut v = self.view.lock();
                        if let View::Recording { ready, .. } = &mut *v {
                            if !*ready { *ready = true; }
                        }
                    }
                    let mut b = self.bars.lock();
                    if b.len() >= 64 { b.remove(0); }
                    b.push(r);
                    let mut pk = self.peak.lock();
                    *pk = pk.max(r).max(0.05);
                    *pk = *pk * 0.97 + r * 0.03;
                    if *pk < 0.05 { *pk = 0.05; }
                }
                OverlayCmd::ReplacementHit => {
                    *self.replacement_at.lock() = Some(Instant::now());
                }
                OverlayCmd::Hide => {
                    *self.view.lock() = View::Hidden;
                }
                OverlayCmd::Quit => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        // Auto-hide expired errors.
        {
            let mut v = self.view.lock();
            if let View::Error { until, .. } = &*v {
                if Instant::now() >= *until {
                    *v = View::Hidden;
                }
            }
        }

        let view = self.view.lock().clone();
        ctx.request_repaint_after(Duration::from_millis(33));

        // Always render a CentralPanel; when Hidden, use a fully-transparent frame
        // so the window appears invisible (still present + click-through).
        let hidden = matches!(view, View::Hidden);
        let frame = if hidden {
            egui::Frame::none().fill(egui::Color32::TRANSPARENT)
        } else {
            let bg = match &view {
                View::Error { .. } => egui::Color32::from_rgba_unmultiplied(120, 20, 20, 230),
                _ => egui::Color32::from_rgba_unmultiplied(22, 22, 28, 230),
            };
            egui::Frame::none()
                .fill(bg)
                .rounding(12.0)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 24)))
                .inner_margin(egui::Margin::symmetric(10.0, 6.0))
        };

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            match view {
                View::Hidden => {}
                View::Recording { ready, .. } => {
                    ui.horizontal_centered(|ui| {
                        let t = ui.input(|i| i.time) as f32;
                        // Replacement-hit: for ~0.6 s after a rule fires we
                        // pull the dot + wave colors toward green.
                        let hit = self
                            .replacement_at
                            .lock()
                            .and_then(|ts| {
                                let age = ts.elapsed().as_secs_f32();
                                (age < 0.6).then_some(1.0 - age / 0.6)
                            })
                            .unwrap_or(0.0);
                        let base_dot = if !ready { (240, 180, 40) } else { (255, 70, 70) };
                        let dot_rgb = blend(base_dot, (90, 220, 120), hit);
                        let freq = if !ready { 5.0 } else { 6.0 };
                        let pulse = 0.6 + 0.4 * (t * freq).sin().abs();
                        draw_dot(ui, dot_rgb, pulse, 4.0);
                        if ready {
                            ui.add_space(6.0);
                            let bars = self.bars.lock().clone();
                            let peak = *self.peak.lock();
                            draw_bars(ui, &bars, peak, hit);
                        }
                    });
                }
                View::Error { msg, .. } => {
                    ui.horizontal_centered(|ui| {
                        draw_dot(ui, (240, 80, 80), 1.0, 4.0);
                        ui.label(
                            egui::RichText::new(msg)
                                .color(egui::Color32::WHITE)
                                .size(11.0),
                        );
                    });
                }
            }
        });
    }
}

#[cfg(feature = "overlay-ui")]
fn blend(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| -> u8 {
        (x as f32 * (1.0 - t) + y as f32 * t).round().clamp(0.0, 255.0) as u8
    };
    (lerp(a.0, b.0), lerp(a.1, b.1), lerp(a.2, b.2))
}

#[cfg(feature = "overlay-ui")]
fn draw_dot(ui: &mut egui::Ui, rgb: (u8, u8, u8), alpha: f32, radius: f32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(radius * 2.0 + 2.0, radius * 2.0 + 2.0), egui::Sense::hover());
    let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
    let color = egui::Color32::from_rgba_unmultiplied(rgb.0, rgb.1, rgb.2, a);
    ui.painter_at(rect).circle_filled(rect.center(), radius, color);
}

#[cfg(feature = "overlay-ui")]
fn draw_bars(ui: &mut egui::Ui, bars: &[f32], peak: f32, hit: f32) {
    const BAR_COUNT: usize = 36;
    let avail = ui.available_width().min(140.0).max(60.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(avail, 22.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let max_h = rect.height();
    let slot_w = rect.width() / BAR_COUNT as f32;
    let bar_w = (slot_w - 1.5).max(1.0);
    // Right-anchored scroll: pad zeros on the left when we have fewer bars.
    let pad = BAR_COUNT.saturating_sub(bars.len());
    for i in 0..BAR_COUNT {
        let r = if i < pad { 0.0 } else { bars[i - pad] };
        // Normalize against a slowly-tracking peak so quiet and loud mics both render well.
        let norm = (r / peak.max(0.05)).clamp(0.0, 1.0);
        // sqrt compression after normalization for smoother mid-range response.
        let scaled = norm.sqrt();
        let h = (scaled * max_h).max(2.5);
        // Age-based fade: older bars slightly dimmer.
        let age = i as f32 / (BAR_COUNT - 1).max(1) as f32;
        let alpha = (0.55 + age * 0.45).min(1.0);
        // Color shifts from pinkish (quiet) toward warm white (loud peaks).
        let r_c = (240.0 + scaled * 15.0).min(255.0) as u8;
        let g_c = (80.0 + scaled * 140.0).min(220.0) as u8;
        let b_c = (100.0 + scaled * 130.0).min(210.0) as u8;
        let (r_c, g_c, b_c) = blend((r_c, g_c, b_c), (90, 220, 120), hit);
        let color = egui::Color32::from_rgba_unmultiplied(r_c, g_c, b_c, (alpha * 255.0) as u8);
        let x = rect.left() + i as f32 * slot_w;
        let y = rect.center().y;
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, y - h / 2.0), egui::vec2(bar_w, h)),
            1.5,
            color,
        );
    }
}

/// Spawn the overlay as a CHILD PROCESS (`whisper-local.exe --overlay`).
/// eframe/winit panics when its window is created from a worker thread on
/// Windows; running the overlay in its own process gives it a real main
/// thread. Parent → child commands travel as text lines on stdin.
#[cfg(feature = "overlay-ui")]
pub fn spawn() -> OverlayHandle {
    let (tx, rx) = unbounded::<OverlayCmd>();
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            log::error!("overlay: current_exe failed: {e}");
            std::thread::spawn(move || while rx.recv().is_ok() {});
            return OverlayHandle(tx);
        }
    };
    let mut child = match std::process::Command::new(&exe)
        .arg("--overlay")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("overlay: failed to spawn child: {e}");
            std::thread::spawn(move || while rx.recv().is_ok() {});
            return OverlayHandle(tx);
        }
    };
    log::info!("overlay: child pid={}", child.id());
    let mut stdin = child.stdin.take().expect("child stdin piped");
    std::thread::spawn(move || {
        use std::io::Write;
        while let Ok(cmd) = rx.recv() {
            let line = cmd_to_line(&cmd);
            if stdin.write_all(line.as_bytes()).is_err() {
                log::warn!("overlay: child stdin closed");
                break;
            }
            let _ = stdin.flush();
            if matches!(cmd, OverlayCmd::Quit) {
                break;
            }
        }
        let _ = child.wait();
    });
    OverlayHandle(tx)
}

/// Run the overlay viewport on the *current* (main) thread of the child
/// process. Called by `whisper-local.exe --overlay`. Reads command lines
/// from stdin and forwards them to the eframe App.
#[cfg(feature = "overlay-ui")]
pub fn run_main_thread() {
    let (tx, rx) = unbounded::<OverlayCmd>();
    let stdin_tx = tx.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let stdin = std::io::stdin();
        let mut buf = String::new();
        loop {
            buf.clear();
            match stdin.lock().read_line(&mut buf) {
                Ok(0) => {
                    let _ = stdin_tx.send(OverlayCmd::Quit);
                    break;
                }
                Ok(_) => {
                    if let Some(cmd) = line_to_cmd(&buf) {
                        if stdin_tx.send(cmd).is_err() {
                            break;
                        }
                    }
                }
                Err(_) => {
                    let _ = stdin_tx.send(OverlayCmd::Quit);
                    break;
                }
            }
        }
    });
    drop(tx);

    let view = Arc::new(Mutex::new(View::Hidden));
    let bars = Arc::new(Mutex::new(Vec::new()));
    let peak = Arc::new(Mutex::new(0.05_f32));
    let replacement_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let size = [200.0_f32, 38.0_f32];
    let pos = bottom_center_of_cursor_monitor(size);
    log::info!("overlay child: pos=({:.0},{:.0})", pos[0], pos[1]);
    let mut vb = egui::ViewportBuilder::default()
        .with_decorations(false)
        .with_transparent(true)
        .with_window_level(egui::WindowLevel::AlwaysOnTop)
        .with_mouse_passthrough(true)
        .with_resizable(false)
        .with_taskbar(false)
        .with_inner_size(size)
        .with_position(egui::pos2(pos[0], pos[1]));
    if let Some(ic) = crate::app_icon::icon_data() {
        vb = vb.with_icon(ic);
    }
    let opts = NativeOptions { viewport: vb, ..Default::default() };
    let app = App { rx, view, bars, peak, replacement_at };
    let result = eframe::run_native(
        "whisper-local-overlay",
        opts,
        Box::new(|cc| {
            crate::fonts::install_broad_unicode_font(&cc.egui_ctx);
            log::info!("overlay child: first frame");
            Box::new(app)
        }),
    );
    if let Err(e) = result {
        log::error!("overlay child: run_native error: {e:#}");
    }
}

#[cfg(feature = "overlay-ui")]
fn cmd_to_line(cmd: &OverlayCmd) -> String {
    match cmd {
        OverlayCmd::ShowRecording => "REC\n".into(),
        OverlayCmd::ShowLatched => "LAT\n".into(),
        OverlayCmd::ShowError(m) => format!("ERR\t{}\n", m.replace('\n', " ")),
        OverlayCmd::PushRms(r) => format!("RMS\t{r}\n"),
        OverlayCmd::ReplacementHit => "HIT\n".into(),
        OverlayCmd::Hide => "HID\n".into(),
        OverlayCmd::Quit => "QUI\n".into(),
    }
}

#[cfg(feature = "overlay-ui")]
fn line_to_cmd(line: &str) -> Option<OverlayCmd> {
    let line = line.trim_end();
    let mut parts = line.splitn(2, '\t');
    let tag = parts.next()?;
    match tag {
        "REC" => Some(OverlayCmd::ShowRecording),
        "LAT" => Some(OverlayCmd::ShowLatched),
        "HID" => Some(OverlayCmd::Hide),
        "QUI" => Some(OverlayCmd::Quit),
        "ERR" => Some(OverlayCmd::ShowError(parts.next().unwrap_or("").to_string())),
        "RMS" => parts.next().and_then(|s| s.parse().ok()).map(OverlayCmd::PushRms),
        "HIT" => Some(OverlayCmd::ReplacementHit),
        _ => None,
    }
}

#[cfg(feature = "overlay-ui")]
fn bottom_center_of_cursor_monitor(size: [f32; 2]) -> [f32; 2] {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    unsafe {
        let mut p = POINT::default();
        if GetCursorPos(&mut p).is_err() {
            log::warn!("overlay: GetCursorPos failed, falling back to (100,100)");
            return [100.0, 100.0];
        }
        let hmon = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(hmon, &mut mi).as_bool() {
            log::warn!("overlay: GetMonitorInfoW failed, falling back to (100,100)");
            return [100.0, 100.0];
        }
        let r = mi.rcWork;
        log::info!(
            "overlay: cursor=({},{}) monitor work-area=({},{},{},{})",
            p.x, p.y, r.left, r.top, r.right, r.bottom
        );
        // 80px above the work-area bottom — clears most taskbar/notification overlays.
        let x = ((r.left + r.right) as f32 - size[0]) / 2.0;
        let y = r.bottom as f32 - size[1] - 80.0;
        [x, y]
    }
}
