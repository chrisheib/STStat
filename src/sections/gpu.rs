use crate::{
    bytes_format::format_bytes,
    color::auto_color_dark,
    components::{
        edgy_progress::EdgyProgressBar,
        section::{add_graph, SectionMetrics, SIDEBAR_SECTION_GRID_SPACING},
    },
    step_timing, CurrentStep, MyApp,
};
use eframe::egui::{Grid, RichText, Ui};
use egui_plot::{Line, PlotPoints};

#[derive(Default, Debug, Clone)]
#[allow(dead_code)]
pub struct GpuData {
    pub utilization: f32,
    pub temperature: f32,
    pub memory_free: f32,
    pub memory_used: f32,
    pub memory_total: f32,
    pub power_usage: f32,
    pub power_limit: f32,
    pub fan_percentage: f32,
    pub clock_mhz: f32,
    pub max_clock: f32,
}

pub fn refresh_gpu(appdata: &mut MyApp) {
    step_timing(appdata, CurrentStep::UpdateGPU);

    if let Some(g) = &appdata.gpu {
        let l = g.lock().unwrap();
        let gpu = l.clone();
        drop(l);

        appdata.gpu_buffer.add(gpu.utilization);
        appdata
            .gpu_mem_buffer
            .add((gpu.memory_used / gpu.memory_total) as f64);
        appdata
            .gpu_power_buffer
            .add((gpu.power_usage / gpu.power_limit) as f64);
        appdata.gpu_temp_buffer.add((gpu.temperature) as f64);
    }

    step_timing(appdata, CurrentStep::UpdateGPU);
}

pub struct Gpu;

impl crate::components::section::Section for Gpu {
    fn name(&self) -> &'static str {
        "GPU"
    }

    fn render(&self, appdata: &mut MyApp, ui: &mut Ui, layout: SectionMetrics) {
        // if appdata.nvid_info.is_some() {
        if let Some(gpu) = &appdata.gpu {
            let l = gpu.lock().unwrap();
            let gpu = l.clone();
            drop(l);
            let half_width = layout.half_lane();
            let full_width = layout.full_width();

            Grid::new("gpu_grid_upper")
                .num_columns(2)
                .spacing([SIDEBAR_SECTION_GRID_SPACING, SIDEBAR_SECTION_GRID_SPACING])
                .striped(true)
                .show(ui, |ui| {
                    ui.add(
                        EdgyProgressBar::new(gpu.utilization / 100.0)
                            .text(
                                RichText::new(format!("GPU: {:.1}%", gpu.utilization))
                                    .small()
                                    .strong(),
                            )
                            .desired_width(half_width)
                            .fill(auto_color_dark(0)),
                    );
                    ui.add(
                        EdgyProgressBar::new(gpu.temperature / 100.0)
                            .text(
                                RichText::new(format!("{:.0} °C", gpu.temperature))
                                    .small()
                                    .strong(),
                            )
                            .desired_width(half_width)
                            .fill(auto_color_dark(3)),
                    );
                });

            ui.add_space(-3.0);

            ui.add(
                EdgyProgressBar::new(gpu.memory_used / gpu.memory_total)
                    .text(
                        RichText::new(format!(
                            "Mem: {} / {}",
                            format_bytes(gpu.memory_used as f64),
                            format_bytes(gpu.memory_total as f64)
                        ))
                        .small()
                        .strong(),
                    )
                    .fill(auto_color_dark(1))
                    .desired_width(full_width),
            );

            ui.add(
                EdgyProgressBar::new(gpu.power_usage / gpu.power_limit)
                    .text(
                        RichText::new(format!(
                            "Pow: {:.0}W / {:.0}W",
                            gpu.power_usage, gpu.power_limit
                        ))
                        .small()
                        .strong(),
                    )
                    .fill(auto_color_dark(2))
                    .desired_width(full_width),
            );
            // ui.add(
            //     EdgyProgressBar::new(gpu.clock_mhz / gpu.max_clock.max(0.01))
            //         .text(
            //             RichText::new(format!(
            //                 "Clk: {:.0}MHz / {:.0}MHz",
            //                 gpu.clock_mhz, gpu.max_clock
            //             ))
            //             .small()
            //             .strong(),
            //         )
            //         .desired_width(SIDEBAR_WIDTH - 8.0),
            // );

            let gpu_buf = appdata.gpu_buffer.read();
            let gpu_line = Line::new(
                (0..appdata.gpu_buffer.capacity())
                    .map(|i| [i as f64, { gpu_buf[i] as f64 }])
                    .collect::<PlotPoints>(),
            );

            let mem_buf = appdata.gpu_mem_buffer.read();
            let mem_line = Line::new(
                (0..appdata.gpu_mem_buffer.capacity())
                    .map(|i| [i as f64, { mem_buf[i] * 100.0 }])
                    .collect::<PlotPoints>(),
            );

            let temp_buf = appdata.gpu_temp_buffer.read();
            let temp_line = Line::new(
                (0..appdata.gpu_temp_buffer.capacity())
                    .map(|i| [i as f64, { temp_buf[i] }])
                    .collect::<PlotPoints>(),
            );

            let pow_buf = appdata.gpu_power_buffer.read();
            let pow_line = Line::new(
                (0..appdata.gpu_power_buffer.capacity())
                    .map(|i| [i as f64, { pow_buf[i] * 100.0 }])
                    .collect::<PlotPoints>(),
            );

            add_graph(
                "gpu",
                ui,
                vec![gpu_line, mem_line, pow_line, temp_line],
                &[100.0],
                layout.content_width(),
            );
        }
    }
}
