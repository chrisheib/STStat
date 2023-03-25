//! Show a custom window frame instead of the default OS window chrome decorations.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::{collections::HashMap, fs::File, io::BufWriter, sync::Arc, thread, time::Instant};

use chrono::{Duration, Local, NaiveDateTime};
use circlevec::CircleVec;
use eframe::{
    egui::{self, ScrollArea, Visuals},
    epaint::Color32,
};
use ekko::{Ekko, EkkoResponse};
use nvml_wrapper::Nvml;
use parking_lot::Mutex;
use sidebar::dispose_sidebar;
use sysinfo::{ProcessRefreshKind, System, SystemExt};
use system_info::{get_windows_glass_color, init_system, refresh, GpuData, OHWNode};
use tokio::{runtime::Runtime, time::sleep};

mod autostart;
mod bytes_format;
mod circlevec;
mod components;
mod sidebar;
mod system_info;

// On read problems, run: lodctr /r

pub const UPDATE_INTERVAL_MILLIS: i64 = 1000;
pub const MEASURE_PERFORMANCE: bool = false;
pub const PERFORMANCE_FRAMES: u64 = 50;

// Right Screen, Left side
// pub const SIZE: egui::Vec2 = egui::Vec2 {
//     x: 150.0,
//     y: 1032.0,
// };
// pub const POS: egui::Pos2 = egui::Pos2 { x: 2560.0, y: 0.0 };
// pub const EDGE: u32 = windows::Win32::UI::Shell::ABE_LEFT;

// Right Screen, Right side
pub const SIZE: egui::Vec2 = egui::Vec2 {
    x: 130.0,
    y: 1032.0,
};
pub const POS: egui::Pos2 = egui::Pos2 {
    x: 4480.0 - SIZE.x,
    y: 148.0,
};
pub const EDGE: u32 = windows::Win32::UI::Shell::ABE_RIGHT;

fn main() -> Result<(), eframe::Error> {
    ctrlc::set_handler(move || {
        println!("received Ctrl+C, removing sidebar");
        dispose_sidebar();
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    dbg!(MEASURE_PERFORMANCE);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let ping_buffer = CircleVec::<u64>::new(100);
    let thread_pb = ping_buffer.clone();
    let ohw_info: Arc<Mutex<Option<OHWNode>>> = Default::default();
    let thread_ohw = ohw_info.clone();

    thread::spawn(move || {
        let ekko = Ekko::with_target([8, 8, 8, 8]).unwrap();
        loop {
            if let EkkoResponse::Destination(res) = ekko.send(32).unwrap() {
                thread_pb.add(res.elapsed.as_millis() as u64);
            }
            thread::sleep(
                Duration::milliseconds(
                    (1000 - Local::now().naive_local().timestamp_subsec_millis() as i64)
                        .min(1000)
                        .max(520),
                )
                .to_std()
                .unwrap(),
            );
        }
    });
    rt.spawn(ohw_thread(thread_ohw));

    let options = eframe::NativeOptions {
        // Hide the OS-specific "chrome" around the window:
        decorated: false,
        // To have rounded corners we need transparency:
        transparent: true,
        min_window_size: Some(egui::vec2(100.0, 100.0)),
        initial_window_size: Some(SIZE),
        initial_window_pos: Some(POS),
        drag_and_drop_support: false,
        vsync: false,
        ..Default::default()
    };

    let mut appstate = MyApp {
        system_status: System::new_all(),
        ping_buffer,
        firstupdate: false,
        create_frame: 0,
        framecount: 0,
        next_update: Default::default(),
        next_process_update: Default::default(),
        last_ping_time: Default::default(),
        windows_performance_query_handle: 0,
        disk_time_value_handle_map: Default::default(),
        cpu_buffer: CircleVec::new(100),
        ram_buffer: CircleVec::new(100),
        nvid_info: Nvml::init().unwrap(),
        ohw_info,
        rt,
        gpu: None,
        timing: {
            let mut v = Vec::default();
            if MEASURE_PERFORMANCE {
                v.resize_with(10000, || (CurrentStep::None, std::time::Duration::ZERO));
            }
            v
        },
        current_frame_start: Instant::now(),
        step: 0,
        cur_ram: 0.0,
        total_ram: 0.0,
        net_up_buffer: Default::default(),
        net_down_buffer: Default::default(),
        gpu_buffer: CircleVec::new(100),
        gpu_mem_buffer: CircleVec::new(100),
        gpu_pow_buffer: CircleVec::new(100),
    };

    init_system(&mut appstate);

    eframe::run_native(
        "RS_Sidebar", // unused title
        options,
        Box::new(|cc| {
            let mut v = Visuals::dark();
            v.override_text_color = Some(Color32::from_gray(250));
            v.window_fill = get_windows_glass_color();
            cc.egui_ctx.set_visuals(v);
            Box::new(appstate)
        }),
    )?;

    dispose_sidebar();

    Ok(())
}

async fn ohw_thread(thread_ohw: Arc<Mutex<Option<OHWNode>>>) -> ! {
    loop {
        if let Ok(data) = reqwest::get("http://localhost:8085/data.json").await {
            if let Ok(data) = data.json::<OHWNode>().await {
                *thread_ohw.lock() = Some(data)
            } else {
                *thread_ohw.lock() = None
            }
        } else {
            *thread_ohw.lock() = None
        };
        sleep(
            Duration::milliseconds(
                (1000 - Local::now().naive_local().timestamp_subsec_millis() as i64)
                    .min(999)
                    .max(520),
            )
            .to_std()
            .unwrap(),
        )
        .await;
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum CurrentStep {
    None,
    Begin,
    UpdateCPU,
    UpdateGPU,
    UpdateSystemDisk,
    UpdateSystemMemory,
    UpdateSystemNetwork,
    UpdateSystemProcess,
    UpdateIoTime,
    Update,
    CpuCrunch,
    CPU,
    CPUGraph,
    ProcCPU,
    ProcRAM,
    Ping,
    Network,
    GPU,
}

pub struct MyApp {
    pub firstupdate: bool,
    pub create_frame: u64,
    pub framecount: u64,
    pub system_status: System,
    pub next_update: NaiveDateTime,
    pub next_process_update: NaiveDateTime,
    pub ping_buffer: Arc<CircleVec<u64>>,
    pub cpu_buffer: Arc<CircleVec<f32>>,
    pub ram_buffer: Arc<CircleVec<f32>>,
    pub last_ping_time: std::time::Duration,
    pub windows_performance_query_handle: isize,
    pub disk_time_value_handle_map: Vec<(String, isize, f64)>,
    pub nvid_info: Nvml,
    pub ohw_info: Arc<Mutex<Option<OHWNode>>>,
    pub rt: Runtime,
    pub gpu: Option<GpuData>,
    pub timing: Vec<(CurrentStep, std::time::Duration)>,
    pub current_frame_start: Instant,
    pub step: usize,
    pub cur_ram: f32,
    pub total_ram: f32,
    pub net_up_buffer: HashMap<String, Arc<CircleVec<f64>>>,
    pub net_down_buffer: HashMap<String, Arc<CircleVec<f64>>>,
    pub gpu_buffer: Arc<CircleVec<f64>>,
    pub gpu_mem_buffer: Arc<CircleVec<f64>>,
    pub gpu_pow_buffer: Arc<CircleVec<f64>>,
}

impl eframe::App for MyApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array() // Make sure we don't paint anything behind the rounded corners
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.current_frame_start = Instant::now();
        step_timing(self, CurrentStep::Begin);
        let now = Local::now().naive_local();
        if now > self.next_update {
            refresh(self);
            step_timing(self, CurrentStep::Update);
            self.next_update =
                now + Duration::milliseconds(1000i64 - now.timestamp_subsec_millis() as i64);
        }
        if now > self.next_process_update {
            self.system_status
                .refresh_processes_specifics(ProcessRefreshKind::everything().without_disk_usage());
            // self.system_status.refresh_processes();
            step_timing(self, CurrentStep::UpdateSystemProcess);
            self.next_process_update =
                now + Duration::milliseconds(4000i64 - now.timestamp_subsec_millis() as i64);
        }

        self.framecount += 1;

        if frame.info().window_info.position != Some(POS) {
            println!(
                "Position weicht ab, old: {:?}, new: {:?}",
                frame.info().window_info.position,
                POS
            );
            frame.set_window_pos(POS);
        }

        if !self.firstupdate && self.framecount > 1 {
            self.firstupdate = true;
            self.create_frame = self.framecount;
            sidebar::setup_sidebar();
            frame.set_window_pos(POS);
        }

        custom_window_frame(ctx, frame, "Sidebar", |ui| {
            // ui.label("This is just the contents of the window.");
            // ui.vertical_centered(|ui| {
            //     ui.label("egui theme:");
            //     egui::widgets::global_dark_light_mode_buttons(ui);
            // });
            ui.label(format!("{}", self.framecount));
            ScrollArea::vertical().show(ui, |ui| {
                system_info::set_system_info_components(self, ui);
            });

            let time_to_next_second = 1000 - chrono::Local::now().timestamp_subsec_millis();

            // guess when the next update should occur.
            ctx.request_repaint_after(
                (chrono::Duration::milliseconds(time_to_next_second as i64 + 5))
                    .to_std()
                    .unwrap(),
            );

            // self.last_ping_time = self.ping_channel.try_recv().unwrap_or(self.last_ping_time);
            // ui.label(format!(
            //     "Last ping: {:?}ms",
            //     self.last_ping_time.as_millis()
            // ));
        });

        if MEASURE_PERFORMANCE && self.framecount == PERFORMANCE_FRAMES {
            use std::io::prelude::*;
            let file = File::create("timings.txt").unwrap();
            let mut file = BufWriter::new(file);
            self.timing
                .iter()
                .filter(|(s, _)| s != &CurrentStep::None)
                .for_each(|(s, d)| writeln!(&mut file, "{}: {s:?}", d.as_micros()).unwrap());
            self.timing.clear()
        }
    }
}

fn custom_window_frame(
    ctx: &egui::Context,
    frame: &mut eframe::Frame,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    use egui::*;

    let panel_frame = egui::Frame {
        fill: { get_windows_glass_color() },
        // rounding: 10.0.into(),
        stroke: Stroke {
            width: 0.0,
            color: Color32::TRANSPARENT,
        },
        outer_margin: 0.0.into(), // so the stroke is within the bounds
        ..Default::default()
    };

    CentralPanel::default().frame(panel_frame).show(ctx, |ui| {
        let app_rect = ui.max_rect();

        let title_bar_height = 32.0;
        let title_bar_rect = {
            let mut rect = app_rect;
            rect.max.y = rect.min.y + title_bar_height;
            rect
        };
        title_bar_ui(ui, frame, title_bar_rect, title);

        // Add the contents:
        let content_rect = {
            let mut rect = app_rect;
            rect.min.y = title_bar_rect.max.y;
            rect
        }
        .shrink(4.0);
        let mut content_ui = ui.child_ui(content_rect, *ui.layout());
        add_contents(&mut content_ui);
    });
}

fn title_bar_ui(
    ui: &mut egui::Ui,
    frame: &mut eframe::Frame,
    title_bar_rect: eframe::epaint::Rect,
    title: &str,
) {
    use egui::*;

    let painter = ui.painter();

    // let title_bar_response = ui.interact(title_bar_rect, Id::new("title_bar"), Sense::click());

    // Paint the title:
    painter.text(
        title_bar_rect.center(),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(20.0),
        ui.style().visuals.text_color(),
    );

    // Paint the line under the title:
    painter.line_segment(
        [
            title_bar_rect.left_bottom() + vec2(1.0, 0.0),
            title_bar_rect.right_bottom() + vec2(-1.0, 0.0),
        ],
        ui.visuals().widgets.noninteractive.bg_stroke,
    );

    // Interact with the title bar (drag to move window):
    // if title_bar_response.double_clicked() {
    //     // frame.set_maximized(!frame.info().window_info.maximized);
    // } else if title_bar_response.is_pointer_button_down_on() {
    //     // frame.drag_window();
    // }

    ui.allocate_ui_at_rect(title_bar_rect, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.visuals_mut().button_frame = false;
            ui.add_space(8.0);
            close_maximize_minimize(ui, frame);
        });
    });
}

/// Show some close/maximize/minimize buttons for the native window.
fn close_maximize_minimize(ui: &mut egui::Ui, frame: &mut eframe::Frame) {
    use egui::{Button, RichText};

    let button_height = 12.0;

    let close_response = ui
        .add(Button::new(RichText::new("❌").size(button_height)))
        .on_hover_text("Close the window");
    if close_response.clicked() {
        frame.close();
    }

    // if frame.info().window_info.maximized {
    //     let maximized_response = ui
    //         .add(Button::new(RichText::new("🗗").size(button_height)))
    //         .on_hover_text("Restore window");
    //     if maximized_response.clicked() {
    //         frame.set_maximized(false);
    //     }
    // } else {
    //     let maximized_response = ui
    //         .add(Button::new(RichText::new("🗗").size(button_height)))
    //         .on_hover_text("Maximize window");
    //     if maximized_response.clicked() {
    //         frame.set_maximized(true);
    //     }
    // }

    // let minimized_response = ui
    //     .add(Button::new(RichText::new("🗕").size(button_height)))
    //     .on_hover_text("Minimize the window");
    // if minimized_response.clicked() {
    //     frame.set_minimized(true);
    // }
}

pub fn step_timing(appdata: &mut MyApp, step: CurrentStep) {
    if MEASURE_PERFORMANCE && appdata.framecount < PERFORMANCE_FRAMES {
        appdata.timing[appdata.step] = (step, appdata.current_frame_start.elapsed());
        appdata.step += 1;
    }
}
