use std::{
    any::Any,
    fmt,
    fs::read_to_string,
    io::{BufRead, BufReader},
    process::Stdio,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use crate::{
    bytes_format::format_bytes,
    circlevec::CircleVec,
    color::{auto_color_dark, get_base_background},
    components::edgy_progress::EdgyProgressBar,
    ohw::MyNode,
    // process::{add_english_counter, get_pdh_process_data, init_process_metrics, Process},
    // sidebar::STATIC_HWND,
    step_timing,
    CurrentStep,
    MyApp,
    SIDEBAR_WIDTH,
};
use chrono::{Local, Timelike};
use eframe::{
    egui::{
        // plot::{Line, Plot, PlotPoints},
        style,
        vec2,
        Grid,
        Label,
        Layout,
        Margin,
        Rect,
        RichText,
        Sense,
        Stroke,
        Ui,
        Vec2,
    },
    emath::Align::{self, Max},
    epaint::Color32,
};
use egui_extras::{Column, TableBuilder};
use egui_plot::{Line, Plot, PlotPoints};
use itertools::Itertools;
// use nvml_wrapper::enum_wrappers::device::{Clock, ClockId, TemperatureSensor};
use sysinfo::{CpuRefreshKind, Pid};
use tokio::process::Command;
// use windows::{
//     core::PWSTR,
//     w,
//     Win32::{
//         Foundation::BOOL,
//         Graphics::Dwm::DwmGetColorizationColor,
//         System::Performance::{
//             PdhBrowseCountersW, PdhCollectQueryData, PdhGetFormattedCounterValue,
//             PDH_BROWSE_DLG_CONFIG_W, PDH_FMT_DOUBLE, PERF_DETAIL_WIZARD,
//         },
//     },
// };

pub fn set_system_info_components(appdata: &mut MyApp, ui: &mut Ui) {
    step_timing(appdata, crate::CurrentStep::Begin);

    show_cpu(appdata, ui);
    show_gpu(appdata, ui);
    show_drives(appdata, ui);
    show_network(appdata, ui);
    show_ping(appdata, ui);
    show_processes(appdata, ui);
    show_battery(appdata, ui);
}

fn show_network(appdata: &mut MyApp, ui: &mut Ui) {
    ui.vertical_centered(|ui| ui.label("Networks"));

    for (interface_name, data) in filter_networks(appdata) {
        // ui.push_id(format!("network graph {interface_name}"), |ui| {
        //     let table = TableBuilder::new(ui)
        //         .striped(true)
        //         .columns(Column::exact((SIDEBAR_WIDTH - 10.0) * 0.4), 2);
        //     table.header(10.0, |mut header| {
        //         header.col(|ui| {
        //             ui.add(
        //                 Label::new(
        //                     RichText::new(format!("⬆ {}", format_bytes(data.tx))).size(12.0),
        //                 )
        //                 .wrap(false),
        //             );
        //         });
        //         header.col(|ui| {
        //             ui.add(
        //                 Label::new(
        //                     RichText::new(format!("⬇ {}", format_bytes(data.rx))).size(12.0),
        //                 )
        //                 .wrap(false),
        //             );
        //         });
        //     });
        // });

        let up_buffer = appdata
            .net_up_buffer
            .entry(interface_name.clone())
            .or_insert(CircleVec::new());
        let up = up_buffer.read();

        // let up_line = Line::new(
        //     (0..up_buffer.capacity())
        //         .map(|i| [i as f64, { up[i] }])
        //         .collect::<PlotPoints>(),
        // );

        let down_buffer = appdata
            .net_down_buffer
            .entry(interface_name.clone())
            .or_insert(CircleVec::new());
        let down = down_buffer.read();

        // let down_line = Line::new(
        //     (0..down_buffer.capacity())
        //         .map(|i| [i as f64, { down[i] }])
        //         .collect::<PlotPoints>(),
        // );

        ui.add_space(3.0);

        let max_down = down
            .iter()
            .reduce(|acc, v| if acc > v { acc } else { v })
            .copied()
            .unwrap_or_default();
        let max_up = up
            .iter()
            .reduce(|acc, v| if acc > v { acc } else { v })
            .copied()
            .unwrap_or_default();

        // add_graph(
        //     "network",
        //     ui,
        //     vec![down_line, up_line],
        //     &[14.0 * 1024.0 * 1024.0, max_down, max_up],
        // );
    }
    ui.separator();
    step_timing(appdata, crate::CurrentStep::Network);
}

fn filter_networks(appdata: &mut MyApp) -> Vec<(String, MyNetworkData)> {
    appdata
        .networks
        .iter()
        .filter(|i| {
            *appdata
                .settings
                .lock()
                .current_settings
                .networks
                .entry(i.0.to_string())
                .or_default()
        })
        .map(|(n, d)| {
            (
                n.to_string(),
                MyNetworkData {
                    tx: d.transmitted() as f64,
                    rx: d.received() as f64,
                },
            )
        })
        .collect_vec()
}

#[derive(Default, Debug, Clone)]
#[allow(dead_code)]
pub struct GpuData {
    utilization: f32,
    temperature: f32,
    memory_free: f32,
    memory_used: f32,
    memory_total: f32,
    power_usage: f32,
    power_limit: f32,
    fan_percentage: f32,
    clock_mhz: f32,
    max_clock: f32,
}

fn timing_to_str(timestamp: std::time::Instant, text: &mut String, perf_trace: bool) {
    if perf_trace {
        *text += &format!("{}\n", timestamp.elapsed().as_micros());
    }
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

// FIX

fn show_processes(appdata: &mut MyApp, ui: &mut Ui) {
    ui.vertical_centered(|ui| ui.label("Processes"));

    let p = appdata
        .system_status
        .processes()
        .iter()
        .map(|a| Process {
            cpu: a.1.cpu_usage(),
            memory: a.1.memory(),
            name: a.1.name().to_str().unwrap_or_default().to_string(),
            pid: a.1.pid(),
            parent: a.1.parent(),
        })
        .collect_vec();

    // By CPU
    let mut cpu_p = p.clone();
    cpu_p.sort_unstable_by(|a, b| b.cpu.total_cmp(&a.cpu));
    let cpu_count = appdata.system_status.cpus().len();
    add_process_table(
        ui,
        5,
        &cpu_p,
        "Proc CPU",
        ProcessTableDisplayMode::Cpu,
        cpu_count,
    );
    step_timing(appdata, crate::CurrentStep::ProcCPU);

    // By Memory

    // FIXME: Ordnen und gruppieren nach Parent
    // 1 systemd -> 1243 systemd -> reelle apps
    // für alle Enkel von 1:
    // alle Kinder zusammenrechnen?
    let mut mem_p = p.clone();
    mem_p.sort_unstable_by(|a, b| b.memory.cmp(&a.memory));
    add_process_table(
        ui,
        5,
        &mem_p,
        "Proc Ram",
        ProcessTableDisplayMode::Ram,
        cpu_count,
    );

    // for p in &mem_p {
    //     println!("{p}");
    // }
    // panic!();

    step_timing(appdata, crate::CurrentStep::ProcRAM);
}

fn show_ping(appdata: &mut MyApp, ui: &mut Ui) {
    ui.vertical_centered(|ui| ui.label("Ping"));
    let pings = appdata.ping_buffer.read();
    let last_ping = pings.last().copied().unwrap_or_default();
    let max_ping = pings.iter().max().copied().unwrap_or_default();
    // let line = Line::new(
    //     (0..appdata.ping_buffer.capacity())
    //         .map(|i| [i as f64, { pings[i] as f64 }])
    //         .collect::<PlotPoints>(),
    // );

    let lp_str = if last_ping == 0 {
        "ERR".to_string()
    } else {
        format!("{last_ping:.0} ms")
    };

    ui.label(RichText::new(format!("M: {max_ping:.0}ms, C: {lp_str}")).size(12.0));
    // add_graph("ping", ui, vec![line], &[50.0, max_ping as f64]);
    step_timing(appdata, crate::CurrentStep::Ping);
    ui.separator();
}

fn show_battery(appdata: &mut MyApp, ui: &mut Ui) {
    if !appdata.battery_enabled {
        return;
    }
    ui.vertical_centered(|ui| ui.label("Battery"));
    let level = appdata.battery_level_buffer.read();
    // let level_line = Line::new(
    //     (0..appdata.battery_level_buffer.capacity())
    //         .map(|i| {
    //             [i as f64, {
    //                 (if level[i] == 0.0 { 100.0 } else { level[i] }) - 50.0
    //             }]
    //         })
    //         .collect::<PlotPoints>(),
    // );
    // let charge = appdata.battery_change_buffer.read();
    // let charge_line = Line::new(
    //     (0..appdata.battery_change_buffer.capacity())
    //         .map(|i| [i as f64, { charge[i] * 25.0 }])
    //         .collect::<PlotPoints>(),
    // );

    // add_graph("battery", ui, vec![level_line, charge_line], &[-50.0, 50.0]);
    step_timing(appdata, crate::CurrentStep::Ping);
    ui.separator();
}

fn show_cpu(appdata: &mut MyApp, ui: &mut Ui) {
    ui.vertical_centered(|ui| ui.label("CPU"));

    step_timing(appdata, crate::CurrentStep::CpuCrunch);
    ui.spacing_mut().interact_size = [15.0, 12.0].into();

    let cpu = appdata.cpu_buffer.read();
    let last_cpu = cpu.last().copied().unwrap_or_default();

    let maxtemp = appdata.cpu_maxtemp_buffer.read();
    let last_maxtemp = maxtemp.last().copied().unwrap_or_default();

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
                EdgyProgressBar::new(last_maxtemp / 100.0)
                    .text(
                        RichText::new(format!("{last_maxtemp:.0} °C"))
                            .small()
                            .strong(),
                    )
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
            for (_i, cpu_chunk) in appdata.system_status.cpus().chunks(2).enumerate() {
                for cpu in cpu_chunk {
                    // let temp = appdata.coretemps.get(i).map(|o| o.1).unwrap_or_default();
                    let usage = cpu.cpu_usage();
                    ui.add(
                        EdgyProgressBar::new(usage / 100.0)
                            .desired_width(SIDEBAR_WIDTH / 2.0 - 5.0)
                            .text(
                                // RichText::new(format!("{usage:.0}% {temp:.0} °C"))
                                RichText::new(format!("{usage:.0}%")).small().strong(),
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
            .map(|i| [i as f64, { maxtemp[i] as f64 }])
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

fn show_gpu(appdata: &MyApp, ui: &mut Ui) {
    // if appdata.nvid_info.is_some() {
    if let Some(gpu) = &appdata.gpu {
        let l = gpu.lock().unwrap();
        let gpu = l.clone();
        drop(l);
        ui.vertical_centered(|ui| ui.label("GPU"));

        Grid::new("gpu_grid_upper")
            .num_columns(2)
            .spacing([2.0, 2.0])
            .striped(true)
            .show(ui, |ui| {
                ui.add(
                    EdgyProgressBar::new(gpu.utilization as f32 / 100.0)
                        .text(
                            RichText::new(format!("GPU: {:.1}%", gpu.utilization))
                                .small()
                                .strong(),
                        )
                        .desired_width(SIDEBAR_WIDTH / 2.0 - 5.0)
                        .fill(auto_color_dark(0)),
                );
                ui.add(
                    EdgyProgressBar::new(gpu.temperature / 100.0)
                        .text(
                            RichText::new(format!("{:.0} °C", gpu.temperature))
                                .small()
                                .strong(),
                        )
                        .desired_width(SIDEBAR_WIDTH / 2.0 - 5.0)
                        .fill(auto_color_dark(3)),
                );
            });

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
                .fill(auto_color_dark(1)),
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
                .fill(auto_color_dark(2)),
        );
        ui.add(
            EdgyProgressBar::new(gpu.clock_mhz / gpu.max_clock.max(0.01)).text(
                RichText::new(format!(
                    "Clk: {:.0}MHz / {:.0}MHz",
                    gpu.clock_mhz, gpu.max_clock
                ))
                .small()
                .strong(),
            ),
        );

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
        );

        ui.separator();
    }
}

#[derive(PartialEq, Eq)]
enum ProcessTableDisplayMode {
    All,
    Cpu,
    Ram,
}

#[derive(Clone, Debug)]
struct Process {
    cpu: f32,
    memory: u64,
    name: String,
    pid: Pid,
    parent: Option<Pid>,
}

// Implement `Display` for `MinMax`.
impl fmt::Display for Process {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Use `self.number` to refer to each positional data point.
        write!(
            f,
            "{}, {}, {}, {}, {:?}",
            self.pid, self.name, self.cpu, self.memory, self.parent
        )
    }
}

fn add_process_table(
    ui: &mut Ui,
    len: usize,
    p: &[Process],
    name: &str,
    display_mode: ProcessTableDisplayMode,
    core_count: usize,
) {
    let mut clicked = false;
    ui.push_id(name, |ui| {
        let mut table = TableBuilder::new(ui).striped(true).column(Column::exact(
            (SIDEBAR_WIDTH - 10.0)
                * if display_mode == ProcessTableDisplayMode::All {
                    0.4
                } else {
                    0.63
                },
        ));
        if display_mode == ProcessTableDisplayMode::All
            || display_mode == ProcessTableDisplayMode::Ram
        {
            table = table.column(Column::exact((SIDEBAR_WIDTH - 10.0) * 0.3))
        };
        if display_mode == ProcessTableDisplayMode::All
            || display_mode == ProcessTableDisplayMode::Cpu
        {
            table = table.column(Column::exact((SIDEBAR_WIDTH - 10.0) * 0.3))
        };
        let table = table.header(10.0, |mut header| {
            header.col(|ui| {
                clicked = clicked
                    || ui
                        .add(Label::new(RichText::new(name).small()).wrap())
                        .interact(Sense::click())
                        .double_clicked();
            });
            if display_mode == ProcessTableDisplayMode::All
                || display_mode == ProcessTableDisplayMode::Ram
            {
                header.col(|ui| {
                    ui.with_layout(Layout::top_down_justified(Max), |ui| {
                        clicked = clicked
                            || ui
                                .add(Label::new(RichText::new("RAM").small()).wrap())
                                .interact(Sense::click())
                                .double_clicked();
                    });
                });
            }
            if display_mode == ProcessTableDisplayMode::All
                || display_mode == ProcessTableDisplayMode::Cpu
            {
                header.col(|ui| {
                    ui.with_layout(Layout::top_down_justified(Max), |ui| {
                        clicked = clicked
                            || ui
                                .add(Label::new(RichText::new("CPU").small()).wrap())
                                .interact(Sense::click())
                                .double_clicked();
                    });
                });
            }
        });
        table.body(|body| {
            body.rows(10.0, len, |mut row| {
                let row_index = row.index();
                if row_index < p.len() {
                    let p = &p[row_index];
                    row.col(|ui| {
                        clicked = clicked
                            || ui
                                .add(
                                    Label::new(
                                        RichText::new(format!(
                                            "{}{}",
                                            p.name,
                                            0 // FIXME

                                              // if p.count > 1 {
                                              //     format!(" ×{}", p.count)
                                              // } else {
                                              //     "".to_string()
                                              // }
                                        ))
                                        .small()
                                        .strong(),
                                    )
                                    .wrap(),
                                )
                                .interact(Sense::click())
                                .double_clicked();
                    });
                    if display_mode == ProcessTableDisplayMode::All
                        || display_mode == ProcessTableDisplayMode::Ram
                    {
                        row.col(|ui| {
                            ui.with_layout(Layout::top_down_justified(Max), |ui| {
                                clicked = clicked
                                    || ui
                                        .add(
                                            Label::new(
                                                RichText::new(format_bytes(p.memory as f64))
                                                    .small()
                                                    .strong(),
                                            )
                                            .wrap(),
                                        )
                                        .interact(Sense::click())
                                        .double_clicked()
                            });
                        });
                    }
                    if display_mode == ProcessTableDisplayMode::All
                        || display_mode == ProcessTableDisplayMode::Cpu
                    {
                        row.col(|ui| {
                            ui.with_layout(Layout::top_down_justified(Max), |ui| {
                                clicked = clicked
                                    || ui
                                        .add(
                                            Label::new(
                                                RichText::new(format!(
                                                    "{:.1}%",
                                                    p.cpu as f32 / core_count as f32
                                                ))
                                                .small()
                                                .strong(),
                                            )
                                            .wrap(),
                                        )
                                        .interact(Sense::click())
                                        .double_clicked();
                            });
                        });
                    }
                }
            });
        });
    });
    ui.separator();

    if clicked {
        match Command::new("powershell")
            .args(["start", "taskmgr", "-v runAs"])
            .spawn()
        {
            Ok(_c) => println!("Starting Task Manager"),
            Err(e) => println!("{e}"),
        };
    }
}

fn add_graph(id: &str, ui: &mut Ui, line: Vec<Line>, max_y: &[f64]) {
    // let desired_size = vec2(SIDEBAR_WIDTH - 7.0, 30.0);
    // let desired_position = ui.min_rect().min; // Starting at top-left corner

    // let rect = Rect::from_min_size(desired_position, desired_size);

    // // Draw a background to visualize the allocated space
    // ui.painter().rect_filled(rect, 0.0, Color32::LIGHT_BLUE);

    // ui.allocate_ui_at_rect(rect, |ui| {
    //     let mut plot = Plot::new(id)
    //         // Your plot configuration
    //         .set_margin_fraction(vec2(0.0, 0.0));

    //     for y in max_y {
    //         plot = plot.include_y(*y);
    //     }

    //     plot.show(ui, |plot_ui| {
    //         for l in line {
    //             plot_ui.line(l);
    //         }
    //     });
    // });

    // let desired_size = egui::vec2(SIDEBAR_WIDTH - 7.0, 30.0);
    // let desired_position = ui.min_rect().min; // Starting at top-left corner

    // let rect = egui::Rect::from_min_size(desired_position, desired_size);

    // // Draw a background to visualize the allocated space
    // ui.painter().rect_filled(rect, 0.0, egui::Color32::LIGHT_BLUE);

    // ui.allocate_ui_at_rect(rect, |ui| {
    // let mut plot = Plot::new(id)
    //     // Your plot configuration
    //     .set_margin_fraction(vec2(0.0, 0.0));

    // for y in max_y {
    //     plot = plot.include_y(*y);
    // }

    // plot.show(ui, |plot_ui| {
    //     for l in line {
    //         plot_ui.line(l);
    //     }
    // });

    //     // let desired_size = vec2(130.0, 30.0);
    // // let rect = Rect::from_min_size(ui.min_rect().min, desired_size);

    // // ui.painter().rect_filled(rect, 0.0, Color32::YELLOW);
    // let desired_size = vec2(SIDEBAR_WIDTH - 7.0 * 1.6, 30.0 * 1.6);
    // let (rect, _) = ui.allocate_exact_size(desired_size, Sense::hover());

    // ui.allocate_ui_at_rect(rect, |iui| {
    //     let mut p = Plot::new(id)
    //         // .show_axes([true, true])
    //         // .label_formatter(|_, _| "".to_string())
    //         // .allow_drag(false)
    //         // .allow_zoom(false)
    //         // .allow_scroll(false)
    //         // .allow_boxed_zoom(false)
    //         // .allow_double_click_reset(false)
    //         // .show_x(false)
    //         // .show_y(false)
    //         // .x_axis_formatter(|_, _| String::new())
    //         // .y_axis_formatter(|_, _| String::new())
    //         // .min_size(desired_size)
    //         // .set_margin_fraction((0.0, 0.0).into())
    //         // .width(desired_size.x)
    //         // .height(desired_size.y)
    //         // .include_y(0.0)
    //         ;

    //     // for y in max_y {
    //     //     p = p.include_y(*y);
    //     // }

    //     p.show(iui, |plot_ui| {
    //         // iui.painter().rect_stroke(
    //         //     plot_ui.ctx().available_rect(),
    //         //     0.0,
    //         //     Stroke::new(1.0, Color32::GREEN),
    //         //     eframe::egui::StrokeKind::Inside,
    //         // );
    //         // for l in line {
    //         //     plot_ui.line(l)
    //         // }
    //     });
    // });

    let desired_size = vec2(SIDEBAR_WIDTH - 7.0 * 1.6, 30.0 * 1.6);

    let layout = Layout::top_down(Align::Min);

    ui.with_layout(layout, |ui| {
        // Temporarily set spacing to zero
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.spacing_mut().window_margin = Margin::ZERO;
        ui.spacing_mut().button_padding = Vec2::ZERO;
        ui.spacing_mut().indent = 0.0;

        let (rect, _) = ui.allocate_exact_size(desired_size, Sense::hover());
        // ui.painter().rect_stroke(
        //     rect,
        //     0.0,
        //     Stroke::new(1.0, Color32::RED),
        //     eframe::egui::StrokeKind::Inside,
        // );
        let mut child_ui = ui.child_ui(rect, *ui.layout(), None);

        let mut p = Plot::new(id)
            .show_axes([true, true])
            .label_formatter(|_, _| "".to_string())
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .allow_double_click_reset(false)
            .show_x(false)
            .show_y(false)
            .x_axis_formatter(|_, _| String::new())
            .y_axis_formatter(|_, _| String::new())
            .min_size(desired_size)
            .set_margin_fraction((0.0, 0.0).into())
            .width(desired_size.x)
            .height(desired_size.y)
            .include_y(0.0);

        for y in max_y {
            p = p.include_y(*y);
        }

        p.show(&mut child_ui, |plot_ui| {
            // ui.painter().rect_stroke(
            //     plot_ui.ctx().available_rect(),
            //     0.0,
            //     Stroke::new(1.0, Color32::GREEN),
            //     eframe::egui::StrokeKind::Inside,
            // );
            for l in line {
                plot_ui.line(l)
            }
        });
    });
}

fn show_drives(appdata: &MyApp, ui: &mut Ui) {
    ui.vertical_centered(|ui| ui.label("Drives"));
    Grid::new("drive_grid")
        .spacing([2.0, 2.0])
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for (i, d) in appdata
                .disks
                .iter()
                .sorted_by_key(|d| d.mount_point())
                .filter(|d| d.file_system() != "vfat")
                .group_by(|d| d.name())
                .into_iter()
                .map(|(_, g)| g.into_iter().next().unwrap())
                .enumerate()
            {
                ui.spacing_mut().interact_size = [15.0, 12.0].into();

                let replace = d.mount_point().to_str().unwrap().replace('\\', "");
                let collect_vec = replace.split("/").collect_vec();
                let mount = collect_vec.last().unwrap();

                // FIXME: rework disk io system in linux
                // let (_, _, value) = appdata
                //     .disk_time_value_handle_map
                //     .iter()
                //     .find(|(s, _, _)| s == mount)
                //     .unwrap();

                ui.add(Label::new(
                    RichText::new(format!(
                        // FIXME: rework disk io system in linux
                        // "/{} {value:.1}%",
                        "/{}",
                        if mount.len() < 12 {
                            mount.to_string()
                        } else {
                            "...".to_string() + &mount[mount.len() - 12..]
                        }
                    ))
                    .small()
                    .strong(),
                ));

                // ui.add_space(5.0);
                // ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(
                    EdgyProgressBar::new(
                        (d.total_space() - d.available_space()) as f32 / d.total_space() as f32,
                    )
                    .desired_width(appdata.settings.lock().current_settings.location.width * 0.55)
                    .text(
                        RichText::new(format!(
                            "Free: {}",
                            format_bytes(d.available_space() as f64),
                        ))
                        .small()
                        .strong(),
                    )
                    .fill(auto_color_dark(i as i32)),
                );
                // });
                ui.end_row();
            }
        });
    ui.spacing();

    // let mut lines = Vec::new();
    // for (_d, diskbuffer) in appdata.disk_buffer.iter().sorted_by_key(|h| h.0) {
    //     let values = diskbuffer.read();
    //     lines.push(Line::new(
    //         (0..diskbuffer.capacity())
    //             .map(|i| [i as f64, { values[i] }])
    //             .collect::<PlotPoints>(),
    //     ));
    // }

    // add_graph("disk", ui, lines, &[100.5]);

    ui.separator();
}

// FIX
fn refresh_disk_io_time(appdata: &mut MyApp) {
    // unsafe {
    //     // Siehe: https://learn.microsoft.com/en-us/windows/win32/perfctrs/pdh-error-codes
    //     for (d, handle, value) in &mut appdata.disk_time_value_handle_map {
    //         let mut new_value = Default::default();
    //         PdhGetFormattedCounterValue(*handle, PDH_FMT_DOUBLE, None, &mut new_value);
    //         *value = new_value.Anonymous.doubleValue;
    //         appdata
    //             .disk_buffer
    //             .entry(d.clone())
    //             .or_insert(CircleVec::new())
    //             .add(*value);
    //     }
    // }
}

pub fn init_system(appdata: &mut MyApp) {
    // open_performance_browser();

    // appdata.process_metric_handles = init_process_metrics(appdata.windows_performance_query_handle);
    appdata.disks.refresh(true);

    // println!();
    // for ele in appdata.disks.iter() {
    //     // dbg!(ele);
    //     dbg!(ele.name());
    //     dbg!(&ele.mount_point());
    //     dbg!(ele.file_system());
    //     println!();
    // }
    // panic!();

    appdata.system_status.refresh_cpu_all();

    // iterate over disks and add disk io time counters
    for d in appdata.disks.iter().sorted_by_key(|d| d.mount_point()) {
        let drive_letter = d.mount_point().to_str().unwrap().replace('\\', "");
        // let metric_handle = add_english_counter(
        //     format!(r"\LogicalDisk({drive_letter})\% Disk Time"),
        //     appdata.windows_performance_query_handle,
        // );

        appdata
            .disk_time_value_handle_map
            .push((drive_letter, 0, 0.0));
    }

    // unsafe { PdhCollectQueryData(appdata.windows_performance_query_handle) };
}

pub fn get_windows_glass_color(use_plain_blackground: bool) -> Color32 {
    if use_plain_blackground {
        return get_base_background();
    }
    let mut col: u32 = 0;
    // let mut opaque: BOOL = BOOL(0);
    // unsafe {
    //     DwmGetColorizationColor(&mut col, &mut opaque).unwrap();
    // }
    let bytes: [u8; 4] = col.to_be_bytes();
    Color32::from_rgba_premultiplied(
        darken(bytes[1]),
        darken(bytes[2]),
        darken(bytes[3]),
        bytes[0],
    )
}

fn darken(v: u8) -> u8 {
    (v as f32 * 0.4) as u8
}

// FIX
#[allow(dead_code)]
pub fn open_performance_browser() {
    unsafe {
        // let hwnd = *STATIC_HWND.read().unwrap();
        // let mut buf: [u16; 10000] = [0; 10000];
        // let returnpathbuffer = PWSTR::from_raw(&mut buf as *mut u16);
        // let p = PWSTR::from_raw(w!("hello").as_ptr() as *mut _);
        // PdhBrowseCountersW(&PDH_BROWSE_DLG_CONFIG_W {
        //     _bitfield: 0,
        //     hWndOwner: hwnd,
        //     szDataSource: PWSTR::null(),
        //     szReturnPathBuffer: returnpathbuffer,
        //     cchReturnPathLength: 10000,
        //     pCallBack: None,
        //     dwCallBackArg: 0,
        //     CallBackStatus: 0,
        //     dwDefaultDetailLevel: PERF_DETAIL_WIZARD,
        //     szDialogBoxCaption: p,
        // } as *const PDH_BROWSE_DLG_CONFIG_W);

        // println!("{}", returnpathbuffer.display());
    }
}

// fn convert_to_pcwstr(s: &str) -> PCWSTR {
//     let mut v = s.encode_utf16().collect_vec();
//     v.push(0);
//     let p = v.as_ptr();
//     PCWSTR::from_raw(p)
// }

pub fn refresh(appdata: &mut MyApp) {
    // refresh windows perfcount stats once
    // unsafe { PdhCollectQueryData(appdata.windows_performance_query_handle) };

    refresh_cpu(appdata);
    step_timing(appdata, CurrentStep::UpdateCPU);

    refresh_gpu(appdata);
    step_timing(appdata, CurrentStep::UpdateGPU);

    appdata.disks.refresh(true);
    step_timing(appdata, CurrentStep::UpdateSystemDisk);

    refresh_system_memory(appdata);
    step_timing(appdata, CurrentStep::UpdateSystemMemory);

    refresh_networks(appdata);
    step_timing(appdata, CurrentStep::UpdateSystemNetwork);

    refresh_disk_io_time(appdata);
    step_timing(appdata, CurrentStep::UpdateIoTime);

    refresh_processes(appdata);
    step_timing(appdata, CurrentStep::UpdateSystemProcess);

    refresh_battery(appdata);
    step_timing(appdata, CurrentStep::UpdateBattery);
}

fn refresh_processes(appdata: &mut MyApp) {
    appdata
        .system_status
        .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
}

pub fn refresh_color(appdata: &mut MyApp, ui: &mut Ui) {
    let v = ui.visuals_mut();
    v.override_text_color = Some(Color32::from_gray(250));
    v.window_fill = get_windows_glass_color(
        appdata
            .settings
            .lock()
            .current_settings
            .use_plain_dark_background,
    );
}

pub struct MyNetworkData {
    tx: f64,
    rx: f64,
}

fn refresh_networks(appdata: &mut MyApp) {
    appdata.networks.refresh(true);
    for (name, data) in filter_networks(appdata) {
        let e = appdata
            .net_down_buffer
            .entry(name.clone())
            .or_insert(CircleVec::new());
        e.add(data.rx);
        let e = appdata
            .net_up_buffer
            .entry(name.clone())
            .or_insert(CircleVec::new());
        e.add(data.tx);
    }
}

fn refresh_system_memory(appdata: &mut MyApp) {
    appdata.system_status.refresh_memory();

    let cur_ram = appdata.system_status.used_memory() as f32;
    let tot_ram = appdata.system_status.total_memory() as f32;

    appdata.cur_ram = cur_ram;
    if appdata.total_ram == 0.0 {
        appdata.total_ram = tot_ram;
    }
    appdata.ram_buffer.add(cur_ram / appdata.total_ram);
}

fn refresh_cpu(appdata: &mut MyApp) {
    appdata
        .system_status
        .refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
    appdata
        .cpu_buffer
        .add(appdata.system_status.global_cpu_usage());

    let cpu_temp = &read_to_string("/sys/class/thermal/thermal_zone2/temp")
        .unwrap_or_default()
        .trim()
        .parse::<f32>()
        .unwrap_or_default()
        / 1000.0;

    let coretemps = appdata
        .system_status
        .cpus()
        .iter()
        .map(|c| (c.name().to_string(), cpu_temp))
        .collect_vec();

    let max_temp = coretemps
        .iter()
        .map(|(_, v)| v)
        .max_by(|x, y| x.abs().partial_cmp(&y.abs()).unwrap())
        .copied();

    appdata.coretemps = coretemps;

    appdata.cpu_maxtemp_buffer.add(max_temp.unwrap_or(0.0));

    // let cpu_power = ohw_opt.parse_value_path_def("#0|+images_icon/cpu.png|Power|Package");
    let current_power: u128 =
        read_to_string("/sys/devices/virtual/powercap/intel-rapl/subsystem/intel-rapl:0/energy_uj")
            .unwrap_or_default()
            .trim()
            .parse()
            .unwrap_or_default();

    let timediff = appdata.last_update_timestamp.elapsed().as_millis();
    if timediff > 100 {
        let pow = (current_power - appdata.last_joules) as f64 / 1_000_000.0;
        let uj_per_ms = pow / (timediff as f64 / 1000.0); // uj per ms -> j per s -> W

        let mut s = appdata.settings.lock();
        if uj_per_ms < 1000.0 {
            if uj_per_ms > s.current_settings.max_cpu_power {
                s.current_settings.max_cpu_power = uj_per_ms;
            }
            appdata.cpu_power_buffer.add(uj_per_ms);
        }
        drop(s);
        appdata.last_update_timestamp = Instant::now();
        appdata.last_joules = current_power;
    }
}

pub fn refresh_battery(appdata: &mut MyApp) {
    let level: f64 = appdata
        .ohw_info
        .lock()
        .parse_value_path_def("#0|+images_icon/battery.png|levels|charge");

    if level != 0.0 {
        appdata.battery_enabled = true;
        let ohw = appdata.ohw_info.lock();
        let mut charge =
            -ohw.parse_value_path_def::<f64>("#0|+images_icon/battery.png|currents|discharge");
        if charge == -0.0 {
            charge = ohw.parse_value_path_def("#0|+images_icon/battery.png|currents|charge");
        }
        drop(ohw);
        appdata.battery_change_buffer.add(charge);

        let now = Local::now().naive_local();
        if now > appdata.battery_level_next_update {
            appdata.battery_level_buffer.add(level);
            appdata.battery_level_next_update =
                now + chrono::Duration::seconds(60 - now.time().second() as i64);
        }
    }
}

pub fn run_nvidia_smi(shared_data: Arc<Mutex<GpuData>>) {
    loop {
        // Start nvidia-smi in continuous mode
        let mut child = match std::process::Command::new("nvidia-smi")
            .args(&[
                "--query-gpu=temperature.gpu,power.draw,memory.total,memory.used,memory.free,utilization.gpu,clocks.current.graphics,fan.speed,power.limit,clocks.max.graphics",
                // "--format=csv,noheader,nounits",
                "--format=csv,noheader",
                "-l",
                "1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                eprintln!("Failed to start nvidia-smi: {}", e);
                thread::sleep(Duration::from_secs(5));
                continue;
            }
        };

        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                eprintln!("Failed to capture stdout");
                thread::sleep(Duration::from_secs(5));
                continue;
            }
        };

        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                eprintln!("Failed to capture stderr");
                thread::sleep(Duration::from_secs(5));
                continue;
            }
        };

        // Use BufReader to read output line by line
        let reader = BufReader::new(stdout);

        // Clone shared data for error handling
        let shared_data_clone = Arc::clone(&shared_data);

        // Handle stderr in a separate thread to avoid blocking
        thread::spawn(move || {
            let err_reader = BufReader::new(stderr);
            for e in err_reader.lines() {
                eprintln!("nvidia-smi stderr: {:?}", e);
            }
        });

        // Read and parse each line asynchronously
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    // Parse the line to extract GPU data

                    // dbg!(&line);

                    let nvsmi_output = line.split(", ").collect_vec();

                    let temperature: f32 = nvsmi_output[0].parse().unwrap();
                    let power_usage: f32 = nvsmi_output[1]
                        .split_ascii_whitespace()
                        .next()
                        .unwrap()
                        .parse()
                        .unwrap();
                    let memory_total: f32 = 1024.0
                        * 1024.0
                        * nvsmi_output[2]
                            .split_ascii_whitespace()
                            .next()
                            .unwrap()
                            .parse::<f32>()
                            .unwrap();
                    let memory_used: f32 = 1024.0
                        * 1024.0
                        * nvsmi_output[3]
                            .split_ascii_whitespace()
                            .next()
                            .unwrap()
                            .parse::<f32>()
                            .unwrap();
                    let memory_free: f32 = 1024.0
                        * 1024.0
                        * nvsmi_output[4]
                            .split_ascii_whitespace()
                            .next()
                            .unwrap()
                            .parse::<f32>()
                            .unwrap();
                    let utilization: f32 = nvsmi_output[5]
                        .split_ascii_whitespace()
                        .next()
                        .unwrap()
                        .parse()
                        .unwrap();
                    let clock_mhz: f32 = nvsmi_output[6]
                        .split_ascii_whitespace()
                        .next()
                        .unwrap()
                        .parse()
                        .unwrap();
                    let fan_percentage: f32 = nvsmi_output[7]
                        .split_ascii_whitespace()
                        .next()
                        .unwrap()
                        .parse()
                        .unwrap();
                    let power_limit: f32 = nvsmi_output[8]
                        .split_ascii_whitespace()
                        .next()
                        .unwrap()
                        .parse()
                        .unwrap();
                    let max_clock: f32 = nvsmi_output[9]
                        .split_ascii_whitespace()
                        .next()
                        .unwrap()
                        .parse()
                        .unwrap();

                    let g = GpuData {
                        utilization,
                        temperature,
                        memory_free,
                        memory_used,
                        memory_total,
                        power_usage,
                        power_limit,
                        fan_percentage,
                        clock_mhz,
                        max_clock,
                    };

                    let mut data = shared_data_clone.lock().unwrap();
                    *data = g;
                }
                Err(e) => {
                    eprintln!("Error reading nvidia-smi output: {}", e);
                    break; // Exit the loop to restart the command
                }
            }
        }

        // If the loop exits, attempt to kill the child process
        match child.kill() {
            Ok(_) => eprintln!("Killed nvidia-smi process."),
            Err(e) => eprintln!("Failed to kill nvidia-smi: {}", e),
        }

        // Wait for the child process to exit
        match child.wait() {
            Ok(status) => eprintln!("nvidia-smi exited with status: {}", status),
            Err(e) => eprintln!("Failed to wait on nvidia-smi: {}", e),
        }

        // Sleep before restarting
        thread::sleep(Duration::from_secs(5));
    }
}
