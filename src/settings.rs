use std::{collections::HashMap, fs};

use display_info::DisplayInfo;
use eframe::egui::{DragValue, Ui};
use serde::{Deserialize, Serialize};
use sysinfo::SystemExt;
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CYFULLSCREEN};

use crate::{
    sidebar::{dispose_sidebar, setup_sidebar},
    MyApp, SIZE,
};

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct MySettings {
    pub loaded_settings: InnerSettings,
    pub current_settings: InnerSettings,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct InnerSettings {
    pub networks: HashMap<String, bool>,
    pub display_right: bool,
    pub screen_id: usize,
    pub location: Location,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct Location {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl MySettings {
    pub fn load() -> Self {
        let inner: InnerSettings =
            serde_json::from_str(&fs::read_to_string("conf.json").unwrap_or_default())
                .unwrap_or_default();
        Self {
            current_settings: inner.clone(),
            loaded_settings: inner,
        }
    }

    pub fn save(&self) {
        let j = serde_json::to_string_pretty(&self.current_settings).unwrap_or_default();
        fs::write("conf.json", j).unwrap();
    }
}

pub fn show_settings(appdata: &mut MyApp, ui: &mut Ui) {
    println!("1");
    let mut settings = appdata.settings.lock();
    if settings.current_settings != settings.loaded_settings {
        if settings.current_settings.display_right != settings.loaded_settings.display_right
            || settings.current_settings.screen_id != settings.loaded_settings.screen_id
        {
            drop(settings);
            get_screen_size(&appdata);
            dispose_sidebar(appdata.settings.clone());
            setup_sidebar(&appdata);
            settings = appdata.settings.lock();
        }
        settings.save();
        settings.loaded_settings = settings.current_settings.clone();
    }

    ui.separator();
    ui.label("Show Networks:");
    for (net, _) in appdata.system_status.networks() {
        let e = settings
            .current_settings
            .networks
            .entry(net.to_string())
            .or_insert(false);
        ui.checkbox(e, &format!("{net}"));
    }
    ui.separator();
    ui.label("Screen ID:");
    ui.add(DragValue::new(&mut settings.current_settings.screen_id));
    ui.checkbox(
        &mut settings.current_settings.display_right,
        "Display on right side:",
    );
    ui.separator();
    drop(settings);
}

/// Returns X, Y, Width, Height
pub fn get_screen_size(appdata: &MyApp) {
    let mut settings = appdata.settings.lock();
    let workarea_height = unsafe { GetSystemMetrics(SM_CYFULLSCREEN) };

    let display_infos = DisplayInfo::all().unwrap();
    // for display_info in &display_infos {
    //   println!("display_info {display_info:?}");
    // }

    let maindisplay = display_infos
        .iter()
        .find(|m| m.is_primary)
        .expect("Es sollte einen primären Monitor geben");

    let mainscale = maindisplay.scale_factor;
    let main_display_height = maindisplay.scale_factor;
    let taskbarsize_main = (main_display_height - workarea_height as f32) / mainscale;

    let display_id;
    if settings.current_settings.screen_id < display_infos.len() {
        display_id = settings.current_settings.screen_id
    } else {
        display_id = 0
    }

    let target_display = display_infos[display_id];

    let target_taskbar_size = taskbarsize_main * target_display.scale_factor;

    let sidebar_height = target_display.height - target_taskbar_size as u32;

    let sidebar_x = if !settings.current_settings.display_right {
        target_display.x
    } else {
        target_display.x + target_display.width as i32 - SIZE.x as i32
    };

    let sidebar_y = target_display.y;

    settings.current_settings.location = Location {
        x: sidebar_x,
        y: sidebar_y,
        width: (SIZE.x as f32 * target_display.scale_factor) as i32,
        height: sidebar_height as i32,
    }
}
