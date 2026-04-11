use crate::{
    bytes_format::format_bytes,
    color::auto_color_dark,
    components::{
        edgy_progress::EdgyProgressBar,
        section::{add_graph, SectionMetrics, SIDEBAR_SECTION_GRID_SPACING},
    },
    step_timing, MyApp,
};
use eframe::egui::{Grid, RichText, Ui};
use egui_plot::{Line, PlotPoints};

pub struct Cpu;

impl crate::components::section::Section for Cpu {
    fn name(&self) -> &'static str {
        "CPU"
    }

    fn render(&self, appdata: &mut MyApp, ui: &mut Ui, layout: SectionMetrics) {
        step_timing(appdata, crate::CurrentStep::CpuCrunch);
        ui.spacing_mut().interact_size = [15.0, 12.0].into();

        let cpu = appdata.cpu_buffer.read();
        let last_cpu = cpu.last().copied().unwrap_or_default();

        let maxtemp = appdata.cpu_maxtemp_buffer.read();
        let last_maxtemp = maxtemp.last().copied().unwrap_or_default();
        let half_width = layout.half_lane();
        let full_width = layout.full_width();

        Grid::new("cpu_grid_upper")
            .num_columns(2)
            .spacing([SIDEBAR_SECTION_GRID_SPACING, SIDEBAR_SECTION_GRID_SPACING])
            .striped(true)
            .show(ui, |ui| {
                ui.add(
                    EdgyProgressBar::new(last_cpu / 100.0)
                        .text(
                            RichText::new(format!("CPU: {last_cpu:.0}%",))
                                .small()
                                .strong(),
                        )
                        .desired_width(half_width)
                        .fill(auto_color_dark(0)),
                );
                ui.add(
                    EdgyProgressBar::new(last_maxtemp / 100.0)
                        .text(
                            RichText::new(format!("{last_maxtemp:.0} °C"))
                                .small()
                                .strong(),
                        )
                        .desired_width(half_width)
                        .fill(auto_color_dark(3)),
                );
            });

        ui.add(
            EdgyProgressBar::new(appdata.cur_ram / appdata.total_ram)
                .text(
                    RichText::new(format!(
                        "RAM: {} / {}",
                        format_bytes(appdata.cur_ram as f64),
                        format_bytes(appdata.total_ram as f64)
                    ))
                    .small()
                    .strong(),
                )
                .fill(auto_color_dark(1))
                .desired_width(full_width),
        );
        let power = appdata.cpu_power_buffer.read();
        let current_power = power.last().copied().unwrap_or_default();
        let max_power = 200.0;

        ui.add(
            EdgyProgressBar::new((current_power / max_power) as f32)
                .text(
                    RichText::new(format!("Pow: {current_power:.0}W / {max_power:.0}W",))
                        .small()
                        .strong(),
                )
                .fill(auto_color_dark(2))
                .desired_width(full_width),
        );

        let settings = appdata.settings.lock();
        if !settings.current_settings.hide_cores {
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.spacing_mut().interact_size.y = 4.0;

                Grid::new("cpu_grid_cores")
                    .num_columns(2)
                    .spacing([SIDEBAR_SECTION_GRID_SPACING, 0.0])
                    .striped(false)
                    .show(ui, |ui| {
                        for cpu_chunk in appdata.system_status.cpus().chunks(2) {
                            for cpu in cpu_chunk {
                                let usage = cpu.cpu_usage();
                                ui.add(
                                    EdgyProgressBar::new(usage / 100.0)
                                        .desired_width(half_width)
                                        .text(
                                            RichText::new(format!("{usage:.0}%")).small().strong(),
                                        )
                                        .compact(true),
                                );
                            }
                            ui.end_row();
                        }
                    });
            });
        }
        drop(settings);

        let cpu_line = Line::new(
            (0..appdata.cpu_buffer.capacity())
                .map(|i| [i as f64, { cpu[i] as f64 }])
                .collect::<PlotPoints>(),
        );

        let ram = appdata.ram_buffer.read();
        let ram_line = Line::new(
            (0..appdata.ram_buffer.capacity())
                .map(|i| [i as f64, { ram[i] as f64 * 100.0 }])
                .collect::<PlotPoints>(),
        );

        let power_line = Line::new(
            (0..appdata.cpu_power_buffer.capacity())
                .map(|i| [i as f64, { (power[i] / max_power) * 100.0 }])
                .collect::<PlotPoints>(),
        );

        let temp_line = Line::new(
            (0..appdata.cpu_maxtemp_buffer.capacity())
                .map(|i| [i as f64, { maxtemp[i] as f64 }])
                .collect::<PlotPoints>(),
        );

        step_timing(appdata, crate::CurrentStep::CPU);
        add_graph(
            "cpu",
            ui,
            vec![cpu_line, ram_line, power_line, temp_line],
            &[100.5],
            layout.content_width(),
        );
        step_timing(appdata, crate::CurrentStep::CPUGraph);
    }
}
