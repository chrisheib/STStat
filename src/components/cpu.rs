use crate::{
    bytes_format::format_bytes, color::auto_color_dark, step_timing, system_info::add_graph,
    widgets::edgy_progress::EdgyProgressBar, MyApp, SIDEBAR_WIDTH,
};
use eframe::egui::{
    plot::{Line, PlotPoints},
    Grid, RichText, Ui,
};
use itertools::Itertools;
use sysinfo::{CpuExt, SystemExt};

pub(crate) fn show_cpu(appdata: &mut MyApp, ui: &mut Ui) {
    ui.vertical_centered(|ui| ui.label("CPU"));

    let ohw_opt = appdata.ohw_info.lock();
    let coretemps = if let Some(ohw) = ohw_opt.as_ref() {
        ohw.Children[0]
            .Children
            .iter()
            .find(|n| n.ImageURL == "images_icon/cpu.png")
            .unwrap()
            .Children
            .iter()
            .find(|n| n.Text == "Temperatures")
            .unwrap()
            .Children
            .iter()
            .filter_map(|n| {
                if let Ok(text) = n.Text.replace("CPU Core #", "").parse::<i32>() {
                    Some((
                        text,
                        n.Value
                            .replace("°C", "")
                            .replace(',', ".")
                            .trim()
                            .parse::<f32>()
                            .unwrap_or_default(),
                    ))
                } else {
                    None
                }
            })
            .collect_vec()
    } else {
        vec![]
    };

    let max_temp_line = appdata.cpu_maxtemp_buffer.read();
    let max_temp = max_temp_line.last().copied().unwrap_or_default();

    drop(ohw_opt);

    step_timing(appdata, crate::CurrentStep::CpuCrunch);
    ui.spacing_mut().interact_size = [15.0, 12.0].into();

    let cpu = appdata.cpu_buffer.read();
    let last_cpu = cpu.last().copied().unwrap_or_default();

    Grid::new("cpu_grid_upper")
        .num_columns(2)
        .spacing([2.0, 2.0])
        .striped(true)
        .show(ui, |ui| {
            ui.add(
                EdgyProgressBar::new(last_cpu / 100.0)
                    .text(
                        RichText::new(format!("CPU: {last_cpu:.0}%",))
                            .small()
                            .strong(),
                    )
                    .desired_width(SIDEBAR_WIDTH / 2.0 - 5.0)
                    .fill(auto_color_dark(0)),
            );
            ui.add(
                EdgyProgressBar::new(max_temp / 100.0)
                    .text(RichText::new(format!("{max_temp:.0} °C")).small().strong())
                    .desired_width(SIDEBAR_WIDTH / 2.0 - 5.0)
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
            .fill(auto_color_dark(1)),
    );
    let power = appdata.cpu_power_buffer.read();
    let current_power = power.last().copied().unwrap_or_default();
    let max_power = appdata.settings.lock().current_settings.max_cpu_power;

    ui.add(
        EdgyProgressBar::new((current_power / max_power) as f32)
            .text(
                RichText::new(format!("Pow: {current_power:.0}W / {max_power:.0}W",))
                    .small()
                    .strong(),
            )
            .fill(auto_color_dark(2)),
    );

    Grid::new("cpu_grid_cores")
        .num_columns(2)
        .spacing([2.0, 2.0])
        .striped(true)
        .show(ui, |ui| {
            for (i, cpu_chunk) in appdata.system_status.cpus().chunks(2).enumerate() {
                for cpu in cpu_chunk {
                    let temp = coretemps.get(i).map(|o| o.1).unwrap_or_default();
                    let usage = cpu.cpu_usage();
                    ui.add(
                        EdgyProgressBar::new(usage / 100.0)
                            .desired_width(SIDEBAR_WIDTH / 2.0 - 5.0)
                            .text(
                                RichText::new(format!("{usage:.0}% {temp:.0} °C"))
                                    .small()
                                    .strong(),
                            ),
                    );
                }
                ui.end_row();
            }
        });

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
            .map(|i| [i as f64, { max_temp_line[i] as f64 }])
            .collect::<PlotPoints>(),
    );

    step_timing(appdata, crate::CurrentStep::CPU);
    add_graph(
        "cpu",
        ui,
        vec![cpu_line, ram_line, power_line, temp_line],
        &[100.5],
    );
    step_timing(appdata, crate::CurrentStep::CPUGraph);

    ui.separator();
}
