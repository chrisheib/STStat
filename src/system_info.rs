use std::ops::Add;

use eframe::egui::{
    plot::{Line, Plot, PlotPoints},
    Label, RichText, Ui,
};
use egui_extras::{Column, TableBuilder};
use itertools::Itertools;
use sysinfo::{CpuExt, DiskExt, NetworkExt, NetworksExt, ProcessExt, SystemExt};

use crate::{bytes_format::format_bytes, MyApp, SIZE};

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
    // let mut text = format!("{:#?}", appdata.system_status)
    let mut text = String::new();

    // We display all disks' information:
    text += "\n=> disks:\n";
    for disk in appdata.system_status.disks() {
        text += &format!(
            "{} {} {} / {}\n",
            disk.name().to_str().unwrap_or_default(),
            disk.mount_point().to_str().unwrap_or_default(),
            format_bytes(disk.available_space() as f64),
            format_bytes(disk.total_space() as f64),
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
