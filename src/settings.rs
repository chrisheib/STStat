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
    CurrentStep,
    MyApp,
    SIDEBAR_WIDTH,
};

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct MySettings {
    pub loaded_settings: InnerSettings,
    pub current_settings: InnerSettings,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
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
        for (net, _) in &appdata.networks {
            let e = settings
                .current_settings
                .networks
                .entry(net.to_string())
                .or_insert(false);
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
