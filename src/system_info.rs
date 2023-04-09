use crate::{
    bytes_format::format_bytes, circlevec::CircleVec, color::auto_color,
    components::edgy_progress::EdgyProgressBar, sidebar::STATIC_HWND, step_timing, CurrentStep,
    MyApp, SIZE,
};
use eframe::{
    egui::{
        plot::{Line, Plot, PlotPoints},
        Grid, Label, Layout, RichText, Sense, Ui,
    },
    emath::Align::{self, Max},
    epaint::{Color32, Vec2},
};
use egui_extras::{Column, TableBuilder};
use itertools::Itertools;
use serde::Deserialize;
use std::ops::Add;
use sysinfo::{CpuExt, DiskExt, NetworkExt, NetworksExt, ProcessExt, SystemExt};
use tokio::process::Command;
use windows::{
    core::{decode_utf8_char, PCWSTR, PWSTR},
    w,
    Win32::{
        Foundation::BOOL,
        Graphics::Dwm::DwmGetColorizationColor,
        System::Performance::{
            PdhAddCounterW, PdhBrowseCountersW, PdhCollectQueryData, PdhGetFormattedCounterValue,
            PdhOpenQueryW, PDH_BROWSE_DLG_CONFIG_W, PDH_CSTATUS_VALID_DATA, PDH_FMT_DOUBLE,
            PERF_DETAIL_ADVANCED,
        },
    },
};

#[derive(Default, Debug)]
struct Proc {
    name: String,
    cpu: f32,
    memory: u64,
    disk_read: u64,
    disk_write: u64,
    count: u64,
}

impl Add for Proc {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            name: self.name,
            cpu: self.cpu + rhs.cpu,
            memory: self.memory + rhs.memory,
            disk_read: self.disk_read + rhs.disk_read,
            disk_write: self.disk_write + rhs.disk_write,
            count: self.count + rhs.count,
        }
    }
}

pub fn set_system_info_components(appdata: &mut MyApp, ui: &mut Ui) {
    step_timing(appdata, crate::CurrentStep::Begin);

    show_cpu(appdata, ui);
    show_gpu(appdata, ui);
    show_drives(appdata, ui);
    show_network(appdata, ui);
    show_ping(appdata, ui);
    show_processes(appdata, ui);
}

fn show_network(appdata: &mut MyApp, ui: &mut Ui) {
    ui.vertical_centered(|ui| ui.label("Net"));

    for (interface_name, data) in filter_networks(appdata) {
        // clock_video: u32,

        ui.push_id(format!("network graph {interface_name}"), |ui| {
            let table = TableBuilder::new(ui)
                .striped(true)
                .columns(Column::exact((SIZE.x - 10.0) * 0.4), 2);
            table.header(10.0, |mut header| {
                header.col(|ui| {
                    ui.add(
                        Label::new(RichText::new(format!("⬆ {}", format_bytes(data.tx))))
                            .wrap(false),
                    );
                });
                header.col(|ui| {
                    ui.add(
                        Label::new(RichText::new(format!("⬇ {}", format_bytes(data.rx))))
                            .wrap(false),
                    );
                });
            });
        });

        let up_buffer = appdata
            .net_up_buffer
            .entry(interface_name.clone())
            .or_insert(CircleVec::new());
        let up = up_buffer.read();

        let up_line = Line::new(
            (0..up_buffer.capacity())
                .map(|i| [i as f64, { up[i] }])
                .collect::<PlotPoints>(),
        );

        let down_buffer = appdata
            .net_down_buffer
            .entry(interface_name.clone())
            .or_insert(CircleVec::new());
        let down = down_buffer.read();

        let down_line = Line::new(
            (0..down_buffer.capacity())
                .map(|i| [i as f64, { down[i] }])
                .collect::<PlotPoints>(),
        );

        ui.add_space(3.0);

        add_graph(
            "network",
            ui,
            vec![down_line, up_line],
            14.0 * 1024.0 * 1024.0,
        );
    }
    ui.separator();
    step_timing(appdata, crate::CurrentStep::Network);
}

fn filter_networks(appdata: &mut MyApp) -> Vec<(String, MyNetworkData)> {
    appdata
        .system_status
        .networks()
        .iter()
        .filter(|i| {
            appdata
                .settings
                .lock()
                .current_settings
                .networks
                .entry(i.0.to_string())
                .or_default()
                .clone()
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
    utilization: f64,
    temperature: f32,
    memory_free: f32,
    memory_used: f32,
    memory_total: f32,
    power_usage: f32,
    power_limit: f32,
    fan_percentage: f32,
    clock_mhz: f32,
}

fn timing_to_str(timestamp: std::time::Instant, text: &mut String, perf_trace: bool) {
    if perf_trace {
        *text += &format!("{}\n", timestamp.elapsed().as_micros());
    }
}

pub fn refresh_gpu(appdata: &mut MyApp) {
    step_timing(appdata, CurrentStep::UpdateGPU);
    let perf_trace = appdata.settings.lock().current_settings.track_timings;

    let mut text = String::new();
    timing_to_str(appdata.current_frame_start, &mut text, perf_trace); // , 96

    let mut utilization = 0.0;
    let mut temperature = 0.0;
    let mut fan_percentage = 0.0;
    let mut power_usage = 0.0;
    let mut memory_free = 0.0;
    let mut memory_used = 0.0;
    let mut memory_total = 0.0;
    let mut clock_mhz = 0.0;

    let power_limit;
    if let Some(gpu) = &appdata.gpu {
        power_limit = gpu.power_limit;
    } else {
        let gpu = appdata.nvid_info.device_by_index(0).unwrap();
        power_limit = gpu.enforced_power_limit().unwrap() as f32 / 1000.0;
    }
    timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

    let ohw = appdata.ohw_info.lock();
    if let Some(ohw) = ohw.as_ref() {
        let nodes = &ohw.Children[0]
            .Children
            .iter()
            .find(|n| n.ImageURL == "images_icon/nvidia.png")
            .unwrap()
            .Children;
        timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

        temperature = nodes
            .iter()
            .find(|n| n.Text == "Temperatures")
            .unwrap()
            .Children[0]
            .Value
            .split_whitespace()
            .next()
            .unwrap()
            .replace(',', ".")
            .parse::<f32>()
            .unwrap();
        timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

        power_usage = nodes.iter().find(|n| n.Text == "Powers").unwrap().Children[0]
            .Value
            .split_whitespace()
            .next()
            .unwrap()
            .replace(',', ".")
            .parse::<f32>()
            .unwrap();
        timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

        memory_free = nodes.iter().find(|n| n.Text == "Data").unwrap().Children[0]
            .Value
            .split_whitespace()
            .next()
            .unwrap()
            .replace(',', ".")
            .parse::<f32>()
            .unwrap()
            * 1024.0
            * 1024.0;
        timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

        memory_used = nodes.iter().find(|n| n.Text == "Data").unwrap().Children[1]
            .Value
            .split_whitespace()
            .next()
            .unwrap()
            .replace(',', ".")
            .parse::<f32>()
            .unwrap()
            * 1024.0
            * 1024.0;
        timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

        memory_total = nodes.iter().find(|n| n.Text == "Data").unwrap().Children[2]
            .Value
            .split_whitespace()
            .next()
            .unwrap()
            .replace(',', ".")
            .parse::<f32>()
            .unwrap()
            * 1024.0
            * 1024.0;
        timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

        fan_percentage = nodes
            .iter()
            .find(|n| n.Text == "Controls")
            .unwrap()
            .Children[0]
            .Value
            .split_whitespace()
            .next()
            .unwrap()
            .replace(',', ".")
            .parse::<f32>()
            .unwrap();
        timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

        utilization = nodes.iter().find(|n| n.Text == "Load").unwrap().Children[0]
            .Value
            .split_whitespace()
            .next()
            .unwrap()
            .replace(',', ".")
            .parse::<f64>()
            .unwrap();
        timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

        clock_mhz = nodes.iter().find(|n| n.Text == "Clocks").unwrap().Children[0]
            .Value
            .split_whitespace()
            .next()
            .unwrap()
            .replace(',', ".")
            .parse::<f32>()
            .unwrap();
        timing_to_str(appdata.current_frame_start, &mut text, perf_trace);
    };
    drop(ohw);

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
    };
    timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

    appdata.gpu_buffer.add(g.utilization);
    appdata
        .gpu_mem_buffer
        .add((g.memory_used / g.memory_total) as f64);
    appdata
        .gpu_pow_buffer
        .add((g.power_usage / g.power_limit) as f64);

    if perf_trace && appdata.framecount < 1000 {
        println!("{text}");
    }
    appdata.gpu = Some(g);
    step_timing(appdata, CurrentStep::UpdateGPU);
}

fn show_processes(appdata: &mut MyApp, ui: &mut Ui) {
    ui.vertical_centered(|ui| ui.label("Processes"));
    let num_cpus = appdata.system_status.cpus().len();
    // Processes
    let mut processes = appdata
        .system_status
        .processes()
        .values()
        .map(|p| Proc {
            name: p.name().to_string().replace(".exe", ""),
            cpu: p.cpu_usage(),
            memory: p.memory(),
            disk_read: p.disk_usage().read_bytes,
            disk_write: p.disk_usage().written_bytes,
            count: 1,
        })
        .sorted_by_key(|p| p.name.clone())
        .group_by(|p| p.name.clone())
        .into_iter()
        .map(|(_name, group)| group.reduce(|acc, v| acc + v).unwrap())
        .collect_vec();

    // By CPU
    processes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    add_process_table(
        ui,
        7,
        &processes,
        num_cpus,
        "Proc CPU",
        ProcessTableDisplayMode::Cpu,
    );
    step_timing(appdata, crate::CurrentStep::ProcCPU);

    // By Memory
    processes.sort_by(|a, b| b.memory.cmp(&a.memory));
    add_process_table(
        ui,
        7,
        &processes,
        num_cpus,
        "Proc Ram",
        ProcessTableDisplayMode::Ram,
    );
    step_timing(appdata, crate::CurrentStep::ProcRAM);
}

fn show_ping(appdata: &mut MyApp, ui: &mut Ui) {
    ui.vertical_centered(|ui| ui.label("Ping"));
    let pings = appdata.ping_buffer.read();
    let last_ping = pings.last().copied().unwrap_or_default();
    let max_ping = pings.iter().max().copied().unwrap_or_default();
    let line = Line::new(
        (0..appdata.ping_buffer.capacity())
            .map(|i| [i as f64, { pings[i] as f64 }])
            .collect::<PlotPoints>(),
    );

    ui.label(format!("M: {max_ping:.0}ms, C: {last_ping:.0} ms"));
    add_graph("ping", ui, vec![line], 50.0);
    step_timing(appdata, crate::CurrentStep::Ping);
    ui.separator();
}

fn show_cpu(appdata: &mut MyApp, ui: &mut Ui) {
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
                if n.Text.contains("CPU Core #") {
                    if let Ok(val) = n.Text.replace("CPU Core #", "").parse::<i32>() {
                        Some((val, n.to_owned()))
                    } else {
                        None
                    }
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
                        RichText::new(format!("CPU: {last_cpu:.1}%",))
                            .small()
                            .strong(),
                    )
                    .desired_width(SIZE.x / 2.0 - 5.0)
                    .colored_dot(Some(auto_color(0))),
            );
            ui.add(
                EdgyProgressBar::new(max_temp / 100.0)
                    .text(RichText::new(format!("{max_temp:.1} °C")).small().strong())
                    .desired_width(SIZE.x / 2.0 - 5.0)
                    .colored_dot(Some(auto_color(2))),
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
            .colored_dot(Some(auto_color(1))),
    );

    Grid::new("cpu_grid_cores")
        .num_columns(2)
        .spacing([2.0, 2.0])
        .striped(true)
        .show(ui, |ui| {
            for (i, cpu_chunk) in appdata.system_status.cpus().chunks(2).enumerate() {
                for cpu in cpu_chunk {
                    let temp = coretemps
                        .get(i)
                        .map(|o| o.1.Value.to_string())
                        .unwrap_or_default();
                    let usage = cpu.cpu_usage();
                    ui.add(
                        EdgyProgressBar::new(usage / 100.0)
                            .desired_width(SIZE.x / 2.0 - 5.0)
                            .text(
                                RichText::new(format!("{usage:.0}% {temp}"))
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

    let temp_line = Line::new(
        (0..appdata.cpu_maxtemp_buffer.capacity())
            .map(|i| [i as f64, { max_temp_line[i] as f64 }])
            .collect::<PlotPoints>(),
    );

    step_timing(appdata, crate::CurrentStep::CPU);
    add_graph("cpu", ui, vec![cpu_line, ram_line, temp_line], 100.5);
    step_timing(appdata, crate::CurrentStep::CPUGraph);

    ui.separator();
}

fn show_gpu(appdata: &MyApp, ui: &mut Ui) {
    ui.vertical_centered(|ui| ui.label("GPU"));
    ui.add(
        EdgyProgressBar::new(appdata.gpu.as_ref().unwrap().utilization as f32 / 100.0)
            .text(
                RichText::new(format!(
                    "GPU: {:.1}%",
                    appdata.gpu.as_ref().unwrap().utilization
                ))
                .small()
                .strong(),
            )
            .colored_dot(Some(auto_color(0))),
    );

    ui.add(
        EdgyProgressBar::new(
            appdata.gpu.as_ref().unwrap().memory_used / appdata.gpu.as_ref().unwrap().memory_total,
        )
        .text(
            RichText::new(format!(
                "Mem: {} / {}",
                format_bytes(appdata.gpu.as_ref().unwrap().memory_used as f64),
                format_bytes(appdata.gpu.as_ref().unwrap().memory_total as f64)
            ))
            .small()
            .strong(),
        )
        .colored_dot(Some(auto_color(1))),
    );

    ui.add(
        EdgyProgressBar::new(
            appdata.gpu.as_ref().unwrap().power_usage / appdata.gpu.as_ref().unwrap().power_limit,
        )
        .text(
            RichText::new(format!(
                "Pow: {:.0}W / {:.0}W",
                appdata.gpu.as_ref().unwrap().power_usage,
                appdata.gpu.as_ref().unwrap().power_limit
            ))
            .small()
            .strong(),
        )
        .colored_dot(Some(auto_color(2))),
    );

    ui.push_id("gpu table", |ui| {
        let table = TableBuilder::new(ui)
            .striped(true)
            .columns(Column::exact((SIZE.x * 0.5) - 9.5), 2);

        table.body(|mut body| {
            body.row(12.0, |mut row| {
                // Clock
                row.col(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{:.0} MHz",
                            appdata.gpu.as_ref().unwrap().clock_mhz
                        ))
                        .small()
                        .strong(),
                    );
                });
                // temp
                row.col(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{:.0} °C",
                                appdata.gpu.as_ref().unwrap().temperature
                            ))
                            .small()
                            .strong(),
                        );
                    });
                });
            });
        });
    });

    let gpu_buf = appdata.gpu_buffer.read();
    let gpu_line = Line::new(
        (0..appdata.gpu_buffer.capacity())
            .map(|i| [i as f64, { gpu_buf[i] }])
            .collect::<PlotPoints>(),
    );

    let mem_buf = appdata.gpu_mem_buffer.read();
    let mem_line = Line::new(
        (0..appdata.gpu_mem_buffer.capacity())
            .map(|i| [i as f64, { mem_buf[i] * 100.0 }])
            .collect::<PlotPoints>(),
    );

    let pow_buf = appdata.gpu_pow_buffer.read();
    let pow_line = Line::new(
        (0..appdata.gpu_pow_buffer.capacity())
            .map(|i| [i as f64, { pow_buf[i] * 100.0 }])
            .collect::<PlotPoints>(),
    );

    add_graph("gpu", ui, vec![gpu_line, mem_line, pow_line], 100.0);

    ui.separator();
}

#[derive(PartialEq, Eq)]
enum ProcessTableDisplayMode {
    All,
    Cpu,
    Ram,
}

fn add_process_table(
    ui: &mut Ui,
    len: usize,
    p: &[Proc],
    num_cpus: usize,
    name: &str,
    display_mode: ProcessTableDisplayMode,
) {
    let mut clicked = false;
    ui.push_id(name, |ui| {
        let mut table = TableBuilder::new(ui).striped(true).column(Column::exact(
            (SIZE.x - 10.0)
                * if display_mode == ProcessTableDisplayMode::All {
                    0.4
                } else {
                    0.63
                },
        ));
        if display_mode == ProcessTableDisplayMode::All
            || display_mode == ProcessTableDisplayMode::Ram
        {
            table = table.column(Column::exact((SIZE.x - 10.0) * 0.3))
        };
        if display_mode == ProcessTableDisplayMode::All
            || display_mode == ProcessTableDisplayMode::Cpu
        {
            table = table.column(Column::exact((SIZE.x - 10.0) * 0.3))
        };
        let table = table.header(10.0, |mut header| {
            header.col(|ui| {
                clicked = clicked
                    || ui
                        .add(Label::new(RichText::new(name).small()).wrap(false))
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
                                .add(Label::new(RichText::new("RAM").small()).wrap(false))
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
                                .add(Label::new(RichText::new("CPU").small()).wrap(false))
                                .interact(Sense::click())
                                .double_clicked();
                    });
                });
            }
        });
        table.body(|body| {
            body.rows(10.0, len, |row_index, mut row| {
                let p = &p[row_index];
                row.col(|ui| {
                    clicked = clicked
                        || ui
                            .add(
                                Label::new(
                                    RichText::new(format!(
                                        "{}{}",
                                        p.name,
                                        if p.count > 1 {
                                            format!(" ×{}", p.count)
                                        } else {
                                            "".to_string()
                                        }
                                    ))
                                    .small()
                                    .strong(),
                                )
                                .wrap(false),
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
                                        .wrap(false),
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
                                                p.cpu / num_cpus as f32
                                            ))
                                            .small()
                                            .strong(),
                                        )
                                        .wrap(false),
                                    )
                                    .interact(Sense::click())
                                    .double_clicked();
                        });
                    });
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

fn add_graph(id: &str, ui: &mut Ui, line: Vec<Line>, max_y: f64) {
    Plot::new(id)
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
        .width(SIZE.x - 7.0)
        .height(30.0)
        .include_y(0.0)
        .include_y(max_y)
        .set_margin_fraction(Vec2::ZERO)
        .show(ui, |plot_ui| {
            for l in line {
                plot_ui.line(l)
            }
        });
}

fn show_drives(appdata: &MyApp, ui: &mut Ui) {
    ui.vertical_centered(|ui| ui.label("Drives"));
    Grid::new("drive_grid")
        .spacing([2.0, 2.0])
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for (i, d) in appdata.system_status.disks().iter().enumerate() {
                ui.spacing_mut().interact_size = [15.0, 12.0].into();
                let mount = d.mount_point().to_str().unwrap().replace('\\', "");
                let (_, _, value) = appdata
                    .disk_time_value_handle_map
                    .iter()
                    .find(|(s, _, _)| s == &mount)
                    .unwrap();

                ui.add(Label::new(
                    RichText::new(format!("{mount} {value:.1}%"))
                        .small()
                        .strong(),
                ));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(
                        EdgyProgressBar::new(
                            (d.total_space() - d.available_space()) as f32 / d.total_space() as f32,
                        )
                        .desired_width(
                            appdata.settings.lock().current_settings.location.width as f32 * 0.5,
                        )
                        .text(
                            RichText::new(format!(
                                "Free: {}",
                                format_bytes(d.available_space() as f64),
                            ))
                            .small()
                            .strong(),
                        ),
                    );
                    ui.add(Label::new(
                        RichText::new("⏺").size(8.0).color(auto_color(i as i32)),
                    ));
                });
                ui.end_row();
            }
        });
    ui.spacing();

    let mut lines = Vec::new();
    for (_, diskbuffer) in &appdata.disk_buffer {
        let values = diskbuffer.read();
        lines.push(Line::new(
            (0..diskbuffer.capacity())
                .map(|i| [i as f64, { values[i] as f64 }])
                .collect::<PlotPoints>(),
        ));
    }

    add_graph("disk", ui, lines, 100.5);

    ui.separator();
}

fn refresh_disk_io_time(appdata: &mut MyApp) {
    unsafe {
        // Siehe: https://learn.microsoft.com/en-us/windows/win32/perfctrs/pdh-error-codes
        PdhCollectQueryData(appdata.windows_performance_query_handle);
        for (d, handle, value) in &mut appdata.disk_time_value_handle_map {
            let mut new_value = Default::default();
            PdhGetFormattedCounterValue(*handle, PDH_FMT_DOUBLE, None, &mut new_value);
            *value = new_value.Anonymous.doubleValue;
            appdata
                .disk_buffer
                .entry(d.clone())
                .or_insert(CircleVec::new())
                .add(*value);
        }
    }
}

pub fn init_system(appdata: &mut MyApp) {
    // get_core_efficiency_data();

    // open_performance_browser();
    appdata.system_status.refresh_disks_list();

    unsafe { PdhOpenQueryW(None, 0, &mut appdata.windows_performance_query_handle) };
    for d in appdata.system_status.disks() {
        let drive_letter = d.mount_point().to_str().unwrap().replace('\\', "");
        let path_str = format!("\\Logischer Datenträger({drive_letter})\\Zeit (%)");
        // let path_str = format!("\\LogicalDisk({drive_letter})\\% Disk Time");
        let path = convert_to_pcwstr(&path_str);
        let mut metric_handle = 0;
        let mut result = 1;
        while result != 0 {
            unsafe {
                result = PdhAddCounterW(
                    appdata.windows_performance_query_handle,
                    path,
                    0,
                    &mut metric_handle,
                );

                if result != PDH_CSTATUS_VALID_DATA {
                    println!("Fehler beim registrieren von ({drive_letter}): path: {path_str}, result: {result:X}");
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
        appdata
            .disk_time_value_handle_map
            .push((drive_letter, metric_handle, 0.0));
    }

    unsafe {
        PdhCollectQueryData(appdata.windows_performance_query_handle);
    }
}

pub fn get_windows_glass_color() -> Color32 {
    let mut col: u32 = 0;
    let mut opaque: BOOL = BOOL(0);
    unsafe {
        DwmGetColorizationColor(&mut col, &mut opaque).unwrap();
    }
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

// fn get_core_efficiency_data() {
//     unsafe {
//         let mut req_len = 0;
//         GetSystemCpuSetInformation(None, 0, &mut req_len, None, 0);

//         let mut sys_inf = SYSTEM_CPU_SET_INFORMATION::default();
//         dbg!(GetSystemCpuSetInformation(
//             Some(&mut sys_inf),
//             dbg!(req_len),
//             &mut req_len,
//             None,
//             0
//         ));
//         dbg!(sys_inf.Anonymous.CpuSet.EfficiencyClass);
//     }
// }

#[allow(dead_code)]
fn open_performance_browser() {
    unsafe {
        let hwnd = *STATIC_HWND.read().unwrap();
        let mut buf: [u16; 1000] = [0; 1000];
        let returnpathbuffer = PWSTR::from_raw(&mut buf as *mut u16);
        let p = PWSTR::from_raw(w!("hello").as_ptr() as *mut _);
        PdhBrowseCountersW(&PDH_BROWSE_DLG_CONFIG_W {
            _bitfield: 0,
            hWndOwner: hwnd,
            szDataSource: PWSTR::null(),
            szReturnPathBuffer: returnpathbuffer,
            cchReturnPathLength: 1000,
            pCallBack: None,
            dwCallBackArg: 0,
            CallBackStatus: 0,
            dwDefaultDetailLevel: PERF_DETAIL_ADVANCED,
            szDialogBoxCaption: p,
        } as *const PDH_BROWSE_DLG_CONFIG_W);

        println!("{}", returnpathbuffer.display());
    }
}

fn convert_to_pcwstr(s: &str) -> PCWSTR {
    let input: &[u8] = s.as_bytes();
    let output: Vec<u16> = {
        let mut buffer = Vec::<u16>::new();
        let mut input_pos = 0;
        while let Some((mut code_point, new_pos)) = decode_utf8_char(input, input_pos) {
            input_pos = new_pos;
            if code_point <= 0xffff {
                buffer.push(code_point as u16);
            } else {
                code_point -= 0x10000;
                buffer.push(0xd800 + (code_point >> 10) as u16);
                buffer.push(0xdc00 + (code_point & 0x3ff) as u16);
            }
        }
        buffer.push(0);
        buffer
    };
    PCWSTR::from_raw(output.as_ptr())
}

pub fn refresh(appdata: &mut MyApp) {
    refresh_cpu(appdata);
    step_timing(appdata, CurrentStep::UpdateCPU);
    refresh_gpu(appdata);
    step_timing(appdata, CurrentStep::UpdateGPU);

    appdata.system_status.refresh_disks();
    step_timing(appdata, CurrentStep::UpdateSystemDisk);
    refresh_system_memory(appdata);
    step_timing(appdata, CurrentStep::UpdateSystemMemory);
    refresh_networks(appdata);
    step_timing(appdata, CurrentStep::UpdateSystemNetwork);

    refresh_disk_io_time(appdata);
    step_timing(appdata, CurrentStep::UpdateIoTime);
}

pub fn refresh_color(ui: &mut Ui) {
    let v = ui.visuals_mut();
    v.override_text_color = Some(Color32::from_gray(250));
    v.window_fill = get_windows_glass_color();
}

pub struct MyNetworkData {
    tx: f64,
    rx: f64,
}

fn refresh_networks(appdata: &mut MyApp) {
    appdata.system_status.refresh_networks();
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
    let ohw = appdata.ohw_info.lock();
    let mut cur_ram = 0.0;
    let mut tot_ram = 0.0;
    if let Some(ohw) = ohw.as_ref() {
        let nodes = &ohw.Children[0]
            .Children
            .iter()
            .find(|n| n.Text == "Generic Memory")
            .unwrap()
            .Children
            .iter()
            .find(|n| n.Text == "Data")
            .unwrap()
            .Children;

        cur_ram = nodes
            .iter()
            .find(|n| n.Text == "Memory Used")
            .unwrap()
            .Value
            .split_whitespace()
            .next()
            .unwrap()
            .replace(',', ".")
            .parse::<f32>()
            .unwrap()
            * 1024.0
            * 1024.0
            * 1024.0;

        tot_ram = cur_ram
            + nodes
                .iter()
                .find(|n| n.Text == "Memory Available")
                .unwrap()
                .Value
                .split_whitespace()
                .next()
                .unwrap()
                .replace(',', ".")
                .parse::<f32>()
                .unwrap()
                * 1024.0
                * 1024.0
                * 1024.0;
    }
    appdata.cur_ram = cur_ram;
    if appdata.total_ram == 0.0 {
        appdata.total_ram = tot_ram;
    }
    appdata.ram_buffer.add(cur_ram / appdata.total_ram);
}

fn refresh_cpu(appdata: &mut MyApp) {
    appdata.system_status.refresh_cpu();
    appdata
        .cpu_buffer
        .add(appdata.system_status.global_cpu_info().cpu_usage());

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
                if n.Text.contains("CPU Core #") {
                    if let Ok(val) = n.Text.replace("CPU Core #", "").parse::<i32>() {
                        Some((val, n.to_owned()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect_vec()
    } else {
        vec![]
    };

    let max_temp = coretemps
        .iter()
        .map(|(_, node)| {
            node.Value
                .replace("°C", "")
                .replace(",", ".")
                .trim()
                .parse::<f32>()
                .unwrap_or_default()
        })
        .max_by(|x, y| x.abs().partial_cmp(&y.abs()).unwrap());

    appdata.cpu_maxtemp_buffer.add(max_temp.unwrap_or(0.0));
}

#[derive(Deserialize, Default, Debug, Clone)]
#[allow(dead_code)]
#[allow(non_snake_case)]
pub struct OHWNode {
    Children: Vec<OHWNode>,
    ImageURL: String,
    Max: String,
    Min: String,
    Text: String,
    Value: String,
    id: i64,
}
