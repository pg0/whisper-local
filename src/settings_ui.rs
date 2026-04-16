use crate::audio::AudioCapture;
use crate::autostart;
use crate::config::Config;
use eframe::egui;

const COMMAND_SYNTAX: &str = include_str!("../templates/syntax/command_syntax.txt");
const REGEX_SYNTAX: &str = include_str!("../templates/syntax/regex_syntax.txt");
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub fn open(initial: Config) -> Config {
    let state = Arc::new(Mutex::new(SettingsState {
        cfg: initial,
        devices: AudioCapture::list_input_devices(),
        autostart_enabled: autostart::is_enabled(),
        last_health_check: None,
        health_ok: false,
        save_clicked: false,
        tab: Tab::General,
    }));
    let state_for_ui = state.clone();
    let mut vb = egui::ViewportBuilder::default()
        .with_title("whisper-local")
        .with_inner_size([560.0, 460.0])
        .with_resizable(false);
    if let Some(ic) = crate::app_icon::icon_data() {
        vb = vb.with_icon(ic);
    }
    let opts = eframe::NativeOptions { viewport: vb, ..Default::default() };
    let _ = eframe::run_native(
        "whisper-local-settings",
        opts,
        Box::new(move |_cc| Box::new(SettingsApp { state: state_for_ui })),
    );
    let s = state.lock();
    s.cfg.clone()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    General,
    CommandSyntax,
    RegexSyntax,
}

struct SettingsState {
    cfg: Config,
    devices: Vec<String>,
    autostart_enabled: bool,
    last_health_check: Option<Instant>,
    health_ok: bool,
    save_clicked: bool,
    tab: Tab,
}

struct SettingsApp {
    state: Arc<Mutex<SettingsState>>,
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut st = self.state.lock();
        let now = Instant::now();
        if st
            .last_health_check
            .map(|t| now.duration_since(t) > Duration::from_secs(2))
            .unwrap_or(true)
        {
            let url = st.cfg.whisper.health_url();
            st.health_ok = reqwest::blocking::Client::builder()
                .timeout(Duration::from_millis(800))
                .build()
                .ok()
                .and_then(|c| c.get(&url).send().ok())
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            st.last_health_check = Some(now);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("whisper-local settings");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (Tab::General, "General"),
                    (Tab::CommandSyntax, "Command Syntax"),
                    (Tab::RegexSyntax, "Regex Syntax"),
                ] {
                    if ui.selectable_label(st.tab == tab, label).clicked() {
                        st.tab = tab;
                    }
                }
            });
            ui.separator();
            ui.add_space(4.0);

            if st.tab != Tab::General {
                let body = match st.tab {
                    Tab::CommandSyntax => COMMAND_SYNTAX,
                    Tab::RegexSyntax => REGEX_SYNTAX,
                    Tab::General => "",
                };
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut body.to_string())
                                .font(egui::FontId::monospace(10.5))
                                .desired_width(f32::INFINITY)
                                .interactive(false),
                        );
                    });
                return;
            }

            ui.label("Microphone:");
            let current = st.cfg.mic_name.clone();
            egui::ComboBox::from_id_source("mic")
                .width(360.0)
                .selected_text(if current.is_empty() {
                    "(default)".to_string()
                } else {
                    current.clone()
                })
                .show_ui(ui, |ui: &mut egui::Ui| {
                    ui.selectable_value(&mut st.cfg.mic_name, String::new(), "(default)");
                    let devs = st.devices.clone();
                    for d in devs {
                        ui.selectable_value(&mut st.cfg.mic_name, d.clone(), d);
                    }
                });

            ui.add_space(8.0);
            ui.label("Whisper server URL:");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut st.cfg.whisper.base_url);
                ui.weak("(default: http://localhost:10010)");
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Language:");
                let current = st.cfg.language.clone();
                egui::ComboBox::from_id_source("language")
                    .selected_text(if current.is_empty() {
                        "Auto-detect".to_string()
                    } else {
                        current.clone()
                    })
                    .show_ui(ui, |ui| {
                        for (code, label) in crate::config::LANGUAGES {
                            ui.selectable_value(
                                &mut st.cfg.language,
                                (*code).to_string(),
                                *label,
                            );
                        }
                    });
            });

            #[cfg(feature = "speaker-detection")]
            {
                ui.add_space(8.0);
                let mut spk = st.cfg.enable_speaker_detection;
                if ui
                    .checkbox(&mut spk, "Enable speaker detection (diarization)")
                    .on_hover_text(
                        "Shows a Speakers dropdown in the Transcribe-file window \
                         (sends diarize + min/num_speakers to whisper).",
                    )
                    .changed()
                {
                    st.cfg.enable_speaker_detection = spk;
                }
            }

            ui.add_space(8.0);
            let mut auto_hold = st.cfg.auto_hold;
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut auto_hold, "Auto-hold after")
                    .on_hover_text("Hold chord this long → app keeps recording.")
                    .changed()
                {
                    st.cfg.auto_hold = auto_hold;
                }
                ui.add_enabled(
                    st.cfg.auto_hold,
                    egui::DragValue::new(&mut st.cfg.auto_hold_secs)
                        .speed(0.1)
                        .clamp_range(0.5..=30.0)
                        .suffix(" s"),
                );
                ui.weak("(2.0 s)");
            });

            let mut cont = st.cfg.continuous;
            if ui
                .checkbox(&mut cont, "Continuous typing (type on every short pause)")
                .on_hover_text("Keep listening; type chunk on each short pause. Ctrl+Win stops.")
                .changed()
            {
                st.cfg.continuous = cont;
            }
            ui.add_enabled_ui(st.cfg.continuous, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.label("Silence threshold");
                    ui.add(
                        egui::DragValue::new(&mut st.cfg.silence_rms_threshold)
                            .speed(0.001)
                            .clamp_range(0.0..=1.0)
                            .max_decimals(4),
                    );
                    ui.weak("(0.01)");
                })
                .response
                .on_hover_text("Mic RMS below this = silence. Also used by Stop.");
            });

            let mut auto_stop = st.cfg.auto_stop;
            if ui
                .checkbox(&mut auto_stop, "Stop on silence (one-shot)")
                .on_hover_text("End recording after silence.")
                .changed()
            {
                st.cfg.auto_stop = auto_stop;
            }
            ui.add_enabled_ui(st.cfg.auto_stop, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.label("Silence duration");
                    ui.add(
                        egui::DragValue::new(&mut st.cfg.stop_silence_secs)
                            .speed(0.1)
                            .clamp_range(0.5..=60.0)
                            .suffix(" s"),
                    );
                    ui.weak("(5.0 s)");
                })
                .response
                .on_hover_text("Pause this long → end session.");
            });

            ui.add_space(8.0);
            let mut lcl = st.cfg.left_click_listen;
            if ui
                .checkbox(&mut lcl, "Left-click tray icon starts command mode")
                .on_hover_text("When on, clicking the tray icon toggles continuous typing + drop non-commands.")
                .changed()
            {
                st.cfg.left_click_listen = lcl;
            }

            ui.add_space(8.0);
            let mut enabled = st.autostart_enabled;
            if ui.checkbox(&mut enabled, "Start at login").changed() {
                if let Ok(exe) = autostart::current_exe_path() {
                    let _ = autostart::set_enabled(enabled, &exe);
                    st.autostart_enabled = autostart::is_enabled();
                }
            }

            ui.add_space(8.0);
            let (color, txt) = if st.health_ok {
                (egui::Color32::from_rgb(80, 200, 100), "online")
            } else {
                (egui::Color32::from_rgb(220, 70, 70), "offline")
            };
            ui.horizontal(|ui| {
                ui.label("Whisper:");
                ui.colored_label(color, egui::RichText::new("\u{25A0}").size(16.0));
                ui.label(txt);
            });

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    st.save_clicked = true;
                    let _ = st.cfg.save();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("Cancel").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(concat!("v", env!("CARGO_PKG_VERSION")));
                });
            });
        });
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}
