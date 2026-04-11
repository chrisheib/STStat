use std::{
    collections::HashMap,
    fmt,
    fs::read_to_string,
    io::{BufRead, BufReader},
    process::Stdio,
    sync::{Arc, Mutex},
    thread::{self},
    time::{Duration, Instant},
};

use crate::{
    bytes_format::format_bytes,
    color::{auto_color_dark, get_base_background},
    components::edgy_progress::EdgyProgressBar,
    disk::refresh_disks,
    network::refresh_networks,
    step_timing,
    tasks::{schedule_tasks_refresh, tasks_browser_url, tasks_status_line},
    CurrentStep, MyApp,
};
use eframe::{
    egui::{
        vec2,
        // plot::{Line, Plot, PlotPoints},
        CursorIcon,
        Grid,
        Label,
        Layout,
        RichText,
        Sense,
        Separator,
        Ui,
        UiBuilder,
    },
    emath::Align::Max,
    epaint::Color32,
};
use egui_extras::{Column, TableBuilder};
use egui_plot::{GridInput, GridMark, Line, Plot, PlotPoints};
use itertools::Itertools;
use sysinfo::{CpuRefreshKind, Pid, ProcessRefreshKind};
use tokio::process::Command;

const SIDEBAR_SECTION_SIDE_MARGIN: f32 = 1.0;
const SIDEBAR_SECTION_GRID_SPACING: f32 = 2.0;
const SIDEBAR_SECTION_RIGHT_GUARD: f32 = 0.0;

/// Returns whether the sidebar layout debug overlay should be shown.
fn layout_debug_enabled(appdata: &MyApp) -> bool {
    appdata
        .settings
        .lock()
        .current_settings
        .layout_debug_overlay
}

#[derive(Clone, Copy)]
struct SectionLayout {
    width: f32,
    half_width: f32,
}

/// Renders a sidebar block with consistent 2 px side margins and centered section title.
fn show_sidebar_block(
    ui: &mut Ui,
    title: &str,
    body: impl FnOnce(&mut Ui, SectionLayout),
) -> eframe::egui::Response {
    ui.scope(|ui| {
        let outer_width = ui.available_width().max(1.0);
        ui.allocate_ui(vec2(outer_width, 0.0), |ui| {
            let inner_rect = ui
                .max_rect()
                .shrink2(vec2(SIDEBAR_SECTION_SIDE_MARGIN, 0.0));
            ui.scope_builder(UiBuilder::new().max_rect(inner_rect), |ui| {
                let inner_width = (ui.available_width() - SIDEBAR_SECTION_RIGHT_GUARD).max(1.0);
                let layout = SectionLayout {
                    width: inner_width,
                    half_width: ((inner_width - SIDEBAR_SECTION_GRID_SPACING) / 2.0).max(1.0),
                };
                ui.allocate_ui_with_layout(
                    vec2(inner_width, 0.0),
                    Layout::top_down_justified(eframe::emath::Align::Center),
                    |ui| {
                        ui.label(title);
                    },
                );

                ui.allocate_ui(vec2(inner_width, 0.0), |ui| {
                    body(ui, layout);
                    ui.add(Separator::default().spacing(1.0));
                });
            });
        });
    })
    .response
}

pub fn set_system_info_components(appdata: &mut MyApp, ui: &mut Ui) {
    step_timing(appdata, crate::CurrentStep::Begin);

    if layout_debug_enabled(appdata) {
        let outer_width = ui.available_width().max(1.0);
        let section_inner =
            (outer_width - SIDEBAR_SECTION_SIDE_MARGIN * 2.0 - SIDEBAR_SECTION_RIGHT_GUARD)
                .max(1.0);
        ui.label(
            RichText::new(format!(
                "DBG outer:{outer_width:.1} section:{section_inner:.1} guard:{:.1}",
                SIDEBAR_SECTION_RIGHT_GUARD
            ))
            .small()
            .weak(),
        );
        ui.separator();
    }

    show_cpu(appdata, ui);
    show_gpu(appdata, ui);
    show_drives(appdata, ui);
    show_network(appdata, ui);
    show_ping(appdata, ui);
    show_tasks(appdata, ui);
    show_processes(appdata, ui);
    show_battery(appdata, ui);
}

fn show_network(appdata: &mut MyApp, ui: &mut Ui) {
    show_sidebar_block(ui, "Networks", |ui, layout| {
        for net in &appdata.networks {
            ui.push_id(format!("network graph {}", net.interface), |ui| {
                let table_col_width = layout.width * 0.5;
                let table = TableBuilder::new(ui)
                    .striped(true)
                    .columns(Column::exact(table_col_width), 2);
                table.header(10.0, |mut header| {
                    header.col(|ui| {
                        ui.add(
                            Label::new(
                                RichText::new(format!(
                                    "⬆ {}",
                                    format_bytes(*net.history_up.read().last().unwrap())
                                ))
                                .size(12.0),
                            )
                            .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                        );
                    });
                    header.col(|ui| {
                        ui.add(
                            Label::new(
                                RichText::new(format!(
                                    "⬇ {}",
                                    format_bytes(*net.history_down.read().last().unwrap())
                                ))
                                .size(12.0),
                            )
                            .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                        );
                    });
                });
            });

            ui.add_space(3.0);

            let up = net.history_up.read();

            let up_line = Line::new(
                (0..net.history_up.capacity())
                    .map(|i| [i as f64, { up[i] }])
                    .collect::<PlotPoints>(),
            );

            let down = net.history_down.read();

            let down_line = Line::new(
                (0..net.history_down.capacity())
                    .map(|i| [i as f64, { down[i] }])
                    .collect::<PlotPoints>(),
            );

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

            add_graph(
                &format!("network-{}", net.interface),
                ui,
                vec![down_line, up_line],
                &[14.0 * 1024.0 * 1024.0, max_down, max_up],
                layout.width,
            );
        }
    });
    step_timing(appdata, crate::CurrentStep::Network);
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
    show_sidebar_block(ui, "Processes", |ui, layout| {
        let p = appdata
            .system_status
            .processes()
            .iter()
            .filter(|(_, p)| p.thread_kind().is_none())
            .map(|(_, a)| Process {
                cpu: a.cpu_usage(),
                memory: a.memory(),
                name: a.name().to_str().unwrap_or_default().to_string(),
                pid: a.pid(),
                parent: a.parent(),
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
            layout.width,
        );
        step_timing(appdata, crate::CurrentStep::ProcCPU);
        ui.add_space(4.0);

        // By Memory
        let mut mem_p = p.clone();
        clean_memory_process_list(&mut mem_p);
        add_process_table(
            ui,
            5,
            &mem_p,
            "Proc Ram",
            ProcessTableDisplayMode::Ram,
            cpu_count,
            layout.width,
        );

        step_timing(appdata, crate::CurrentStep::ProcRAM);
    });
}

fn clean_memory_process_list(proc: &mut [Process]) {
    proc.sort_unstable_by_key(|a| a.pid);
    // let mut i = proc.len() - 1;
    // while i > 0 {
    //     if let Some(ppid) = proc[i].parent {
    //         if let Some(parent) = proc.iter().find(|p| p.pid == ppid) {
    //             if parent.memory == proc[i].memory {
    //                 // If the parent has the same memory usage, remove the child
    //                 println!(
    //                     "Removing {} ({}), parent: {}",
    //                     proc[i].name, proc[i].pid, parent.name
    //                 );
    //                 proc.remove(i);
    //             }
    //         }
    //     }
    //     i -= 1;
    // }
    // panic!();
    proc.sort_unstable_by(|a, b| b.memory.cmp(&a.memory));
}

fn show_ping(appdata: &mut MyApp, ui: &mut Ui) {
    show_sidebar_block(ui, "Ping", |ui, layout| {
        let plot_width = (layout.width - 1.0).max(1.0);
        let pings = appdata.ping_buffer.read();
        let last_ping = pings.last().copied().unwrap_or_default();
        let max_ping = pings.iter().max().copied().unwrap_or_default();
        let line = Line::new(
            (0..appdata.ping_buffer.capacity())
                .map(|i| [i as f64, { pings[i] as f64 }])
                .collect::<PlotPoints>(),
        );

        let lp_str = if last_ping == 0 {
            "ERR".to_string()
        } else {
            format!("{last_ping:.0} ms")
        };

        ui.label(RichText::new(format!("M: {max_ping:.0}ms, C: {lp_str}")).size(12.0));
        add_graph("ping", ui, vec![line], &[50.0, max_ping as f64], plot_width);
        step_timing(appdata, crate::CurrentStep::Ping);
    });
}

fn show_battery(appdata: &mut MyApp, ui: &mut Ui) {
    if !appdata.battery_enabled {
        return;
    }
    show_sidebar_block(ui, "Battery", |_ui, _layout| {
        // let level = appdata.battery_level_buffer.read();
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
    });
}

fn show_cpu(appdata: &mut MyApp, ui: &mut Ui) {
    show_sidebar_block(ui, "CPU", |ui, layout| {
        step_timing(appdata, crate::CurrentStep::CpuCrunch);
        ui.spacing_mut().interact_size = [15.0, 12.0].into();

        let cpu = appdata.cpu_buffer.read();
        let last_cpu = cpu.last().copied().unwrap_or_default();

        let maxtemp = appdata.cpu_maxtemp_buffer.read();
        let last_maxtemp = maxtemp.last().copied().unwrap_or_default();
        let half_width = layout.half_width;
        let full_width = layout.width;

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
            layout.width,
        );
        step_timing(appdata, crate::CurrentStep::CPUGraph);
    });
}

/// Renders task items from Google Tasks to provide a compact, actionable queue.
fn show_tasks(appdata: &mut MyApp, ui: &mut Ui) {
    let settings = appdata.settings.lock().current_settings.clone();
    if !settings.tasks_enabled {
        return;
    }

    let tasks_page_url = tasks_browser_url(&settings);
    let block_response = show_sidebar_block(ui, "Tasks", |ui, layout| {
        let content_width = (layout.width - 1.0).max(1.0);
        ui.set_min_width(content_width);
        ui.set_max_width(content_width);

        let tasks = appdata.tasks_state.lock().items.clone();
        if tasks.is_empty() {
            ui.label(RichText::new("No upcoming tasks").small());
            ui.with_layout(Layout::right_to_left(eframe::emath::Align::Center), |ui| {
                ui.label(
                    RichText::new(tasks_status_line(&appdata.tasks_state))
                        .small()
                        .weak(),
                );
            });
            return;
        }

        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.spacing_mut().interact_size.y = 10.0;
            Grid::new("tasks_grid")
                .num_columns(1)
                .spacing([0.0, 0.0])
                .striped(true)
                .show(ui, |ui| {
                    for (shown, task) in tasks.into_iter().enumerate() {
                        if shown >= settings.tasks_max_items {
                            break;
                        }

                        let label = if let Some(due) = task.due {
                            format!("{}  {}", due.format("%d.%m.%y"), task.title)
                        } else {
                            format!("--.--.--  {}", task.title)
                        };

                        ui.add(
                            Label::new(RichText::new(label).small().strong())
                                .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                        );
                        ui.end_row();
                    }
                });
            ui.add_space(1.0);
        });

        ui.with_layout(Layout::right_to_left(eframe::emath::Align::Center), |ui| {
            ui.label(
                RichText::new(tasks_status_line(&appdata.tasks_state))
                    .small()
                    .weak(),
            );
        });
    });

    let response = ui
        .interact(
            block_response.rect,
            ui.id().with("tasks_block_link"),
            Sense::click(),
        )
        .on_hover_cursor(CursorIcon::PointingHand);
    if response.clicked() {
        let _ = webbrowser::open(tasks_page_url);
    }
}

fn show_gpu(appdata: &MyApp, ui: &mut Ui) {
    // if appdata.nvid_info.is_some() {
    if let Some(gpu) = &appdata.gpu {
        let l = gpu.lock().unwrap();
        let gpu = l.clone();
        drop(l);
        show_sidebar_block(ui, "GPU", |ui, layout| {
            let half_width = layout.half_width;
            let full_width = layout.width;

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
                layout.width,
            );
        });
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
    table_width: f32,
) {
    let mut clicked = false;
    ui.push_id(name, |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        let mut table = TableBuilder::new(ui).striped(true).column(Column::exact(
            table_width
                * if display_mode == ProcessTableDisplayMode::All {
                    0.4
                } else {
                    0.7
                },
        ));
        if display_mode == ProcessTableDisplayMode::All
            || display_mode == ProcessTableDisplayMode::Ram
        {
            table = table.column(Column::exact(table_width * 0.3))
        };
        if display_mode == ProcessTableDisplayMode::All
            || display_mode == ProcessTableDisplayMode::Cpu
        {
            table = table.column(Column::exact(table_width * 0.3))
        };
        let table = table.header(10.0, |mut header| {
            header.col(|ui| {
                clicked = clicked
                    || ui
                        .add(
                            Label::new(RichText::new(name).small())
                                .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                        )
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
                                .add(
                                    Label::new(RichText::new("RAM").small())
                                        .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                                )
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
                                .add(
                                    Label::new(RichText::new("CPU").small())
                                        .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                                )
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
                                    Label::new(RichText::new(p.name.to_string()).small().strong())
                                        .wrap_mode(eframe::egui::TextWrapMode::Truncate),
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
                                            .wrap_mode(eframe::egui::TextWrapMode::Truncate),
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
                                                    p.cpu / core_count as f32
                                                ))
                                                .small()
                                                .strong(),
                                            )
                                            .wrap_mode(eframe::egui::TextWrapMode::Truncate),
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
    ui.add_space(1.0);

    // if clicked {
    //     match Command::new("powershell")
    //         .args(["start", "taskmgr", "-v runAs"])
    //         .spawn()
    //     {
    //         Ok(_c) => println!("Starting Task Manager"),
    //         Err(e) => println!("{e}"),
    //     };
    // }
}

fn add_graph(id: &str, ui: &mut Ui, line: Vec<Line>, max_y: &[f64], width: f32) {
    let desired_size = vec2(width.max(1.0), 30.0);

    let mut p = Plot::new(id)
        .show_axes([false, false])
        .x_grid_spacer(|input| proportional_grid_marks(input, 10))
        .y_grid_spacer(|input| proportional_grid_marks(input, 5))
        .grid_spacing(eframe::egui::Rangef::new(2.0, 300.0))
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

    p.show(ui, |plot_ui: &mut egui_plot::PlotUi| {
        for l in line {
            plot_ui.line(l)
        }
    });
}

/// Generates evenly spaced grid marks over the visible range.
/// Skips marks at 0 and at max bound to avoid edge artifacts.
fn proportional_grid_marks(input: GridInput, divisions: usize) -> Vec<GridMark> {
    if divisions == 0 {
        return Vec::new();
    }

    let (min, max) = input.bounds;
    let range = max - min;
    if !range.is_finite() || range <= f64::EPSILON {
        return Vec::new();
    }

    let step = range / divisions as f64;
    let eps = step * 0.001;
    let mut marks = Vec::with_capacity(divisions.saturating_sub(1));

    for i in 1..divisions {
        let value = min + step * i as f64;
        if (value - 0.0).abs() <= eps || (value - max).abs() <= eps {
            continue;
        }
        marks.push(GridMark {
            value,
            step_size: step,
        });
    }

    marks
}

/// Left-ellipsizes a drive label based on measured text width.
struct DriveNameCutoutDebug {
    full_text: String,
    cutout_text: String,
    full_width: f32,
    max_width: f32,
    cutout_width: f32,
    ellipsis_width: f32,
    total_chars: usize,
    kept_chars: usize,
}

fn left_ellipsize_drive_name(
    ui: &Ui,
    name: &str,
    max_width: f32,
) -> (String, DriveNameCutoutDebug) {
    let ellipsis = "...";
    let measure_width = |text: &str| {
        eframe::egui::WidgetText::from(RichText::new(text).small().strong())
            .into_galley(
                ui,
                Some(eframe::egui::TextWrapMode::Extend),
                f32::INFINITY,
                eframe::egui::TextStyle::Small,
            )
            .size()
            .x
    };
    let max_width = (max_width - 1.0).max(0.0);
    let full_width = measure_width(name);
    let ellipsis_width = measure_width(ellipsis);
    let total_chars = name.chars().count();

    let mut dbg = DriveNameCutoutDebug {
        full_text: name.to_string(),
        cutout_text: String::new(),
        full_width,
        max_width,
        cutout_width: 0.0,
        ellipsis_width,
        total_chars,
        kept_chars: 0,
    };

    if max_width <= 0.0 {
        return (String::new(), dbg);
    }
    if full_width <= max_width {
        dbg.cutout_text = name.to_string();
        dbg.cutout_width = full_width;
        dbg.kept_chars = total_chars;
        return (name.to_string(), dbg);
    }

    if ellipsis_width > max_width {
        return (String::new(), dbg);
    }

    let chars: Vec<char> = name.chars().collect();
    let total = chars.len();
    for keep in (1..=total).rev() {
        let suffix: String = chars[total - keep..].iter().collect();
        let candidate = format!("{ellipsis}{suffix}");
        let candidate_width = measure_width(&candidate);
        if candidate_width <= max_width {
            dbg.cutout_text = candidate.clone();
            dbg.cutout_width = candidate_width;
            dbg.kept_chars = keep;
            return (candidate, dbg);
        }
    }

    dbg.cutout_text = ellipsis.to_string();
    dbg.cutout_width = ellipsis_width;
    (ellipsis.to_string(), dbg)
}

fn show_drives(appdata: &MyApp, ui: &mut Ui) {
    let debug = layout_debug_enabled(appdata);
    show_sidebar_block(ui, "Drives", |ui, layout| {
        let table_width = layout.width.max(1.0);
        let col_name = table_width * 0.45;
        let col_usage = table_width * 0.2;
        let col_free = table_width * 0.35;
        let row_height = 11.0;

        if debug {
            ui.label(
                RichText::new(format!(
                    "DBG d col n:{col_name:.1} u:{col_usage:.1} f:{col_free:.1}"
                ))
                .small()
                .weak(),
            );
        }

        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let mut first_drive_debug: Option<DriveNameCutoutDebug> = None;

            let table = TableBuilder::new(ui)
                .striped(true)
                .column(Column::exact(col_name))
                .column(Column::exact(col_usage))
                .column(Column::exact(col_free));

            table.body(|body| {
                body.rows(row_height, appdata.disks.len(), |mut row| {
                    let row_index = row.index();
                    if let Some(d) = appdata.disks.get(row_index) {
                        let (blkindex, blk) = appdata
                            .blockdevices
                            .iter()
                            .enumerate()
                            .find(|(_, blk)| blk.name == d.blockdevicename)
                            .unwrap();

                        let history = blk.io_history.read();
                        let usage = history.last().copied().unwrap_or_default();

                        row.col(|ui| {
                            ui.add_space(1.0);
                            let (name_text, name_dbg) = left_ellipsize_drive_name(
                                ui,
                                d.displayname.as_str(),
                                ui.available_width().max(1.0),
                            );
                            if debug && row_index == 0 {
                                first_drive_debug = Some(name_dbg);
                            }
                            ui.add(
                                Label::new(RichText::new(name_text).small().strong())
                                    .wrap_mode(eframe::egui::TextWrapMode::Extend),
                            );
                        });

                        row.col(|ui| {
                            let mut label_rect = ui.max_rect();
                            label_rect.max.x -= 2.0;
                            ui.scope_builder(UiBuilder::new().max_rect(label_rect), |ui| {
                                ui.with_layout(Layout::top_down_justified(Max), |ui| {
                                    ui.add_space(1.0);
                                    ui.add(
                                        Label::new(
                                            RichText::new(format!("{usage:.0}%")).small().strong(),
                                        )
                                        .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                                    );
                                });
                            });
                        });

                        row.col(|ui| {
                            let free_cell_width = ui.available_width().max(1.0);
                            ui.add(
                                EdgyProgressBar::new(d.bytes_used as f32 / d.bytes_total as f32)
                                    .desired_width(free_cell_width)
                                    .desired_height(12.0)
                                    .text(
                                        RichText::new(format_bytes(d.bytes_free as f64))
                                            .small()
                                            .strong()
                                            .color(Color32::from_white_alpha(80)),
                                    )
                                    .text_align_right(true)
                                    .fill(auto_color_dark(blkindex as i32)),
                            );
                        });
                    }
                });
            });

            if debug {
                if let Some(info) = first_drive_debug {
                    ui.label(
                        RichText::new(format!(
                            "DBG d0 full:'{}' cut:'{}' w_full:{:.1} w_max:{:.1} w_cut:{:.1} w_dots:{:.1} chars:{} keep:{}",
                            info.full_text,
                            info.cutout_text,
                            info.full_width,
                            info.max_width,
                            info.cutout_width,
                            info.ellipsis_width,
                            info.total_chars,
                            info.kept_chars,
                        ))
                        .small()
                        .weak(),
                    );
                }
            }
        });
        ui.add_space(1.0);
        ui.spacing();

        let mut lines = Vec::new();
        for d in &appdata.blockdevices {
            let values = d.io_history.read();
            lines.push(Line::new(
                (0..d.io_history.capacity())
                    .map(|i| [i as f64, { values[i] as f64 }])
                    .collect::<PlotPoints>(),
            ));
        }

        add_graph("disk", ui, lines, &[100.5], layout.width);
    });
}

// pub fn init_system(appdata: &mut MyApp) {
//     init_disks(appdata);
// }

pub fn get_windows_glass_color(use_plain_blackground: bool) -> Color32 {
    if use_plain_blackground {
        return get_base_background();
    }
    let col: u32 = 0;
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

pub fn refresh(appdata: &mut MyApp) {
    // refresh windows perfcount stats once
    // unsafe { PdhCollectQueryData(appdata.windows_performance_query_handle) };

    refresh_cpu(appdata);
    step_timing(appdata, CurrentStep::UpdateCPU);

    refresh_gpu(appdata);
    step_timing(appdata, CurrentStep::UpdateGPU);

    // appdata.disks.refresh(true);
    // step_timing(appdata, CurrentStep::UpdateSystemDisk);

    refresh_system_memory(appdata);
    step_timing(appdata, CurrentStep::UpdateSystemMemory);

    refresh_networks(appdata);
    step_timing(appdata, CurrentStep::UpdateSystemNetwork);

    // refresh_disk_io_time(appdata);
    // step_timing(appdata, CurrentStep::UpdateIoTime);

    refresh_processes(appdata);
    step_timing(appdata, CurrentStep::UpdateSystemProcess);

    refresh_battery(appdata);
    step_timing(appdata, CurrentStep::UpdateBattery);

    refresh_disks(appdata);
    schedule_tasks_refresh(appdata);
}

fn refresh_processes(appdata: &mut MyApp) {
    // appdata
    //     .system_status
    //     .refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    appdata.system_status.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cpu().with_memory(),
    );

    // for (pid, p) in appdata.system_status.processes() {
    //     println!(
    //         "Process: {:?} (PID: {}, Parent: {:?} CPU: {:.1}%, Mem: {} bytes, Threadkind {:?})",
    //         p.name(),
    //         p.pid(),
    //         p.parent(),
    //         p.cpu_usage(),
    //         p.memory(),
    //         p.thread_kind()
    //     );
    // }
    // panic!();
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

    let mut cpu_temp = &read_to_string("/sys/class/thermal/thermal_zone2/temp")
        .unwrap_or_default()
        .trim()
        .parse::<f32>()
        .unwrap_or_default()
        / 1000.0;

    if cpu_temp == 0.0 {
        // AMD
        if let Some(temp) = appdata.cpu_temp_thread.lock().unwrap().as_ref() {
            cpu_temp = *temp;
        }
    }

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
        let pow = (current_power.overflowing_sub(appdata.last_joules)).0 as f64 / 1_000_000.0;
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

pub fn refresh_battery(_appdata: &mut MyApp) {
    // TODO
    // let level: f64 = appdata
    //     .ohw_info
    //     .lock()
    //     .parse_value_path_def("#0|+images_icon/battery.png|levels|charge");

    // if level != 0.0 {
    //     appdata.battery_enabled = true;
    //     let ohw = appdata.ohw_info.lock();
    //     let mut charge =
    //         -ohw.parse_value_path_def::<f64>("#0|+images_icon/battery.png|currents|discharge");
    //     if charge == -0.0 {
    //         charge = ohw.parse_value_path_def("#0|+images_icon/battery.png|currents|charge");
    //     }
    //     drop(ohw);
    //     appdata.battery_change_buffer.add(charge);

    //     let now = Local::now().naive_local();
    //     if now > appdata.battery_level_next_update {
    //         appdata.battery_level_buffer.add(level);
    //         appdata.battery_level_next_update =
    //             now + chrono::Duration::seconds(60 - now.time().second() as i64);
    //     }
    // }
}

pub async fn loop_amd_temp_sensor(shared_data: Arc<Mutex<Option<f32>>>) {
    if read_to_string("/sys/class/thermal/thermal_zone2/temp").is_ok() {
        return;
    }

    loop {
        // sensors k10temp-pci-00c3 -j
        if let Ok(output) = Command::new("sensors").arg("-j").output().await {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
                if let Some(temp) = json["k10temp-pci-00c3"]["Tctl"]["temp1_input"].as_f64() {
                    let mut data = shared_data.lock().unwrap();
                    *data = Some(temp as f32);
                }
            } else {
                eprintln!("Failed to execute command");
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
}

// pub async fn loop_iostat_disk_util() {
//     loop {
//         // sensors k10temp-pci-00c3 -j
//         if let Ok(output) = Command::new("iostat")
//             .arg("-x")
//             .arg("-o")
//             .arg("JSON")
//             .output()
//             .await
//         {
//             if dbg!(&output).status.success() {
//                 let stdout = String::from_utf8_lossy(&output.stdout);
//                 let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
//                 if let Some(temp) = json["sysstat"]["hosts"][0]["statistics"][0]["disk"].as_array()
//                 {
//                     for t in temp {
//                         let name = t["disk_device"].as_str().unwrap_or_default();
//                         let util = t["util"].as_f64().unwrap_or_default();
//                         println!("{name}: {util}");
//                     }
//                     // let mut data = shared_data.lock().unwrap();
//                     // *data = Some(temp as f32);
//                 }
//             } else {
//                 eprintln!("Failed to execute command 2");
//             }
//         } else {
//             eprintln!("Failed to execute command 1");
//         }
//         thread::sleep(Duration::from_secs(1));
//     }
// }

pub fn loop_iostat_disk_util(shared_data: Arc<Mutex<HashMap<String, f32>>>) {
    loop {
        // Start nvidia-smi in continuous mode
        let mut child = match std::process::Command::new("iostat")
            .args(&["-x", "-d", "--compact", "1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                eprintln!("Failed to start iostat: {}", e);
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
        // let shared_data_clone = Arc::clone(&shared_data);

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
                    // Parse the line to extract device data
                    // dbg!(&line);
                    if line.is_empty() {
                        continue;
                    }
                    if line.starts_with("Device") {
                        continue;
                    }
                    if line.starts_with("Linux") {
                        continue;
                    }
                    let mut s = line.trim().split_whitespace();
                    let name = s.next().unwrap();
                    let util = s.last().unwrap().replace(",", ".");
                    let util: f32 = util.parse().unwrap();

                    let mut data = shared_data.lock().unwrap();
                    data.insert(name.to_string(), util);
                    drop(data);
                }
                Err(e) => {
                    eprintln!("Error reading iostat output: {}", e);
                    break; // Exit the loop to restart the command
                }
            }
        }

        // If the loop exits, attempt to kill the child process
        match child.kill() {
            Ok(_) => eprintln!("Killed iostat process."),
            Err(e) => eprintln!("Failed to kill iostat: {}", e),
        }

        // Wait for the child process to exit
        match child.wait() {
            Ok(status) => eprintln!("iostat exited with status: {}", status),
            Err(e) => eprintln!("Failed to wait on iostat: {}", e),
        }

        // Sleep before restarting
        thread::sleep(Duration::from_secs(5));
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
