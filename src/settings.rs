use std::{
    collections::HashMap,
    fs::{self, File},
    io::BufWriter,
};

use display_info::DisplayInfo;
use eframe::egui::{DragValue, Ui};
use serde::{Deserialize, Serialize};

use crate::{
    // sidebar::{dispose_sidebar, setup_sidebar},
    tasks::{
        cancel_sign_in, has_oauth_client_config, is_awaiting_callback, oauth_client_source_label,
        sign_out, start_sign_in, tasks_status_line,
    },
    CurrentStep,
    MyApp,
    SIDEBAR_WIDTH,
};

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct MySettings {
    pub loaded_settings: InnerSettings,
    pub current_settings: InnerSettings,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(default)]
pub struct InnerSettings {
    pub networks: HashMap<String, bool>,
    pub display_right: bool,
    pub screen_id: usize,
    pub location: Location,
    pub track_timings: bool,
    pub max_cpu_power: f64,
    pub use_plain_dark_background: bool,
    pub hide_cores: bool,
    pub tasks_enabled: bool,
    pub tasks_list_id: String,
    pub tasks_max_items: usize,
    pub tasks_refresh_seconds: u64,
    pub layout_debug_overlay: bool,
}

impl Default for InnerSettings {
    fn default() -> Self {
        Self {
            networks: HashMap::new(),
            display_right: false,
            screen_id: 0,
            location: Location::default(),
            track_timings: false,
            max_cpu_power: 0.0,
            use_plain_dark_background: false,
            hide_cores: false,
            tasks_enabled: false,
            tasks_list_id: "@default".to_string(),
            tasks_max_items: 5,
            tasks_refresh_seconds: 60,
            layout_debug_overlay: false,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct Location {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl MySettings {
    pub fn load() -> Self {
        let inner: InnerSettings =
            serde_json::from_str(&fs::read_to_string("conf.json").unwrap_or_default())
                .unwrap_or_default();
        let s = Self {
            current_settings: inner.clone(),
            loaded_settings: inner,
        };
        s.save();
        s
    }

    pub fn save(&self) {
        let j = serde_json::to_string_pretty(&self.current_settings).unwrap_or_default();
        fs::write("conf.json", j).unwrap();
    }
}

pub fn show_settings(appdata: &mut MyApp, ui: &mut Ui, scale_override: Option<f32>) {
    let mut settings = appdata.settings.lock();
    if settings.current_settings != settings.loaded_settings {
        if settings.current_settings.display_right != settings.loaded_settings.display_right
            || settings.current_settings.screen_id != settings.loaded_settings.screen_id
        {
            drop(settings);
            get_screen_size(appdata, scale_override);
            // dispose_sidebar(appdata.settings.clone());
            // setup_sidebar(appdata, scale_override);
            settings = appdata.settings.lock();
        }
        settings.save();
        settings.loaded_settings = settings.current_settings.clone();
    }
    if appdata.show_settings {
        ui.separator();
        ui.label("Show Networks:");
        for (net, e) in &mut settings.current_settings.networks {
            ui.checkbox(e, net);
        }
        ui.separator();
        ui.label("Screen ID:");
        ui.add(DragValue::new(&mut settings.current_settings.screen_id));
        ui.checkbox(
            &mut settings.current_settings.display_right,
            "Display on right side:",
        );
        ui.separator();
        ui.checkbox(
            &mut settings.current_settings.use_plain_dark_background,
            "Use plain dark background color",
        );
        ui.separator();
        ui.checkbox(&mut settings.current_settings.hide_cores, "Hide CPU Cores");
        ui.separator();
        ui.checkbox(
            &mut settings.current_settings.tasks_enabled,
            "Enable Google Tasks",
        );
        ui.checkbox(
            &mut settings.current_settings.layout_debug_overlay,
            "Layout debug overlay",
        );
        ui.label("Google Tasks List ID:");
        ui.text_edit_singleline(&mut settings.current_settings.tasks_list_id);
        ui.label("Max shown tasks:");
        ui.add(DragValue::new(&mut settings.current_settings.tasks_max_items).range(1..=20));
        ui.label("Tasks refresh seconds:");
        ui.add(
            DragValue::new(&mut settings.current_settings.tasks_refresh_seconds).range(15..=600),
        );
        let tasks_enabled = settings.current_settings.tasks_enabled;
        drop(settings);

        if tasks_enabled {
            ui.label(tasks_status_line(&appdata.tasks_state));

            let has_secret = has_oauth_client_config();
            ui.label(oauth_client_source_label());
            if !has_secret {
                ui.label(
                    eframe::egui::RichText::new(
                        "No usable OAuth client configuration found. Sign in is disabled.",
                    )
                    .color(eframe::epaint::Color32::LIGHT_RED)
                    .small(),
                );
            }

            if is_awaiting_callback(&appdata.tasks_state) {
                // Keep the elapsed waiting time in the status line fresh.
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(250));
            }

            if is_awaiting_callback(&appdata.tasks_state) {
                if ui.button("Cancel login").clicked() {
                    cancel_sign_in(&appdata.tasks_state);
                }
            } else if ui
                .add_enabled(has_secret, eframe::egui::Button::new("Sign in with Google"))
                .clicked()
            {
                start_sign_in(appdata);
                ui.ctx().request_repaint();
            }

            if ui.button("Sign out").clicked() {
                sign_out(&appdata.tasks_state);
            }
        }

        settings = appdata.settings.lock();
        ui.separator();
        ui.checkbox(&mut settings.current_settings.track_timings, "trace perf");
        if ui.button("save trace").clicked() {
            use std::io::prelude::*;
            let file = File::create("timings.txt").unwrap();
            let mut file = BufWriter::new(file);
            appdata
                .timing
                .read()
                .iter()
                .filter(|s| s.step != CurrentStep::None)
                .for_each(|s| writeln!(&mut file, "{}: {:?}", s.time.as_micros(), s.step).unwrap());
        }
    }
    drop(settings);
}

pub fn get_screen_size(appdata: &MyApp, scale_override: Option<f32>) {
    let mut settings = appdata.settings.lock();
    // let workarea_height = dbg!(unsafe { GetSystemMetrics(SM_CYFULLSCREEN) });

    let display_infos = DisplayInfo::all().unwrap();
    for display_info in &display_infos {
        println!("display_info {display_info:?}");
    }
    // panic!();

    // let main_display_height = maindisplay.height;
    // let taskbarsize_main =
    //     (dbg!(main_display_height) as f32 - dbg!(workarea_height) as f32) / dbg!(mainscale);
    let taskbarsize_main = 48.0;
    // println!("Taskbar_height: {taskbarsize_main}");

    let display_id = if settings.current_settings.screen_id < display_infos.len() {
        settings.current_settings.screen_id
    } else {
        0
    };

    let target_display = display_infos[display_id];
    let target_scale = scale_override.unwrap_or(target_display.scale_factor);
    let target_taskbar_size = taskbarsize_main;

    let width = SIDEBAR_WIDTH;
    let height = (target_display.height as f32 / target_scale) - target_taskbar_size;

    let x = if !settings.current_settings.display_right {
        target_display.x as f32
    } else {
        target_display.x as f32 + (target_display.width as f32) - width * target_scale
    };
    let y = target_display.y as f32;

    settings.current_settings.location = Location {
        x,
        y,
        width,
        height,
    }
}

// /// This is a desperate hack to somehow get the monitor sizes of the system. This seems generally not possible in Linux.
// /// Therefore I need to create a new winit loop, which connects to wayland / x11 and can fetch the screen data that way.
// /// I can close out of the eventloop immediately after grabbing the info, but when trying to create a new event loop for
// /// the main app, winit crashes (Can't recreate event loop).
// /// Therefore the checking-winit needs to run in a separate process.
// fn get_screens_linux() -> Vec<MyMonitor> {
//     #[derive(Default)]
//     struct App {
//         window: Option<Window>,
//         screens: Option<Vec<MyMonitor>>,
//     }

//     impl ApplicationHandler for App {
//         fn resumed(&mut self, event_loop: &ActiveEventLoop) {
//             self.window = Some(
//                 event_loop
//                     .create_window(Window::default_attributes())
//                     .unwrap(),
//             );
//         }

//         fn window_event(
//             &mut self,
//             event_loop: &ActiveEventLoop,
//             _id: WindowId,
//             _event: WindowEvent,
//         ) {
//             // dbg!(&event);
//             // dbg!(event_loop);
//             let mut m = vec![];
//             for (id, screen) in event_loop.available_monitors().enumerate() {
//                 // dbg!(screen.name());
//                 // dbg!(screen.position());
//                 // dbg!(screen.size());
//                 let s = MyMonitor {
//                     id,
//                     name: screen.name().unwrap_or_default(),
//                     pos: (screen.position().x as usize, screen.position().y as usize),
//                     size: (screen.size().width as usize, screen.size().height as usize),
//                 };
//                 m.push(s);
//             }
//             self.screens = Some(m);
//             event_loop.exit();
//         }
//     }

//     procspawn::init();

//     let handle = procspawn::spawn((), |_| -> Vec<MyMonitor> {
//         let el = EventLoop::new().unwrap();
//         el.set_control_flow(winit::event_loop::ControlFlow::Wait);

//         let mut app = App::default();
//         el.run_app(&mut app).unwrap();
//         app.screens.unwrap()
//     });
//     let result = handle.join().unwrap();
//     dbg!(&result);
//     result
// }
