use std::{collections::HashMap, fs};

use eframe::egui::Ui;
use serde::{Serialize, Deserialize};
use sysinfo::SystemExt;

use crate::MyApp;

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct MySettings {
    pub loaded_settings: InnerSettings,
    pub current_settings: InnerSettings
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct InnerSettings {
    pub networks: HashMap<String, bool>
}

impl MySettings {
    pub fn load() -> Self {
        let inner: InnerSettings = serde_json::from_str(&fs::read_to_string("conf.json").unwrap_or_default()).unwrap_or_default();
        Self {
            current_settings: inner.clone(),
            loaded_settings: inner
        }
    }

    pub fn save(&self) {
        let j = serde_json::to_string_pretty(&self.current_settings).unwrap_or_default();
        fs::write("conf.json", j).unwrap();
    }
}

pub fn show_settings(appdata: &mut MyApp, ui: &mut Ui) {
    if appdata.settings.current_settings != appdata.settings.loaded_settings {
        appdata.settings.save();
        appdata.settings.loaded_settings = appdata.settings.current_settings.clone();
    }

    ui.label("Show Networks:");
    for (net, _) in appdata.system_status.networks() {
        let e = appdata.settings.current_settings.networks.entry(net.to_string()).or_insert(false);
        ui.checkbox(e, &format!("{net}"));
    }
    ui.separator();
}