use std::ops::Add;

use eframe::{
    egui::{
        plot::{Line, Plot, PlotPoints},
        Grid, Label, Layout, ProgressBar, RichText, Ui,
    },
    emath::Align::Max,
    epaint::Rect,
};
use egui_extras::{Column, TableBuilder};
use itertools::Itertools;
use sysinfo::{CpuExt, DiskExt, NetworkExt, NetworksExt, ProcessExt, SystemExt};
use windows::{
    core::{decode_utf8_char, PCWSTR, PWSTR},
    w,
    Win32::System::Performance::{
        PdhAddCounterW, PdhBrowseCountersW, PdhCollectQueryData, PdhGetFormattedCounterValue,
        PdhOpenQueryW, PDH_BROWSE_DLG_CONFIG_W, PDH_CSTATUS_VALID_DATA, PDH_FMT_DOUBLE,
        PERF_DETAIL_ADVANCED,
    },
};

use crate::{bytes_format::format_bytes, sidebar::STATIC_HWND, MyApp, SIZE};

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

pub(crate) fn set_system_info_components(appdata: &MyApp, ui: &mut Ui) {
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

    text += &format!(
        "memory: \n{} / {}\n",
        format_bytes(appdata.system_status.used_memory() as f64),
        format_bytes(appdata.system_status.total_memory() as f64),
    );

    // Number of CPUs:
    let num_cpus = appdata.system_status.cpus().len();
    text += &format!(
        "NB CPUs: {num_cpus}\nusage: {:.1} %",
        appdata.system_status.global_cpu_info().cpu_usage()
    );

    show_processes(appdata, ui, num_cpus, &mut text);
    show_ping(appdata, ui);

    ui.label(text);

    add_drives_section(appdata, ui);
    ui.separator();
}

fn show_processes(appdata: &MyApp, ui: &mut Ui, num_cpus: usize, text: &mut String) {
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
    ui.separator();

    *text += "\n";

    // By Memory
    processes.sort_by(|a, b| b.memory.cmp(&a.memory));
    add_process_table(ui, 7, &processes, num_cpus, "Proc Ram");
    ui.separator();
}

fn show_ping(appdata: &MyApp, ui: &mut Ui) {
    let pings = appdata.ping_buffer.read();
    let last_ping = pings.last().copied().unwrap_or_default();
    let max_ping = pings.iter().max().copied().unwrap_or_default();
    let line = Line::new(
        (0..appdata.ping_buffer.capacity())
            .map(|i| {
                [i as f64, {
                    if i < pings.len() {
                        pings[i] as f64
                    } else {
                        0.0
                    }
                }]
            })
            .collect::<PlotPoints>(),
    );

    ui.label(format!("M: {max_ping:.0}ms, C: {last_ping:.0} ms"));
    add_graph("ping", ui, line);
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
}

fn add_graph(id: &str, ui: &mut Ui, line: Line) {
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
        .include_y(50.0)
        .show(ui, |plot_ui| plot_ui.line(line));
}

fn add_drives_section(appdata: &MyApp, ui: &mut Ui) {
    ui.spacing_mut().interact_size = [15.0, 12.0].into();
    Grid::new("my_grid")
        .num_columns(2)
        .spacing([15.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            for d in appdata.system_status.disks() {
                let mount = d.mount_point().to_str().unwrap().replace('\\', "");
                let (_, _, value) = appdata
                    .disk_time_value_handle_map
                    .iter()
                    .find(|(s, _, _)| s == &mount)
                    .unwrap();

                let pos = ui.next_widget_position();
                ui.put(
                    Rect {
                        min: [pos.x - 20.0, pos.y - 5.0].into(),
                        max: [pos.x + 15.0, pos.y + 5.0].into(),
                    },
                    Label::new(
                        RichText::new(format!("{mount} {value:.1}%"))
                            .small()
                            .strong(),
                    ),
                );
                ui.add(
                    ProgressBar::new(
                        (d.total_space() - d.available_space()) as f32 / d.total_space() as f32,
                    )
                    .desired_width(SIZE.x / 1.85)
                    .text(
                        RichText::new(format!(
                            "Free: {}",
                            format_bytes(d.available_space() as f64),
                        ))
                        .small()
                        .strong(),
                    ),
                );
                ui.end_row();
            }
        });
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
        dbg!(PdhCollectQueryData(
            appdata.windows_performance_query_handle
        ));
    }
}

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
    appdata.system_status.refresh_all();
    refresh_disk_io_time(appdata);
}
