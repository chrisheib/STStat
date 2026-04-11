pub mod battery;
pub mod cpu;
pub mod drives;
pub mod gpu;
pub mod network;
pub mod ping;
pub mod processes;
pub mod tasks;

use crate::{components::section::Section, MyApp};
use eframe::egui::Ui;

/// Renders all sidebar sections in canonical order via trait-based dispatch.
pub fn render_sections(appdata: &mut MyApp, ui: &mut Ui) {
    let sections: [&dyn Section; 8] = [
        &cpu::Cpu,
        &gpu::Gpu,
        &drives::Drives,
        &network::Network,
        &ping::Ping,
        &tasks::Tasks,
        &processes::Processes,
        &battery::Battery,
    ];

    for section in sections {
        section.show(appdata, ui);
    }
}
