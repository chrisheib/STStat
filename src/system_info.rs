use crate::{bytes_format::format_bytes, sidebar::STATIC_HWND, MyApp, SIZE};
use eframe::{
    egui::{
        plot::{Line, Plot, PlotPoints},
        Grid, Label, Layout, ProgressBar, RichText, Ui,
    },
    emath::Align::{self, Max},
};
use egui_extras::{Column, TableBuilder};
use itertools::Itertools;
use nvml_wrapper::{
    enum_wrappers::device::{Clock, TemperatureSensor},
    error::NvmlError,
};
use serde::Deserialize;
use std::ops::Add;
use sysinfo::{CpuExt, DiskExt, NetworkExt, NetworksExt, ProcessExt, SystemExt};
// use systemstat::{Platform, System};
use windows::{
    core::{decode_utf8_char, PCWSTR, PWSTR},
    w,
    Win32::System::Performance::{
        PdhAddCounterW, PdhBrowseCountersW, PdhCollectQueryData, PdhGetFormattedCounterValue,
        PdhOpenQueryW, PDH_BROWSE_DLG_CONFIG_W, PDH_CSTATUS_VALID_DATA, PDH_FMT_DOUBLE,
        PERF_DETAIL_ADVANCED,
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

pub fn set_system_info_components(appdata: &MyApp, ui: &mut Ui) {
    let now = chrono::Local::now();
    // ui.heading(text)

    // ui.heading(format!("{}", now.format("%d.%m.%Y")));
    ui.vertical_centered(|ui| {
        ui.heading(RichText::new(now.format("%H:%M:%S").to_string()).strong())
    });
    ui.separator();
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
            "{}:\n{} ⬆\n{} ⬇\n",
            interface_name,
            format_bytes(data.transmitted() as f64),
            format_bytes(data.received() as f64),
        );
    }

    text += &format!("{:#?}", appdata.gpu);

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
    add_drives_section(appdata, ui);

    ui.label(text);
    ui.separator();
}

#[derive(Default, Debug, Clone)]
#[allow(dead_code)]
pub struct GpuData {
    utilization: u32,
    clock_graphics: u32,
    clock_memory: u32,
    clock_sm: u32,
    clock_video: u32,
    temperature: u32,
    memory_free: u64,
    memory_used: u64,
    memory_total: u64,
    power_usage: u32,
    power_limit: u32,
    fan_speeds: Vec<u32>,
}

pub fn refresh_gpu(appdata: &mut MyApp) {
    if let Ok(g) = || -> Result<GpuData, NvmlError> {
        let gpu = appdata.nvid_info.device_by_index(0)?;
        let mut g = GpuData {
            utilization: gpu.utilization_rates()?.gpu,
            clock_graphics: gpu.clock_info(Clock::Graphics)?,
            clock_memory: gpu.clock_info(Clock::Memory)?,
            clock_sm: gpu.clock_info(Clock::SM)?,
            clock_video: gpu.clock_info(Clock::Video)?,
            temperature: gpu.temperature(TemperatureSensor::Gpu)?,
            memory_free: gpu.memory_info()?.free,
            memory_used: gpu.memory_info()?.used,
            memory_total: gpu.memory_info()?.total,
            power_usage: gpu.power_usage()? / 1000,
            power_limit: gpu.enforced_power_limit()? / 1000,
            fan_speeds: vec![],
        };
        for i in 0..gpu.num_fans()? {
            g.fan_speeds.push(gpu.fan_speed(i)?);
        }
        Ok(g)
    }() {
        appdata.gpu = Some(g);
    } else {
        appdata.gpu = None;
    };
}

fn show_processes(appdata: &MyApp, ui: &mut Ui) {
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
    add_process_table(ui, 7, &processes, num_cpus, "Proc CPU");

    // By Memory
    processes.sort_by(|a, b| b.memory.cmp(&a.memory));
    add_process_table(ui, 7, &processes, num_cpus, "Proc Ram");
}

fn show_ping(appdata: &MyApp, ui: &mut Ui) {
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
    ui.separator();
}

fn show_cpu(appdata: &MyApp, ui: &mut Ui) {
    let ohw_opt = appdata.ohw_info.lock().unwrap();
    let ohw_opt_ref = ohw_opt.as_ref();
    let coretemps = if let Some(ohw) = ohw_opt_ref {
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
                    Some((n.Text.replace("CPU Core #", "").parse::<i32>().unwrap(), n))
                } else {
                    None
                }
            })
            .collect_vec()
    } else {
        vec![]
    };
    // for (i, n) in coretemps {
    //     text += &format!("Core {i}: {}\n", n.Value);
    // }

    ui.spacing_mut().interact_size = [15.0, 12.0].into();

    let cur_mem = appdata.system_status.used_memory() as f32;
    let total_mem = appdata.system_status.total_memory() as f32;

    let cpu = appdata.cpu_buffer.read();
    let last_cpu = cpu.last().copied().unwrap_or_default();

    ui.add(
        ProgressBar::new(last_cpu / 100.0).text(
            RichText::new(format!("CPU: {last_cpu:.1}%",))
                .small()
                .strong(),
        ),
    );

    ui.add(
        ProgressBar::new(cur_mem / total_mem).text(
            RichText::new(format!(
                "RAM: {} / {}",
                format_bytes(cur_mem as f64),
                format_bytes(total_mem as f64)
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
                        ProgressBar::new(usage / 100.0)
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

    add_graph("cpu", ui, line, 100.5);

    ui.separator();
}

fn add_process_table(ui: &mut Ui, len: usize, p: &[Proc], num_cpus: usize, name: &str) {
    ui.push_id(name, |ui| {
        let table = TableBuilder::new(ui)
            .striped(true)
            .column(Column::exact((SIZE.x - 10.0) / 2.0))
            .columns(Column::exact((SIZE.x - 10.0) / 4.0), 2)
            .header(10.0, |mut header| {
                header.col(|ui| {
                    ui.add(Label::new(RichText::new(name).small()).wrap(false));
                });
                header.col(|ui| {
                    ui.add(Label::new(RichText::new("RAM").small()).wrap(false));
                });
                header.col(|ui| {
                    ui.add(Label::new(RichText::new("CPU").small()).wrap(false));
                });
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
                row.col(|ui| {
                    ui.add(
                        Label::new(
                            RichText::new(format!("{:.0}%", p.cpu / num_cpus as f32))
                                .small()
                                .strong(),
                        )
                        .wrap(false),
                    );
                });
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
        .width(SIZE.x - 10.0)
        .height(30.0)
        .include_y(0.0)
        .include_y(max_y)
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
                    ui.add_space(15.0);
                    ui.add(
                        ProgressBar::new(
                            (d.total_space() - d.available_space()) as f32 / d.total_space() as f32,
                        )
                        .desired_width(SIZE.x / 1.6)
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
    refresh_gpu(appdata);

    appdata.system_status.refresh_disks();
    appdata.system_status.refresh_memory();
    appdata.system_status.refresh_networks();
    appdata.system_status.refresh_processes();

    refresh_disk_io_time(appdata);
}

fn refresh_cpu(appdata: &mut MyApp) {
    appdata.system_status.refresh_cpu();
    appdata
        .cpu_buffer
        .add(appdata.system_status.global_cpu_info().cpu_usage());
}

#[derive(Deserialize, Default, Debug)]
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
