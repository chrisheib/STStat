//! Show a custom window frame instead of the default OS window chrome decorations.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::{
    collections::HashMap,
    panic,
    sync::{atomic::AtomicBool, Arc},
    thread,
    time::Instant,
};

use crate::settings::get_screen_size;
use chrono::{Duration, Local, NaiveDateTime};
use circlevec::CircleVec;
use eframe::{
    egui::{self, Label, Layout, RichText, ScrollArea, Visuals},
    epaint::Color32,
};
use ekko::{Ekko, EkkoResponse};
use nvml_wrapper::Nvml;
use parking_lot::Mutex;
use process::{Process, ProcessMetricHandles};
use self_update::{backends::github::Update, cargo_crate_version};
use settings::{show_settings, MySettings};
use sidebar::dispose_sidebar;
use sysinfo::{System, SystemExt};
use system_info::{get_windows_glass_color, init_system, refresh, refresh_color, GpuData, OHWNode};
use tokio::{runtime::Runtime, time::sleep};
use windows::Win32::System::Performance::{PdhCloseQuery, PdhOpenQueryA};

mod autostart;
mod bytes_format;
mod circlevec;
mod color;
mod components;
mod process;
mod settings;
mod sidebar;
mod system_info;

// On read problems, run: lodctr /r
pub const UPDATE_INTERVAL_MILLIS: i64 = 1000;
pub const INTERNAL_WINDOW_TITLE: &str = "RS_Sidebar\0";

// Right Screen, Right side
pub const SIZE: egui::Vec2 = egui::Vec2 {
    x: 130.0,
    y: 1032.0,
};

fn main() -> Result<(), eframe::Error> {
    let mut pdh_query_handle: isize = -1;
    unsafe { PdhOpenQueryA(None, 0, &mut pdh_query_handle) };

    panic::set_hook(Box::new(|p| {
        println!("Custom panic hook: {p}");
        std::fs::write("error.txt", format!("{p}")).unwrap_or_default();
    }));

    let settings = Arc::new(Mutex::new(MySettings::load()));
    let cancel_settings = settings.clone();
    let update_available = Arc::new(AtomicBool::new(false));

    ctrlc::set_handler(move || {
        println!("received Ctrl+C, removing sidebar");
        dispose_sidebar(cancel_settings.clone());
        unsafe { PdhCloseQuery(pdh_query_handle) };
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    let rt = tokio::runtime::Runtime::new().unwrap();
    let ping_buffer = CircleVec::<u64, 100>::new();
    let thread_pb = ping_buffer.clone();
    let ohw_info: Arc<Mutex<Option<OHWNode>>> = Default::default();
    let thread_ohw = ohw_info.clone();

    rt.spawn(ping_thread(thread_pb));
    rt.spawn(ohw_thread(thread_ohw));
    let thread_update_available = update_available.clone();
    thread::spawn(move || check_update_thread(thread_update_available));

    let nvid_info = if let Ok(n) = Nvml::init() {
        Some(n)
    } else {
        None
    };

    let mut appstate = MyApp {
        system_status: System::new_all(),
        ping_buffer,
        firstupdate: false,
        framecount: 0,
        next_update: Default::default(),
        next_screen_update: Default::default(),
        windows_performance_query_handle: pdh_query_handle,
        disk_time_value_handle_map: Default::default(),
        core_time_value_handle_map: Default::default(),
        cpu_buffer: CircleVec::new(),
        cpu_maxtemp_buffer: CircleVec::new(),
        cpu_power_buffer: CircleVec::new(),
        ram_buffer: CircleVec::new(),
        ohw_info,
        rt,
        nvid_info,
        gpu: None,
        timing: CircleVec::new(),
        current_frame_start: Instant::now(),
        cur_ram: 0.0,
        total_ram: 0.0,
        net_up_buffer: Default::default(),
        net_down_buffer: Default::default(),
        gpu_buffer: CircleVec::new(),
        gpu_mem_buffer: CircleVec::new(),
        gpu_power_buffer: CircleVec::new(),
        gpu_temp_buffer: CircleVec::new(),
        show_settings: false,
        settings: settings.clone(),
        disk_buffer: Default::default(),
        processes: vec![],
        process_metric_handles: Default::default(),
        update_available,
    };
    get_screen_size(&appstate);

    let s = settings.lock();
    let initial_window_size = (
        s.current_settings.location.width as f32,
        s.current_settings.location.height as f32,
    );
    let initial_window_pos = (
        s.current_settings.location.x as f32,
        s.current_settings.location.y as f32,
    );
    drop(s);

    let options = eframe::NativeOptions {
        // Hide the OS-specific "chrome" around the window:
        decorated: false,
        // To have rounded corners we need transparency:
        transparent: true,
        min_window_size: Some(egui::vec2(100.0, 100.0)),
        initial_window_size: Some(initial_window_size.into()),
        initial_window_pos: Some(initial_window_pos.into()),
        drag_and_drop_support: false,
        vsync: true,
        ..Default::default()
    };

    init_system(&mut appstate);

    eframe::run_native(
        INTERNAL_WINDOW_TITLE, // title used for identifying window to grab handle
        options,
        Box::new(|cc| {
            let mut v = Visuals::dark();
            v.override_text_color = Some(Color32::from_gray(250));
            v.window_fill = get_windows_glass_color();
            cc.egui_ctx.set_visuals(v);
            Box::new(appstate)
        }),
    )?;

    dispose_sidebar(settings.clone());

    unsafe { PdhCloseQuery(pdh_query_handle) };

    Ok(())
}

async fn ping_thread(thread_pb: Arc<CircleVec<u64, 100>>) -> ! {
    let ekko = Ekko::with_target([8, 8, 8, 8]).unwrap();
    loop {
        if let EkkoResponse::Destination(res) = ekko.send(32).unwrap() {
            thread_pb.add(res.elapsed.as_millis() as u64);
        }
        sleep(
            Duration::milliseconds(
                (1000 - Local::now().naive_local().timestamp_subsec_millis() as i64)
                    .min(1000)
                    .max(520),
            )
            .to_std()
            .unwrap(),
        )
        .await;
    }
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

fn check_update_thread(update_available: Arc<AtomicBool>) -> ! {
    loop {
        if let Ok(status) = Update::configure()
            .repo_owner("chrisheib")
            .repo_name("ststat")
            .bin_name("ststat.exe")
            .current_version(cargo_crate_version!())
            .build()
        {
            let cur_ver = status.current_version();
            let new_ver = status.get_latest_release().unwrap_or_default();
            update_available.store(
                cur_ver != new_ver.version,
                std::sync::atomic::Ordering::Relaxed,
            );
        }
        thread::sleep(std::time::Duration::from_secs(60 * 60));
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum CurrentStep {
    #[default]
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

#[derive(Debug, Default, Clone, Copy)]
pub struct TimingStep {
    pub step: CurrentStep,
    pub time: std::time::Duration,
}

pub struct MyApp {
    pub firstupdate: bool,
    pub framecount: u64,
    pub system_status: System,
    pub next_update: NaiveDateTime,
    pub next_screen_update: NaiveDateTime,
    pub ping_buffer: Arc<CircleVec<u64, 100>>,
    pub cpu_buffer: Arc<CircleVec<f32, 100>>,
    pub cpu_maxtemp_buffer: Arc<CircleVec<f32, 100>>,
    pub cpu_power_buffer: Arc<CircleVec<f64, 100>>,
    pub ram_buffer: Arc<CircleVec<f32, 100>>,
    pub windows_performance_query_handle: isize,
    pub disk_time_value_handle_map: Vec<(String, isize, f64)>,
    pub core_time_value_handle_map: Vec<(usize, isize, f64)>,
    pub nvid_info: Option<Nvml>,
    pub ohw_info: Arc<Mutex<Option<OHWNode>>>,
    pub rt: Runtime,
    pub gpu: Option<GpuData>,
    pub timing: Arc<CircleVec<TimingStep, 2000>>,
    pub current_frame_start: Instant,
    pub cur_ram: f32,
    pub total_ram: f32,
    pub net_up_buffer: HashMap<String, Arc<CircleVec<f64, 100>>>,
    pub net_down_buffer: HashMap<String, Arc<CircleVec<f64, 100>>>,
    pub gpu_buffer: Arc<CircleVec<f64, 100>>,
    pub gpu_mem_buffer: Arc<CircleVec<f64, 100>>,
    pub gpu_power_buffer: Arc<CircleVec<f64, 100>>,
    pub gpu_temp_buffer: Arc<CircleVec<f64, 100>>,
    pub show_settings: bool,
    pub settings: Arc<Mutex<MySettings>>,
    pub disk_buffer: HashMap<String, Arc<CircleVec<f64, 100>>>,
    pub processes: Vec<Process>,
    pub process_metric_handles: ProcessMetricHandles,
    pub update_available: Arc<AtomicBool>,
}

impl eframe::App for MyApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array() // Make sure we don't paint anything behind the rounded corners
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.current_frame_start = Instant::now();
        step_timing(self, CurrentStep::Begin);
        let now = Local::now().naive_local();
        if now > self.next_screen_update {
            get_screen_size(self);
            self.next_screen_update = now + Duration::seconds(20);
        }
        let mut update = false;
        if now > self.next_update {
            refresh(self);
            step_timing(self, CurrentStep::Update);
            self.next_update =
                now + Duration::milliseconds(1000i64 - now.timestamp_subsec_millis() as i64);
            update = true;
        }

        self.framecount += 1;

        let s = self.settings.lock();
        let pos = (
            s.current_settings.location.x as f32,
            s.current_settings.location.y as f32,
        );
        let size = (
            s.current_settings.location.width as f32,
            s.current_settings.location.height as f32,
        );
        drop(s);

        if frame.info().window_info.position != Some(pos.into()) {
            println!(
                "Position weicht ab, old: {:?}, new: {:?}",
                frame.info().window_info.position,
                pos
            );
            frame.set_window_pos(pos.into());
            frame.set_window_size(size.into())
        }

        if !self.firstupdate && self.framecount > 1 {
            println!("Setup sidebar");
            self.firstupdate = true;
            sidebar::setup_sidebar(self);
            let s = self.settings.lock();
            frame.set_window_pos(
                (
                    s.current_settings.location.x as f32,
                    s.current_settings.location.y as f32,
                )
                    .into(),
            );
            drop(s);
            println!("Setup sidebar done");
        }

        custom_window_frame(ctx, frame, "STStat", |ui| {
            if update {
                refresh_color(ui);
            }
            ui.columns(2, |ui| {
                ui[0].add(Label::new(
                    RichText::new(format!("{}", self.framecount)).weak(),
                ));
                ui[1].with_layout(Layout::right_to_left(eframe::emath::Align::TOP), |ui| {
                    ui.add(Label::new(
                        RichText::new(format!("v{}", cargo_crate_version!())).weak(),
                    ));
                });
            });

            let now = chrono::Local::now();
            ui.vertical_centered(|ui| {
                ui.heading(RichText::new(now.format("%H:%M:%S").to_string()).strong())
            });
            ui.separator();

            if self
                .update_available
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                ui.hyperlink_to(
                    RichText::new("Update available!")
                        .color(Color32::RED)
                        .strong(),
                    "https://github.com/chrisheib/ststat/releases",
                );
                ui.separator();
            }

            ScrollArea::vertical().show(ui, |ui| {
                system_info::set_system_info_components(self, ui);
                ui.checkbox(&mut self.show_settings, "Show settings");

                show_settings(self, ui);
            });

            let time_to_next_second = 1000 - chrono::Local::now().timestamp_subsec_millis();

            // guess when the next update should occur.
            ctx.request_repaint_after(
                (chrono::Duration::milliseconds(time_to_next_second as i64 + 5))
                    .to_std()
                    .unwrap(),
            );
        });
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
    use egui::Button;

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
    if appdata.settings.lock().current_settings.track_timings {
        appdata.timing.add(TimingStep {
            step,
            time: appdata.current_frame_start.elapsed(),
        });
    }
}
