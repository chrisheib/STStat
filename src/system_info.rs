use std::ops::Add;

use eframe::egui::{
    plot::{Line, Plot, PlotPoints},
    Label, RichText, Ui,
};
use egui_extras::{Column, TableBuilder};
use itertools::Itertools;
use sysinfo::{CpuExt, DiskExt, NetworkExt, NetworksExt, ProcessExt, SystemExt};
use windows::{
    core::{decode_utf8_char, utf16_len, IntoParam, PCWSTR, PWSTR},
    h, s, w,
    Win32::{
        Foundation::BOOL,
        Storage::FileSystem::GetVolumeInformationW,
        System::Performance::{
            PdhAddCounterW, PdhAddEnglishCounterW, PdhBrowseCountersW, PdhCollectQueryData,
            PdhEnumObjectsW, PdhGetFormattedCounterValue, PdhOpenQueryW, PDH_BROWSE_DLG_CONFIG_W,
            PDH_CSTATUS_VALID_DATA, PDH_FMT_DOUBLE, PERF_DETAIL_ADVANCED, PERF_DETAIL_WIZARD,
        },
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
    // refresh_disk_io_time(appdata);

    // let mut text = format!("{:#?}", appdata.system_status)
    let mut text = String::new();

    // We display all disks' information:
    text += "\n=> disks:\n";
    for (i, disk) in appdata.system_status.disks().iter().enumerate() {
        text += &format!(
            "disk {i}: {} {} {} / {}, {:?}\n",
            disk.name().to_str().unwrap_or_default(),
            disk.mount_point().to_str().unwrap_or_default(),
            format_bytes(disk.available_space() as f64),
            format_bytes(disk.total_space() as f64),
            disk.type_()
        );
    }

    // Network interfaces name, data received and data transmitted:
    text += "\n=> networks:\n";
    for (interface_name, data) in appdata
        .system_status
        .networks()
        .iter()
        .filter(|i| i.0 == "Ethernet 2")
    {
        text += &format!(
            "{}: {} ⬆ {} ⬇\n",
            interface_name,
            format_bytes(data.transmitted() as f64),
            format_bytes(data.received() as f64),
        );
    }

    // Components temperature:
    // text += &format!("=> components:\n");
    // for component in appdata.system_status.components() {
    //     text += &format!("{component:?}\n");
    // }

    text += &format!(
        "memory: {} / {}\n",
        format_bytes(appdata.system_status.used_memory() as f64),
        format_bytes(appdata.system_status.total_memory() as f64),
    );

    // Display system information:
    // text += &format!(
    //     "System: {} {}, name: {}\n",
    //     appdata.system_status.name().unwrap_or_default(),
    //     appdata.system_status.os_version().unwrap_or_default(),
    //     appdata.system_status.host_name().unwrap_or_default()
    // );

    // Number of CPUs:
    let num_cpus = appdata.system_status.cpus().len();
    text += &format!(
        "NB CPUs: {num_cpus}, usage: {:.0} %\n\n",
        appdata.system_status.global_cpu_info().cpu_usage()
    );

    show_processes(appdata, ui, num_cpus, &mut text);
    show_ping(appdata, ui);

    text += "\nDrives:\n";
    for (d, _handle, value) in &appdata.disk_time_value_handle_map {
        text += &format!("{d}: {value:.1}% Zeit\n");
        // println!(
        //     "{d}: {:.1}% Zeit",
        //     value.Anonymous.doubleValue,
        //     // value_read.Anonymous.doubleValue  1000.0
        // );
    }

    ui.label(text);
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
            // .auto_shrink([false, false])
            .striped(true)
            .column(Column::exact((SIZE.x - 10.0) / 2.0))
            .columns(Column::exact((SIZE.x - 10.0) / 4.0), 2)
            .header(10.0, |mut header| {
                header.col(|ui| {
                    ui.add(Label::new(RichText::new(name).small()).wrap(false));
                    // ui.strong(name);
                });
                // header.col(|ui| {
                //     ui.add(Label::new(RichText::new("Disk r").small()).wrap(false));
                //     // ui.strong("Disk r");
                // });
                // header.col(|ui| {
                //     ui.add(Label::new(RichText::new("Disk w").small()).wrap(false));
                //     // ui.strong("Disk w");
                // });
                header.col(|ui| {
                    ui.add(Label::new(RichText::new("RAM").small()).wrap(false));
                    // ui.strong("RAM");
                });
                header.col(|ui| {
                    ui.add(Label::new(RichText::new("CPU").small()).wrap(false));
                    // ui.strong("CPU");
                });
            });
        table.body(|body| {
            body.rows(10.0, len, |row_index, mut row| {
                let p = &p[row_index];
                row.col(|ui| {
                    // ui.add(Label::new(p.name()).wrap(false));
                    // ui.label(p.name());
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
                // row.col(|ui| {
                //     ui.add(
                //         Label::new(
                //             RichText::new(format_bytes(p.disk_usage().read_bytes as f64)).small(),
                //         )
                //         .wrap(false),
                //     );
                //     // ui.add(Label::new(format_bytes(p.disk_usage().read_bytes as f64)).wrap(false));
                //     // ui.label(format_bytes(p.disk_usage().read_bytes as f64));
                // });
                // row.col(|ui| {
                //     ui.add(
                //         Label::new(
                //             RichText::new(format_bytes(p.disk_usage().written_bytes as f64)).small(),
                //         )
                //         .wrap(false),
                //     );
                //     // ui.add(Label::new(format_bytes(p.disk_usage().written_bytes as f64)).wrap(false));
                //     // ui.label(format_bytes(p.disk_usage().read_bytes as f64));
                // });
                row.col(|ui| {
                    ui.add(
                        Label::new(
                            RichText::new(format_bytes(p.memory as f64))
                                .small()
                                .strong(),
                        )
                        .wrap(false),
                    );
                    // ui.add(Label::new(format_bytes(p.memory() as f64)).wrap(false));
                    // ui.label(format_bytes(p.memory() as f64));
                });
                row.col(|ui| {
                    ui.add(
                        // Label::new(format!("{:.0}%", p.cpu_usage() / num_cpus as f32)).wrap(false),
                        Label::new(
                            RichText::new(format!("{:.0}%", p.cpu / num_cpus as f32))
                                .small()
                                .strong(),
                        )
                        .wrap(false),
                    );
                    // ui.label();
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

fn refresh_disk_io_time(appdata: &mut MyApp) {
    // open_performance_browser();
    unsafe {
        // let mut buf: [u16; 1000] = [0; 1000];
        // let mut buf2: u32 = 0;
        // let mut buf3: u32 = 0;
        // let mut buf4: u32 = 0;
        // let mut buf5: [u16; 1000] = [0; 1000];
        // // let volumennamebuffer = PWSTR::from_raw(&mut buf as *mut u16);
        // dbg!(GetVolumeInformationW(
        //     w!("C:\\"),
        //     Some(&mut buf),
        //     Some(&mut buf2),
        //     Some(&mut buf3),
        //     Some(&mut buf4),
        //     Some(&mut buf5)
        // ));
        // println!(
        //     " info: {} | {} | {} | {} | {}",
        //     PWSTR::from_raw(&mut buf as *mut u16).display(),
        //     buf2,
        //     buf3,
        //     buf4,
        //     PWSTR::from_raw(&mut buf5 as *mut u16).display()
        // );

        // let mut buf: [u16; 100000] = [0; 100000];
        // let returnlist = PWSTR::from_raw(&mut buf as *mut u16);
        // let mut len = 100000u32;

        // dbg!(PdhEnumObjectsW(
        //     PCWSTR::null(),
        //     PCWSTR::null(),
        //     returnlist,
        //     &mut len,
        //     PERF_DETAIL_WIZARD,
        //     BOOL(1),
        // ));

        // let mut strings = vec![];
        // let mut first_zero = false;
        // // let mut second_zero = false;
        // let mut closed = true;
        // for (i, v) in buf.iter().enumerate() {
        //     // println!("{i}: {v}");
        //     if *v == 0 {
        //         if first_zero {
        //             // if second_zero {
        //             break;
        //             // }
        //             // second_zero = true;
        //         }
        //         first_zero = true;
        //         closed = true;
        //     } else {
        //         if closed {
        //             strings.push(String::new());
        //             closed = false;
        //         }
        //         strings.last_mut().unwrap().push(*v as u8 as char);
        //         first_zero = false;
        //         // second_zero = false;
        //     }
        // }

        // println!("list: {strings:#?}",);

        // Create Performance Query
        // let mut query = 0;
        // dbg!(PdhOpenQueryW(None, 0, &mut query));

        // // open_performance_browser();

        // let mut disksecwritec = 0;
        // dbg!(PdhAddCounterW(
        //     query,
        //     w!("\\Logischer Datenträger(C:)\\Zeit (%)"),
        //     0,
        //     &mut disksecwritec,
        // ));

        // let mut disksecwrited = 0;
        // dbg!(PdhAddCounterW(
        //     query,
        //     w!("\\Logischer Datenträger(D:)\\Zeit (%)"),
        //     0,
        //     &mut disksecwrited,
        // ));

        // let mut disksecwritee = 0;
        // dbg!(PdhAddCounterW(
        //     query,
        //     w!("\\Logischer Datenträger(E:)\\Zeit (%)"),
        //     0,
        //     &mut disksecwritee,
        // ));

        // let mut disksecwritef = 0;
        // dbg!(PdhAddCounterW(
        //     query,
        //     w!("\\Logischer Datenträger(F:)\\Zeit (%)"),
        //     0,
        //     &mut disksecwritef,
        // ));

        // let mut disksecread = 0;
        // dbg!(PdhAddCounterW(
        //     query,
        //     w!("\\physicaldisk(1)\\avg. disk sec/read"),
        //     0,
        //     &mut disksecread,
        // ));

        // Siehe: https://learn.microsoft.com/en-us/windows/win32/perfctrs/pdh-error-codes

        dbg!(PdhCollectQueryData(
            appdata.windows_performance_query_handle
        ));
        for (_d, handle, value) in &mut appdata.disk_time_value_handle_map {
            // std::thread::sleep(std::time::Duration::from_millis(1000));

            let mut new_value = Default::default();
            dbg!(PdhGetFormattedCounterValue(
                *handle,
                PDH_FMT_DOUBLE,
                None,
                &mut new_value,
            ));
            *value = new_value.Anonymous.doubleValue;
            // let mut value_read = Default::default();
            // if 0 == PdhGetFormattedCounterValue(
            //     disksecread,
            //     PDH_FMT_DOUBLE,
            //     None,
            //     &mut value_read,
            // ) {
            // println!(
            //     "{d}: {:.1}% Zeit",
            //     value.Anonymous.doubleValue,
            //     // value_read.Anonymous.doubleValue  1000.0
            // );
            // }
        }
        // panic!();
    }
}

pub fn init_system(appdata: &mut MyApp) {
    appdata.system_status.refresh_disks_list();

    unsafe {
        dbg!(PdhOpenQueryW(
            None,
            0,
            &mut appdata.windows_performance_query_handle
        ))
    };
    for d in appdata.system_status.disks() {
        let drive_letter = d.mount_point().to_str().unwrap().replace('\\', "");
        let path_str = format!("\\Logischer Datenträger({drive_letter})\\Zeit (%)");
        // let path_str = format!("\\LogicalDisk({drive_letter})\\% Disk Time");
        println!("{path_str}");
        let path = convert_to_pcwstr(&path_str);
        let mut metric_handle = 0;
        let mut result = 1;
        while result != 0 {
            unsafe {
                result = dbg!(PdhAddCounterW(
                    appdata.windows_performance_query_handle,
                    path,
                    0,
                    &mut metric_handle,
                ));

                if result != PDH_CSTATUS_VALID_DATA {
                    println!("Fehler beim registrieren von ({drive_letter}): path: {path_str}, result: {result:X}");
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    // panic!();
                }
            }
        }
        dbg!(appdata
            .disk_time_value_handle_map
            .push((drive_letter, metric_handle, 0.0)));
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
        // let p = get_pwstr_from("hi");

        PdhBrowseCountersW(&PDH_BROWSE_DLG_CONFIG_W {
            _bitfield: 0,
            hWndOwner: hwnd,
            szDataSource: PWSTR::null(),
            szReturnPathBuffer: returnpathbuffer,
            cchReturnPathLength: 1000,
            // pCallBack: Some(cb),
            pCallBack: None,
            dwCallBackArg: 0,
            CallBackStatus: 0,
            dwDefaultDetailLevel: PERF_DETAIL_ADVANCED,
            szDialogBoxCaption: p,
        } as *const PDH_BROWSE_DLG_CONFIG_W);

        println!("{}", returnpathbuffer.display());
    }
}
// #[no_mangle]
// pub unsafe extern "system" fn cb(input: usize) -> i32 {
//     println!("Callback: {input}");
//     0
// }

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
