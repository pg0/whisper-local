use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::{egui, NativeOptions};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum OverlayCmd {
    ShowRecording,
    ShowLatched,
    ShowError(String),
    PushRms(f32),
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
    pub fn hide(&self)  { let _ = self.0.send(OverlayCmd::Hide); }
    pub fn quit(&self)  { let _ = self.0.send(OverlayCmd::Quit); }
}

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

struct App {
    rx: Receiver<OverlayCmd>,
    view: Arc<Mutex<View>>,
    bars: Arc<Mutex<Vec<f32>>>,
    /// Exponentially-decaying peak, used to auto-normalize bar heights.
    peak: Arc<Mutex<f32>>,
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0] // transparent
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
                .rounding(16.0)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 24)))
                .inner_margin(egui::Margin::symmetric(14.0, 10.0))
        };

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            match view {
                View::Hidden => {}
                View::Recording { latched, ready, .. } => {
                    ui.horizontal_centered(|ui| {
                        let t = ui.input(|i| i.time) as f32;
                        if !ready {
                            // Mic warming up — amber pulse, no waveform yet.
                            let pulse = 0.55 + 0.45 * (t * 5.0).sin().abs();
                            let dot_color = egui::Color32::from_rgba_unmultiplied(
                                240, 180, 40, (255.0 * pulse) as u8,
                            );
                            ui.colored_label(dot_color, egui::RichText::new("●").size(18.0));
                            ui.label(
                                egui::RichText::new("Starting mic…")
                                    .color(egui::Color32::from_rgb(230, 220, 180))
                                    .size(13.0),
                            );
                        } else {
                            let pulse = 0.6 + 0.4 * (t * 6.0).sin().abs();
                            let dot_color = egui::Color32::from_rgba_unmultiplied(
                                255, 70, 70, (255.0 * pulse) as u8,
                            );
                            ui.colored_label(dot_color, egui::RichText::new("●").size(18.0));
                            let label = if latched {
                                "Listening (latched) — tap to stop"
                            } else {
                                "Recording…"
                            };
                            ui.label(
                                egui::RichText::new(label)
                                    .color(egui::Color32::from_rgb(230, 230, 235))
                                    .size(13.0),
                            );
                            ui.add_space(10.0);
                            let bars = self.bars.lock().clone();
                            let peak = *self.peak.lock();
                            draw_bars(ui, &bars, peak);
                        }
                    });
                }
                View::Error { msg, .. } => {
                    ui.horizontal_centered(|ui| {
                        ui.label(
                            egui::RichText::new(format!("⚠  {msg}"))
                                .color(egui::Color32::WHITE)
                                .size(13.0),
                        );
                    });
                }
            }
        });
    }
}

fn draw_bars(ui: &mut egui::Ui, bars: &[f32], peak: f32) {
    const BAR_COUNT: usize = 48;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(200.0, 34.0), egui::Sense::hover());
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

/// Disabled: eframe panics when run from a worker thread on Windows. The
/// floating recording bar will return as a child-process window in a follow-up.
/// For now the tray icon swap (idle/active) is the recording indicator.
pub fn spawn() -> OverlayHandle {
    let (tx, rx) = unbounded::<OverlayCmd>();
    // Drain commands so senders never block; do nothing with them.
    std::thread::spawn(move || {
        while rx.recv().is_ok() {}
    });
    OverlayHandle(tx)
}

#[allow(dead_code)]
fn spawn_eframe_disabled() -> OverlayHandle {
    let (tx, rx) = unbounded::<OverlayCmd>();
    std::thread::spawn(move || {
        let view = Arc::new(Mutex::new(View::Hidden));
        let bars = Arc::new(Mutex::new(Vec::new()));
        let peak = Arc::new(Mutex::new(0.05_f32));
        let size = [420.0_f32, 68.0_f32];
        let pos = bottom_center_of_cursor_monitor(size);
        // DEBUG MODE: force opaque, no passthrough, no transparency, fixed
        // position (100,100). Confirms that the window can be created at all.
        // Once visible we can re-enable transparency/passthrough.
        let debug_visible = std::env::var("OVERLAY_DEBUG").is_ok();
        let mut vb = egui::ViewportBuilder::default()
            .with_decorations(debug_visible)
            .with_transparent(!debug_visible)
            .with_window_level(egui::WindowLevel::AlwaysOnTop)
            .with_mouse_passthrough(!debug_visible)
            .with_resizable(false)
            .with_taskbar(debug_visible)
            .with_inner_size(if debug_visible { [600.0, 120.0] } else { size })
            .with_position(if debug_visible {
                egui::pos2(100.0, 100.0)
            } else {
                egui::pos2(pos[0], pos[1])
            });
        if let Some(ic) = crate::app_icon::icon_data() {
            vb = vb.with_icon(ic);
        }
        log::info!(
            "overlay: spawning at pos=({:.0},{:.0}) size={:?}",
            pos[0], pos[1], size
        );
        let opts = NativeOptions { viewport: vb, ..Default::default() };
        let app = App { rx, view, bars, peak };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            eframe::run_native(
                "whisper-local-overlay",
                opts,
                Box::new(|_cc| {
                    log::info!("overlay: eframe creator fired");
                    Box::new(app)
                }),
            )
        }));
        match outcome {
            Ok(Ok(_)) => log::info!("overlay: run_native exited cleanly"),
            Ok(Err(e)) => log::error!("overlay: run_native returned error: {e:#}"),
            Err(panic) => {
                let msg = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| format!("{:?}", panic));
                log::error!("overlay: run_native PANICKED: {msg}");
            }
        }
    });
    OverlayHandle(tx)
}

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
