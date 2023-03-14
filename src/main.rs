//! Show a custom window frame instead of the default OS window chrome decorations.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::{sync::Arc, thread};

use chrono::{Duration, Local, NaiveDateTime};
use circlevec::CircleVec;
use eframe::egui::{self, ScrollArea};
use ekko::{Ekko, EkkoResponse};
use sidebar::dispose_sidebar;
use sysinfo::{System, SystemExt};

mod autostart;
mod bytes_format;
mod circlevec;
mod sidebar;
mod system_info;

// TODO: nvml-wrapper = "0.9.0"

pub const UPDATE_INTERVAL_MILLIS: i64 = 1000;

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

    // let (tx, rx): (Sender<std::time::Duration>, Receiver<std::time::Duration>) = mpsc::channel();
    // let (mut prod, cons) = SharedRb::<u128, Vec<_>>::new(100).split();
    let ping_buffer = CircleVec::<u128>::new(100);
    let thread_pb = ping_buffer.clone();

    // Each thread will send its id via the channel
    thread::spawn(move || {
        let ekko = Ekko::with_target([8, 8, 8, 8]).unwrap();
        loop {
            if let EkkoResponse::Destination(res) = ekko.send(32).unwrap() {
                // tx.send(res.elapsed).unwrap();
                // println!("Ping answer received: {:?}", res.elapsed);
                // if prod.free_len() = 0 {
                //     // prod.
                // }
                thread_pb.add(res.elapsed.as_millis());
                thread::sleep(std::time::Duration::from_secs(1) - res.elapsed)
            } else {
                thread::sleep(std::time::Duration::from_secs(1))
            };
        }
    });

    let options = eframe::NativeOptions {
        // Hide the OS-specific "chrome" around the window:
        decorated: false,
        // To have rounded corners we need transparency:
        transparent: true,
        min_window_size: Some(egui::vec2(100.0, 100.0)),
        initial_window_size: Some(SIZE),
        initial_window_pos: Some(POS),
        vsync: false,
        ..Default::default()
    };

    eframe::run_native(
        "Sidebar", // unused title
        options,
        Box::new(|_cc| {
            Box::new(MyApp {
                system_status: System::new_all(),
                ping_buffer,
                firstupdate: Default::default(),
                create_frame: Default::default(),
                framecount: Default::default(),
                last_update_timestamp: Default::default(),
                last_ping_time: Default::default(),
            })
        }),
    )?;

    dispose_sidebar();

    Ok(())
}

// type PingBuffer = Consumer<u128, Arc<SharedRb<u128, Vec<MaybeUninit<u128>>>>>;

pub struct MyApp {
    pub firstupdate: bool,
    pub create_frame: u64,
    pub framecount: u64,
    pub system_status: System,
    pub last_update_timestamp: NaiveDateTime,
    pub ping_buffer: Arc<CircleVec<u128>>,
    pub last_ping_time: std::time::Duration,
}

impl eframe::App for MyApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array() // Make sure we don't paint anything behind the rounded corners
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let now = Local::now().naive_local();
        if now - self.last_update_timestamp > Duration::milliseconds(1000) {
            self.system_status.refresh_all();
            self.last_update_timestamp = now;
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

            // guess when the next update should occur.
            ctx.request_repaint_after(
                (chrono::Duration::milliseconds(UPDATE_INTERVAL_MILLIS)
                    - (now - self.last_update_timestamp)
                    + chrono::Duration::milliseconds(10))
                .to_std()
                .unwrap(),
            );

            // self.last_ping_time = self.ping_channel.try_recv().unwrap_or(self.last_ping_time);
            // ui.label(format!(
            //     "Last ping: {:?}ms",
            //     self.last_ping_time.as_millis()
            // ));
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
        fill: ctx.style().visuals.window_fill(),
        // rounding: 10.0.into(),
        stroke: ctx.style().visuals.widgets.noninteractive.fg_stroke,
        outer_margin: 0.5.into(), // so the stroke is within the bounds
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
