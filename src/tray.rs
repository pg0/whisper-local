use crate::audio::AudioCapture;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
#[cfg(feature = "transcribe-file")]
use tray_icon::{ClickType, TrayIconEvent};

pub enum TrayEvent {
    OpenSettings,
    Quit,
    SelectMic(String),
    SelectLanguage(String),
    #[cfg(feature = "transcribe-file")]
    OpenTranscribeFile,
}

pub struct Tray {
    _icon: TrayIcon,
    idle: Icon,
    active: Icon,
    settings_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
    /// Mic entries: (CheckMenuItem, mic name where "" = default). Handle kept
    /// so we can toggle checks to simulate radio-group behavior.
    mic_items: Vec<(CheckMenuItem, String)>,
    /// Language entries: (CheckMenuItem, ISO code where "" = auto).
    lang_items: Vec<(CheckMenuItem, String)>,
}

impl Tray {
    pub fn new(current_mic: &str, current_language: &str) -> anyhow::Result<Self> {
        let idle = load_icon(include_bytes!("../assets/tray_idle.png"))?;
        let active = load_icon(include_bytes!("../assets/tray_active.png"))?;

        let menu = Menu::new();

        // Microphone submenu (radio-group behavior enforced manually in try_recv).
        let mic_submenu = Submenu::new("Microphone", true);
        let mut mic_items: Vec<(CheckMenuItem, String)> = Vec::new();

        let default_item =
            CheckMenuItem::new("(default)", true, current_mic.is_empty(), None);
        mic_submenu.append(&default_item)?;
        mic_items.push((default_item, String::new()));

        let devices = AudioCapture::list_input_devices();
        if !devices.is_empty() {
            mic_submenu.append(&PredefinedMenuItem::separator())?;
            for d in devices {
                let checked = d == current_mic;
                let item = CheckMenuItem::new(&d, true, checked, None);
                mic_submenu.append(&item)?;
                mic_items.push((item, d));
            }
        }

        menu.append(&mic_submenu)?;

        // Language submenu (radio-group behavior enforced manually in try_recv).
        let lang_submenu = Submenu::new("Language", true);
        let mut lang_items: Vec<(CheckMenuItem, String)> = Vec::new();
        for (code, label) in crate::config::LANGUAGES {
            let checked = *code == current_language;
            let item = CheckMenuItem::new(*label, true, checked, None);
            lang_submenu.append(&item)?;
            lang_items.push((item, (*code).to_string()));
            if code.is_empty() {
                lang_submenu.append(&PredefinedMenuItem::separator())?;
            }
        }
        menu.append(&lang_submenu)?;
        menu.append(&PredefinedMenuItem::separator())?;

        let settings_item = MenuItem::new("Settings", true, None);
        let quit_item = MenuItem::new("Quit", true, None);
        menu.append(&settings_item)?;
        menu.append(&quit_item)?;

        let settings_id = settings_item.id().clone();
        let quit_id = quit_item.id().clone();

        let icon = TrayIconBuilder::new()
            .with_tooltip("whisper-local")
            .with_icon(idle.clone())
            .with_menu(Box::new(menu))
            .build()?;

        Ok(Self {
            _icon: icon,
            idle,
            active,
            settings_id,
            quit_id,
            mic_items,
            lang_items,
        })
    }

    /// Poll for the next tray/menu event (non-blocking).
    pub fn try_recv(&self) -> Option<TrayEvent> {
        // First check menu events.
        let menu_rx = MenuEvent::receiver();
        if let Ok(e) = menu_rx.try_recv() {
            if e.id == self.settings_id {
                return Some(TrayEvent::OpenSettings);
            } else if e.id == self.quit_id {
                return Some(TrayEvent::Quit);
            }
            if let Some(idx) = self.mic_items.iter().position(|(item, _)| item.id() == &e.id) {
                for (i, (item, _)) in self.mic_items.iter().enumerate() {
                    item.set_checked(i == idx);
                }
                return Some(TrayEvent::SelectMic(self.mic_items[idx].1.clone()));
            }
            if let Some(idx) = self.lang_items.iter().position(|(item, _)| item.id() == &e.id) {
                for (i, (item, _)) in self.lang_items.iter().enumerate() {
                    item.set_checked(i == idx);
                }
                return Some(TrayEvent::SelectLanguage(self.lang_items[idx].1.clone()));
            }
        }
        // Then check icon-click events: left double-click opens file transcribe
        // (only present when the transcribe-file feature is enabled).
        #[cfg(feature = "transcribe-file")]
        {
            let icon_rx = TrayIconEvent::receiver();
            if let Ok(e) = icon_rx.try_recv() {
                if e.click_type == ClickType::Double {
                    return Some(TrayEvent::OpenTranscribeFile);
                }
            }
        }
        None
    }

    pub fn set_active(&mut self, active: bool) {
        let ic = if active { &self.active } else { &self.idle };
        let _ = self._icon.set_icon(Some(ic.clone()));
    }
}

fn load_icon(bytes: &[u8]) -> anyhow::Result<Icon> {
    let img = image::load_from_memory(bytes)?.into_rgba8();
    let (w, h) = img.dimensions();
    Ok(Icon::from_rgba(img.into_raw(), w, h)?)
}
