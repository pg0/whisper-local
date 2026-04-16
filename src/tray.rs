use crate::audio::AudioCapture;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{ClickType, Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub enum TrayEvent {
    #[cfg(feature = "gui")]
    OpenSettings,
    Quit,
    SelectMic(String),
    SelectLanguage(String),
    ToggleNewlineFeed(bool),
    ToggleCommandMode(bool),
    OpenReplaceMapsFolder,
    ToggleReplaceMaps(bool),
    ToggleReplaceMapFile(String, bool),
    /// Left-click on the tray icon: flip continuous + command_mode together
    /// (turn the app into a pure voice-command surface, or back to passthrough).
    ToggleListen,
    #[cfg(feature = "transcribe-file")]
    OpenTranscribeFile,
}

pub struct Tray {
    _icon: TrayIcon,
    idle: Icon,
    active: Icon,
    #[cfg(feature = "gui")]
    settings_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
    newline_feed_item: CheckMenuItem,
    command_mode_item: CheckMenuItem,
    open_replace_maps_folder_id: tray_icon::menu::MenuId,
    replace_maps_enabled_item: CheckMenuItem,
    replace_map_file_items: Vec<(CheckMenuItem, String)>,
    /// Mic entries: (CheckMenuItem, mic name where "" = default). Handle kept
    /// so we can toggle checks to simulate radio-group behavior.
    mic_items: Vec<(CheckMenuItem, String)>,
    /// Language entries: (CheckMenuItem, ISO code where "" = auto).
    lang_items: Vec<(CheckMenuItem, String)>,
}

impl Tray {
    pub fn new(
        current_mic: &str,
        current_language: &str,
        newline_feed: bool,
        command_mode: bool,
        replace_maps_enabled: bool,
        all_replace_maps: &[String],
        active_replace_maps: &[String],
    ) -> anyhow::Result<Self> {
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

        // Language submenu first (radio-group behavior enforced manually in try_recv).
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
        menu.append(&mic_submenu)?;
        menu.append(&PredefinedMenuItem::separator())?;

        let newline_feed_item =
            CheckMenuItem::new("NewLine", true, newline_feed, None);
        menu.append(&newline_feed_item)?;
        let command_mode_item =
            CheckMenuItem::new("Drop non-commands", true, command_mode, None);
        menu.append(&command_mode_item)?;

        let replace_maps_submenu = Submenu::new("Replace maps", true);
        let replace_maps_enabled_item =
            CheckMenuItem::new("Enabled", true, replace_maps_enabled, None);
        replace_maps_submenu.append(&replace_maps_enabled_item)?;
        replace_maps_submenu.append(&PredefinedMenuItem::separator())?;

        let mut replace_map_file_items: Vec<(CheckMenuItem, String)> = Vec::new();
        for name in all_replace_maps {
            let checked = active_replace_maps.iter().any(|n| n == name);
            let item = CheckMenuItem::new(name, true, checked, None);
            replace_maps_submenu.append(&item)?;
            replace_map_file_items.push((item, name.clone()));
        }
        if !replace_map_file_items.is_empty() {
            replace_maps_submenu.append(&PredefinedMenuItem::separator())?;
        }

        let open_replace_maps_folder_item = MenuItem::new("Config", true, None);
        replace_maps_submenu.append(&open_replace_maps_folder_item)?;
        let open_replace_maps_folder_id = open_replace_maps_folder_item.id().clone();
        menu.append(&replace_maps_submenu)?;
        menu.append(&PredefinedMenuItem::separator())?;

        #[cfg(feature = "gui")]
        let settings_id = {
            let settings_item = MenuItem::new("Settings", true, None);
            menu.append(&settings_item)?;
            settings_item.id().clone()
        };
        let quit_item = MenuItem::new("Quit", true, None);
        menu.append(&quit_item)?;
        let quit_id = quit_item.id().clone();

        let icon = TrayIconBuilder::new()
            .with_tooltip("whisper-local")
            .with_icon(idle.clone())
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()?;

        Ok(Self {
            _icon: icon,
            idle,
            active,
            #[cfg(feature = "gui")]
            settings_id,
            quit_id,
            newline_feed_item,
            command_mode_item,
            open_replace_maps_folder_id,
            replace_maps_enabled_item,
            replace_map_file_items,
            mic_items,
            lang_items,
        })
    }

    /// Poll for the next tray/menu event (non-blocking).
    pub fn try_recv(&self) -> Option<TrayEvent> {
        // First check menu events.
        let menu_rx = MenuEvent::receiver();
        if let Ok(e) = menu_rx.try_recv() {
            #[cfg(feature = "gui")]
            if e.id == self.settings_id {
                return Some(TrayEvent::OpenSettings);
            }
            if e.id == self.quit_id {
                return Some(TrayEvent::Quit);
            }
            if e.id == self.newline_feed_item.id() {
                let enabled = self.newline_feed_item.is_checked();
                return Some(TrayEvent::ToggleNewlineFeed(enabled));
            }
            if e.id == self.command_mode_item.id() {
                let enabled = self.command_mode_item.is_checked();
                return Some(TrayEvent::ToggleCommandMode(enabled));
            }
            if e.id == self.open_replace_maps_folder_id {
                return Some(TrayEvent::OpenReplaceMapsFolder);
            }
            if e.id == self.replace_maps_enabled_item.id() {
                let enabled = self.replace_maps_enabled_item.is_checked();
                return Some(TrayEvent::ToggleReplaceMaps(enabled));
            }
            if let Some((item, name)) = self
                .replace_map_file_items
                .iter()
                .find(|(item, _)| item.id() == &e.id)
            {
                return Some(TrayEvent::ToggleReplaceMapFile(
                    name.clone(),
                    item.is_checked(),
                ));
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
        // Tray icon clicks. Left = toggle listen mode. Double = open the
        // transcribe-file window (when that feature is on).
        let icon_rx = TrayIconEvent::receiver();
        if let Ok(e) = icon_rx.try_recv() {
            log::info!("tray icon click: {:?}", e.click_type);
            match e.click_type {
                #[cfg(feature = "transcribe-file")]
                ClickType::Double => return Some(TrayEvent::OpenTranscribeFile),
                ClickType::Left => return Some(TrayEvent::ToggleListen),
                _ => {}
            }
        }
        None
    }

    pub fn set_newline_feed(&mut self, enabled: bool) {
        self.newline_feed_item.set_checked(enabled);
    }

    pub fn set_command_mode(&mut self, enabled: bool) {
        self.command_mode_item.set_checked(enabled);
    }

    pub fn set_command_mode_locked(&mut self, locked: bool) {
        self.command_mode_item.set_enabled(!locked);
    }

    pub fn set_replace_maps_enabled(&mut self, enabled: bool) {
        self.replace_maps_enabled_item.set_checked(enabled);
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
