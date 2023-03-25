use crate::{
    bytes_format::format_bytes, components::edgy_progress::EdgyProgressBar, sidebar::STATIC_HWND,
    step_timing, CurrentStep, MyApp, MEASURE_PERFORMANCE, PERFORMANCE_FRAMES, SIZE,
};
use eframe::{
    egui::{
        plot::{Line, Plot, PlotPoints},
        Grid, Label, Layout, RichText, Ui,
    },
    emath::Align::{self, Max},
    epaint::{Color32, Vec2},
};
use egui_extras::{Column, TableBuilder};
use itertools::Itertools;
use serde::Deserialize;
use std::ops::Add;
use sysinfo::{CpuExt, DiskExt, NetworkExt, NetworksExt, ProcessExt, SystemExt};
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
    let now = chrono::Local::now();
    // ui.heading(text)

    // ui.heading(format!("{}", now.format("%d.%m.%Y")));
    ui.vertical_centered(|ui| {
        ui.heading(RichText::new(now.format("%H:%M:%S").to_string()).strong())
    });
    ui.separator();
    step_timing(appdata, crate::CurrentStep::Begin);

    // text += &format!(
    //     "memory: \n{} / {}\n",
    //     format_bytes(appdata.system_status.used_memory() as f64),
    //     format_bytes(appdata.system_status.total_memory() as f64),
    // );
    // let gpu = appdata.nvid_info.device_by_index(0).unwrap();
    // appdata.gpu = Some(gpu);

    // text += &format!(
    //     "\ngpu:\nUsage: {}%\nClock-G: {} MHz\nClock-M: {} MHz\nClock-SM: {} MHz\nClock-V: {} MHz\nTemp {}°C\nMem-Free: {}\nMem-Used: {}\nMem-Total: {}\nPower: {} / {} W\nFans: {}% | {}%\n",
    //     gpu.utilization_rates().unwrap().gpu,
    //     gpu.clock_info(Clock::Graphics).unwrap(),
    //     gpu.clock_info(Clock::Memory).unwrap(),
    //     gpu.clock_info(Clock::SM).unwrap(),
    //     gpu.clock_info(Clock::Video).unwrap(),
    //     gpu.temperature(TemperatureSensor::Gpu).unwrap(),
    //     format_bytes(gpu.memory_info().unwrap().free as f64),
    //     format_bytes(gpu.memory_info().unwrap().used as f64),
    //     format_bytes(gpu.memory_info().unwrap().total as f64),
    //     gpu.power_usage().unwrap_or_default() / 1000,
    //     gpu.enforced_power_limit().unwrap_or_default() / 1000,
    //     gpu.fan_speed(0).unwrap_or_default(),
    //     gpu.fan_speed(1).unwrap_or_default(),
    // );

    // text += &format!("{:#?}\n", appdata.ohw_info);

    // let sys = System::new();
    // text += &format!("cpu temp: {} °C", sys.cpu_load().unwrap().done().unwrap()[0].platform.);

    show_cpu(appdata, ui);
    show_processes(appdata, ui);
    show_ping(appdata, ui);
    show_gpu(appdata, ui);
    add_drives_section(appdata, ui);

    let mut text = String::new();

    // Network interfaces name, data received and data transmitted:
    text += "\n=> networks:\n";
    for (interface_name, data) in appdata
        .system_status
        .networks()
        .iter()
        .filter(|i| i.0 == "Ethernet 2")
    {
        text += &format!(
            "{}:\n⬆ {}\n⬇ {}\n",
            interface_name,
            format_bytes(data.transmitted() as f64),
            format_bytes(data.received() as f64),
        );
    }
    step_timing(appdata, crate::CurrentStep::Network);

    // text += &format!("{:?}", appdata.gpu);
    // step_timing(appdata, crate::CurrentStep::GPU);

    ui.label(text);
    ui.separator();
}

#[derive(Default, Debug, Clone)]
#[allow(dead_code)]
pub struct GpuData {
    utilization: f32,
    // clock_graphics: u32,
    // clock_memory: u32,
    // clock_sm: u32,
    // clock_video: u32,
    temperature: f32,
    memory_free: f32,
    memory_used: f32,
    memory_total: f32,
    power_usage: f32,
    power_limit: f32,
    fan_percentage: f32,
    clock_mhz: f32,
}

fn timing_to_str(timestamp: std::time::Instant, text: &mut String) {
    if MEASURE_PERFORMANCE {
        *text += &format!("{}\n", timestamp.elapsed().as_micros());
    }
}

pub fn refresh_gpu(appdata: &mut MyApp) {
    step_timing(appdata, CurrentStep::UpdateGPU);

    let mut text = String::new();
    timing_to_str(appdata.current_frame_start, &mut text); // 96

    // let gpu = appdata.nvid_info.device_by_index(0)?;
    // timing_to_str(appdata.current_frame_start, &mut text); // 105

    // let utilization = gpu.utilization_rates()?.gpu;
    // timing_to_str(appdata.current_frame_start, &mut text); // 585

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
    timing_to_str(appdata.current_frame_start, &mut text); // 3910

    let ohw = appdata.ohw_info.lock();
    if let Some(ohw) = ohw.as_ref() {
        let nodes = &ohw.Children[0]
            .Children
            .iter()
            .find(|n| n.ImageURL == "images_icon/nvidia.png")
            .unwrap()
            .Children;
        timing_to_str(appdata.current_frame_start, &mut text); // 3910

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
        timing_to_str(appdata.current_frame_start, &mut text); // 3910

        power_usage = nodes.iter().find(|n| n.Text == "Powers").unwrap().Children[0]
            .Value
            .split_whitespace()
            .next()
            .unwrap()
            .replace(',', ".")
            .parse::<f32>()
            .unwrap();
        timing_to_str(appdata.current_frame_start, &mut text); // 3910

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
        timing_to_str(appdata.current_frame_start, &mut text); // 3910

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
        timing_to_str(appdata.current_frame_start, &mut text); // 3910

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
        timing_to_str(appdata.current_frame_start, &mut text); // 3910

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
        timing_to_str(appdata.current_frame_start, &mut text); // 3910

        utilization = nodes.iter().find(|n| n.Text == "Load").unwrap().Children[0]
            .Value
            .split_whitespace()
            .next()
            .unwrap()
            .replace(',', ".")
            .parse::<f32>()
            .unwrap();
        timing_to_str(appdata.current_frame_start, &mut text); // 3910

        clock_mhz = nodes.iter().find(|n| n.Text == "Clocks").unwrap().Children[0]
            .Value
            .split_whitespace()
            .next()
            .unwrap()
            .replace(',', ".")
            .parse::<f32>()
            .unwrap();
        timing_to_str(appdata.current_frame_start, &mut text); // 3910
    };
    drop(ohw);

    // let clock_graphics = gpu.clock_info(Clock::Graphics)?;
    // timing_to_str(appdata.current_frame_start, &mut text); // 839

    // let clock_memory = gpu.clock_info(Clock::Memory)?;
    // timing_to_str(appdata.current_frame_start, &mut text); // 960

    // let clock_sm = gpu.clock_info(Clock::SM)?;
    // timing_to_str(appdata.current_frame_start, &mut text); // 1069

    // let clock_video = gpu.clock_info(Clock::Video)?;
    // timing_to_str(appdata.current_frame_start, &mut text); // 1177

    // let temperature = gpu.temperature(TemperatureSensor::Gpu)?;

    // timing_to_str(appdata.current_frame_start, &mut text);
    // let gpu_memory = gpu.memory_info()?;
    // let memory_free = gpu_memory.free;
    // timing_to_str(appdata.current_frame_start, &mut text); // 3500
    // let memory_used = gpu_memory.used;
    // timing_to_str(appdata.current_frame_start, &mut text); //
    // let memory_total = gpu_memory.total;
    // timing_to_str(appdata.current_frame_start, &mut text); //

    // let power_usage = gpu.power_usage()? / 1000;
    // timing_to_str(appdata.current_frame_start, &mut text); // 3800
    // let power_limit = gpu.enforced_power_limit()? / 1000;
    // timing_to_str(appdata.current_frame_start, &mut text); // 3910

    let g = GpuData {
        utilization,
        // clock_graphics,
        // clock_memory,
        // clock_sm,
        // clock_video,
        temperature,
        memory_free,
        memory_used,
        memory_total,
        power_usage,
        power_limit,
        fan_percentage,
        clock_mhz,
    };
    timing_to_str(appdata.current_frame_start, &mut text); // 4267

    // for i in 0..gpu.num_fans()? {
    //     g.fan_speeds.push(gpu.fan_speed(i)?);
    //     timing_to_str(appdata.current_frame_start, &mut text); // 4552
    //                                                            // 9557, 10231
    // }

    if MEASURE_PERFORMANCE && appdata.framecount < PERFORMANCE_FRAMES {
        println!("{text}");
    }
    appdata.gpu = Some(g);
    step_timing(appdata, CurrentStep::UpdateGPU);
}

fn show_processes(appdata: &mut MyApp, ui: &mut Ui) {
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
    let pings = appdata.ping_buffer.read();
    let last_ping = pings.last().copied().unwrap_or_default();
    let max_ping = pings.iter().max().copied().unwrap_or_default();
    let line = Line::new(
        (0..appdata.ping_buffer.capacity())
            .map(|i| [i as f64, { pings[i] as f64 }])
            .collect::<PlotPoints>(),
    );

    ui.label(format!("M: {max_ping:.0}ms, C: {last_ping:.0} ms"));
    add_graph("ping", ui, line, 50.0);
    step_timing(appdata, crate::CurrentStep::Ping);
    ui.separator();
}

fn show_cpu(appdata: &mut MyApp, ui: &mut Ui) {
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

    drop(ohw_opt);
    // for (i, n) in coretemps {
    //     text += &format!("Core {i}: {}\n", n.Value);
    // }

    step_timing(appdata, crate::CurrentStep::CpuCrunch);
    ui.spacing_mut().interact_size = [15.0, 12.0].into();

    let cpu = appdata.cpu_buffer.read();
    let last_cpu = cpu.last().copied().unwrap_or_default();

    ui.add(
        EdgyProgressBar::new(last_cpu / 100.0).text(
            RichText::new(format!("CPU: {last_cpu:.1}%",))
                .small()
                .strong(),
        ),
    );

    ui.add(
        EdgyProgressBar::new(appdata.cur_ram / appdata.total_ram).text(
            RichText::new(format!(
                "RAM: {} / {}",
                format_bytes(appdata.cur_ram as f64),
                format_bytes(appdata.total_ram as f64)
            ))
            .small()
            .strong(),
        ),
    );

    Grid::new("cpu_grid")
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

    let line = Line::new(
        (0..appdata.ping_buffer.capacity())
            .map(|i| [i as f64, { cpu[i] as f64 }])
            .collect::<PlotPoints>(),
    );

    step_timing(appdata, crate::CurrentStep::CPU);
    add_graph("cpu", ui, line, 100.5);
    step_timing(appdata, crate::CurrentStep::CPUGraph);

    ui.separator();
}

fn show_gpu(appdata: &MyApp, ui: &mut Ui) {
    ui.label(format!("{:?}", appdata.gpu));
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
                ui.add(Label::new(RichText::new(name).small()).wrap(false));
            });
            if display_mode == ProcessTableDisplayMode::All
                || display_mode == ProcessTableDisplayMode::Ram
            {
                header.col(|ui| {
                    ui.with_layout(Layout::top_down_justified(Max), |ui| {
                        ui.add(Label::new(RichText::new("RAM").small()).wrap(false));
                    });
                });
            }
            if display_mode == ProcessTableDisplayMode::All
                || display_mode == ProcessTableDisplayMode::Cpu
            {
                header.col(|ui| {
                    ui.with_layout(Layout::top_down_justified(Max), |ui| {
                        ui.add(Label::new(RichText::new("CPU").small()).wrap(false));
                    });
                });
            }
        });
        table.body(|body| {
            body.rows(10.0, len, |row_index, mut row| {
                let p = &p[row_index];
                row.col(|ui| {
                    ui.add(
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
                    );
                });
                if display_mode == ProcessTableDisplayMode::All
                    || display_mode == ProcessTableDisplayMode::Ram
                {
                    row.col(|ui| {
                        ui.with_layout(Layout::top_down_justified(Max), |ui| {
                            ui.add(
                                Label::new(
                                    RichText::new(format_bytes(p.memory as f64))
                                        .small()
                                        .strong(),
                                )
                                .wrap(false),
                            )
                        });
                    });
                }
                if display_mode == ProcessTableDisplayMode::All
                    || display_mode == ProcessTableDisplayMode::Cpu
                {
                    row.col(|ui| {
                        ui.with_layout(Layout::top_down_justified(Max), |ui| {
                            ui.add(
                                Label::new(
                                    RichText::new(format!("{:.1}%", p.cpu / num_cpus as f32))
                                        .small()
                                        .strong(),
                                )
                                .wrap(false),
                            );
                        });
                    });
                }
            });
        });
    });
    ui.separator();
}

fn add_graph(id: &str, ui: &mut Ui, line: Line, max_y: f64) {
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
        .show(ui, |plot_ui| plot_ui.line(line));
}

fn add_drives_section(appdata: &MyApp, ui: &mut Ui) {
    ui.spacing_mut().interact_size = [15.0, 12.0].into();
    Grid::new("drive_grid")
        .num_columns(2)
        .spacing([2.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            for d in appdata.system_status.disks() {
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
                        .desired_width(SIZE.x * 0.6)
                        .text(
                            RichText::new(format!(
                                "Free: {}",
                                format_bytes(d.available_space() as f64),
                            ))
                            .small()
                            .strong(),
                        ),
                    )
                });
                ui.end_row();
            }
        });
    ui.separator();
}

fn refresh_disk_io_time(appdata: &mut MyApp) {
    unsafe {
        // Siehe: https://learn.microsoft.com/en-us/windows/win32/perfctrs/pdh-error-codes
        PdhCollectQueryData(appdata.windows_performance_query_handle);
        for (_d, handle, value) in &mut appdata.disk_time_value_handle_map {
            let mut new_value = Default::default();
            PdhGetFormattedCounterValue(*handle, PDH_FMT_DOUBLE, None, &mut new_value);
            *value = new_value.Anonymous.doubleValue;
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
        // let bytes1: [u8; 4] = dbg!(GetSysColor(SYS_COLOR_INDEX(5)).to_le_bytes());
    }
    let bytes: [u8; 4] = col.to_be_bytes();
    Color32::from_rgba_premultiplied(
        darken(bytes[1]),
        darken(bytes[2]),
        darken(bytes[3]),
        bytes[0],
    )

    // out = out.linear_multiply(1.2);
    // out
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
    // appdata.system_status.refresh_memory();
    step_timing(appdata, CurrentStep::UpdateSystemMemory);
    appdata.system_status.refresh_networks();
    step_timing(appdata, CurrentStep::UpdateSystemNetwork);

    refresh_disk_io_time(appdata);
    step_timing(appdata, CurrentStep::UpdateIoTime);
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
}

fn refresh_cpu(appdata: &mut MyApp) {
    appdata.system_status.refresh_cpu();
    appdata
        .cpu_buffer
        .add(appdata.system_status.global_cpu_info().cpu_usage());
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
