use crate::{
    bytes_format::format_bytes,
    circlevec::CircleVec,
    color::{auto_color_dark, get_base_background},
    components::cpu::show_cpu,
    ohw::MyNode,
    process::{add_english_counter, get_pdh_process_data, init_process_metrics, Process},
    sidebar::STATIC_HWND,
    step_timing,
    widgets::edgy_progress::EdgyProgressBar,
    CurrentStep, MyApp, SIDEBAR_WIDTH,
};
use chrono::{Local, Timelike};
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
use nvml_wrapper::enum_wrappers::device::Clock;
use sysinfo::{CpuExt, CpuRefreshKind, DiskExt, NetworkExt, NetworksExt, SystemExt};
use tokio::process::Command;
use windows::{
    core::PWSTR,
    w,
    Win32::{
        Foundation::BOOL,
        Graphics::Dwm::DwmGetColorizationColor,
        System::Performance::{
            PdhBrowseCountersW, PdhCollectQueryData, PdhGetFormattedCounterValue,
            PDH_BROWSE_DLG_CONFIG_W, PDH_FMT_DOUBLE, PERF_DETAIL_WIZARD,
        },
    },
};

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
        ui.push_id(format!("network graph {interface_name}"), |ui| {
            let table = TableBuilder::new(ui)
                .striped(true)
                .columns(Column::exact((SIDEBAR_WIDTH - 10.0) * 0.4), 2);
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
            &[14.0 * 1024.0 * 1024.0],
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
    utilization: f64,
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
    let perf_trace = appdata.settings.lock().current_settings.track_timings;
    step_timing(appdata, CurrentStep::UpdateGPU);
    if let Some(gpu) = appdata.nvid_info.as_ref() {
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
        let max_clock;
        if let Some(gpu) = &appdata.gpu {
            power_limit = gpu.power_limit;
            max_clock = gpu.max_clock;
        } else {
            let gpu = gpu.device_by_index(0).unwrap();
            power_limit = gpu.enforced_power_limit().unwrap() as f32 / 1000.0;
            max_clock = gpu.max_clock_info(Clock::Graphics).unwrap_or_default() as f32;
        }
        timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

        let ohw = appdata.ohw_info.lock();
        let n = ohw.select("#0|+images_icon/nvidia.png");
        if let Some(n) = n {
            timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

            temperature = n.parse_value_path_def("Temperatures|#0");
            timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

            power_usage = n.parse_value_path_def("Powers|#0");
            timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

            memory_free = n.parse_value_path_def::<f32>("Data|#0") * 1024.0 * 1024.0;
            timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

            memory_used = n.parse_value_path_def::<f32>("Data|#1") * 1024.0 * 1024.0;
            timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

            memory_total = n.parse_value_path_def::<f32>("Data|#2") * 1024.0 * 1024.0;
            timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

            fan_percentage = n.parse_value_path_def("Controls|#0");
            timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

            utilization = n.parse_value_path_def("Load|#0");
            timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

            clock_mhz = n.parse_value_path_def("Clocks|#0");
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
            max_clock,
        };
        timing_to_str(appdata.current_frame_start, &mut text, perf_trace);

        appdata.gpu_buffer.add(g.utilization);
        appdata
            .gpu_mem_buffer
            .add((g.memory_used / g.memory_total) as f64);
        appdata
            .gpu_power_buffer
            .add((g.power_usage / g.power_limit) as f64);
        appdata.gpu_temp_buffer.add((g.temperature) as f64);

        if perf_trace && appdata.framecount < 1000 {
            println!("{text}");
        }
        appdata.gpu = Some(g);
        step_timing(appdata, CurrentStep::UpdateGPU);
    }
}

fn show_processes(appdata: &mut MyApp, ui: &mut Ui) {
    ui.vertical_centered(|ui| ui.label("Processes"));
    // By CPU
    let mut p = appdata.processes.clone();

    p.sort_unstable_by(|a, b| b.cpu.total_cmp(&a.cpu));
    let cpu_count = appdata.system_status.cpus().len();
    add_process_table(
        ui,
        5,
        &p,
        "Proc CPU",
        ProcessTableDisplayMode::Cpu,
        cpu_count,
    );
    step_timing(appdata, crate::CurrentStep::ProcCPU);

    // By Memory
    let mut p = appdata.processes.clone();
    p.sort_unstable_by(|a, b| b.memory.cmp(&a.memory));
    add_process_table(
        ui,
        5,
        &p,
        "Proc Ram",
        ProcessTableDisplayMode::Ram,
        cpu_count,
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
    add_graph("ping", ui, vec![line], &[50.0]);
    step_timing(appdata, crate::CurrentStep::Ping);
    ui.separator();
}

fn show_battery(appdata: &mut MyApp, ui: &mut Ui) {
    if !appdata.battery_enabled {
        return;
    }
    ui.vertical_centered(|ui| ui.label("Battery"));
    let level = appdata.battery_level_buffer.read();
    let level_line = Line::new(
        (0..appdata.battery_level_buffer.capacity())
            .map(|i| {
                [i as f64, {
                    (if level[i] == 0.0 { 100.0 } else { level[i] }) - 50.0
                }]
            })
            .collect::<PlotPoints>(),
    );
    let charge = appdata.battery_change_buffer.read();
    let charge_line = Line::new(
        (0..appdata.battery_change_buffer.capacity())
            .map(|i| [i as f64, { charge[i] * 25.0 }])
            .collect::<PlotPoints>(),
    );

    add_graph("battery", ui, vec![level_line, charge_line], &[-50.0, 50.0]);
    step_timing(appdata, crate::CurrentStep::Ping);
    ui.separator();
}

fn show_gpu(appdata: &MyApp, ui: &mut Ui) {
    if appdata.nvid_info.is_some() {
        ui.vertical_centered(|ui| ui.label("GPU"));

        Grid::new("gpu_grid_upper")
            .num_columns(2)
            .spacing([2.0, 2.0])
            .striped(true)
            .show(ui, |ui| {
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
                        .desired_width(SIDEBAR_WIDTH / 2.0 - 5.0)
                        .fill(auto_color_dark(0)),
                );
                ui.add(
                    EdgyProgressBar::new(appdata.gpu.as_ref().unwrap().temperature / 100.0)
                        .text(
                            RichText::new(format!(
                                "{:.0} °C",
                                appdata.gpu.as_ref().unwrap().temperature
                            ))
                            .small()
                            .strong(),
                        )
                        .desired_width(SIDEBAR_WIDTH / 2.0 - 5.0)
                        .fill(auto_color_dark(3)),
                );
            });

        ui.add(
            EdgyProgressBar::new(
                appdata.gpu.as_ref().unwrap().memory_used
                    / appdata.gpu.as_ref().unwrap().memory_total,
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
            .fill(auto_color_dark(1)),
        );

        ui.add(
            EdgyProgressBar::new(
                appdata.gpu.as_ref().unwrap().power_usage
                    / appdata.gpu.as_ref().unwrap().power_limit,
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
            .fill(auto_color_dark(2)),
        );
        ui.add(
            EdgyProgressBar::new(
                appdata.gpu.as_ref().unwrap().clock_mhz
                    / appdata.gpu.as_ref().unwrap().max_clock.max(0.01),
            )
            .text(
                RichText::new(format!(
                    "Clk: {:.0}MHz / {:.0}MHz",
                    appdata.gpu.as_ref().unwrap().clock_mhz,
                    appdata.gpu.as_ref().unwrap().max_clock
                ))
                .small()
                .strong(),
            ),
        );

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
                                                    p.cpu as f32 / core_count as f32
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

pub fn add_graph(id: &str, ui: &mut Ui, line: Vec<Line>, max_y: &[f64]) {
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
        .width(SIDEBAR_WIDTH - 7.0)
        .height(30.0)
        .include_y(0.0);
    for y in max_y {
        p = p.include_y(*y);
    }
    p.set_margin_fraction(Vec2::ZERO).show(ui, |plot_ui| {
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
            for (i, d) in appdata
                .system_status
                .disks()
                .iter()
                .sorted_by_key(|d| d.mount_point())
                .enumerate()
            {
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
                            appdata.settings.lock().current_settings.location.width * 0.55,
                        )
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
                });
                ui.end_row();
            }
        });
    ui.spacing();

    let mut lines = Vec::new();
    for (_d, diskbuffer) in appdata.disk_buffer.iter().sorted_by_key(|h| h.0) {
        let values = diskbuffer.read();
        lines.push(Line::new(
            (0..diskbuffer.capacity())
                .map(|i| [i as f64, { values[i] }])
                .collect::<PlotPoints>(),
        ));
    }

    add_graph("disk", ui, lines, &[100.5]);

    ui.separator();
}

fn refresh_disk_io_time(appdata: &mut MyApp) {
    unsafe {
        // Siehe: https://learn.microsoft.com/en-us/windows/win32/perfctrs/pdh-error-codes
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
    // open_performance_browser();

    appdata.process_metric_handles = init_process_metrics(appdata.windows_performance_query_handle);
    appdata.system_status.refresh_disks_list();
    appdata.system_status.refresh_cpu();

    // iterate over disks and add disk io time counters
    for d in appdata
        .system_status
        .disks()
        .iter()
        .sorted_by_key(|d| d.mount_point())
    {
        let drive_letter = d.mount_point().to_str().unwrap().replace('\\', "");
        let metric_handle = add_english_counter(
            format!(r"\LogicalDisk({drive_letter})\% Disk Time"),
            appdata.windows_performance_query_handle,
        );

        appdata
            .disk_time_value_handle_map
            .push((drive_letter, metric_handle, 0.0));
    }

    unsafe { PdhCollectQueryData(appdata.windows_performance_query_handle) };
}

pub fn get_windows_glass_color(use_plain_blackground: bool) -> Color32 {
    if use_plain_blackground {
        return get_base_background();
    }
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

#[allow(dead_code)]
pub fn open_performance_browser() {
    unsafe {
        let hwnd = *STATIC_HWND.read().unwrap();
        let mut buf: [u16; 10000] = [0; 10000];
        let returnpathbuffer = PWSTR::from_raw(&mut buf as *mut u16);
        let p = PWSTR::from_raw(w!("hello").as_ptr() as *mut _);
        PdhBrowseCountersW(&PDH_BROWSE_DLG_CONFIG_W {
            _bitfield: 0,
            hWndOwner: hwnd,
            szDataSource: PWSTR::null(),
            szReturnPathBuffer: returnpathbuffer,
            cchReturnPathLength: 10000,
            pCallBack: None,
            dwCallBackArg: 0,
            CallBackStatus: 0,
            dwDefaultDetailLevel: PERF_DETAIL_WIZARD,
            szDialogBoxCaption: p,
        } as *const PDH_BROWSE_DLG_CONFIG_W);

        println!("{}", returnpathbuffer.display());
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
    unsafe { PdhCollectQueryData(appdata.windows_performance_query_handle) };

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

    refresh_processes(appdata);
    step_timing(appdata, CurrentStep::UpdateSystemProcess);

    refresh_battery(appdata);
    step_timing(appdata, CurrentStep::UpdateBattery);
}

fn refresh_processes(appdata: &mut MyApp) {
    appdata.processes = get_pdh_process_data(&appdata.process_metric_handles);
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
    if ohw.is_some() {
        let nodes = ohw.select("#0|Generic Memory|Data").cloned();

        cur_ram = nodes.parse_value_path_def::<f32>("Memory Used") * 1024.0 * 1024.0 * 1024.0;
        tot_ram = cur_ram
            + nodes.parse_value_path_def::<f32>("Memory Available") * 1024.0 * 1024.0 * 1024.0;
    }
    appdata.cur_ram = cur_ram;
    if appdata.total_ram == 0.0 {
        appdata.total_ram = tot_ram;
    }
    appdata.ram_buffer.add(cur_ram / appdata.total_ram);
}

fn refresh_cpu(appdata: &mut MyApp) {
    appdata
        .system_status
        .refresh_cpu_specifics(CpuRefreshKind::new().with_cpu_usage());
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
        .map(|(_, v)| v)
        .max_by(|x, y| x.abs().partial_cmp(&y.abs()).unwrap())
        .copied();

    appdata.cpu_maxtemp_buffer.add(max_temp.unwrap_or(0.0));

    let cpu_power = ohw_opt.parse_value_path_def("#0|+images_icon/cpu.png|Power|Package");

    let mut s = appdata.settings.lock();
    if cpu_power > s.current_settings.max_cpu_power {
        s.current_settings.max_cpu_power = cpu_power;
    }
    drop(s);
    appdata.cpu_power_buffer.add(cpu_power);
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
