use eframe::egui::{Label, RichText, Ui};
use egui_extras::{Column, TableBuilder};
use itertools::Itertools;
use sysinfo::{CpuExt, DiskExt, NetworkExt, NetworksExt, Pid, Process, ProcessExt, SystemExt};

use crate::{bytes_format::format_bytes, MyApp, SIZE};

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

    // Processes
    let mut processes = appdata.system_status.processes().iter().collect_vec();

    // By CPU
    processes.sort_by(|a, b| b.1.cpu_usage().total_cmp(&a.1.cpu_usage()));
    add_process_table(ui, 7, &processes, num_cpus, "Proc CPU");
    ui.separator();

    // for (_pid, process) in processes.iter().take(7) {
    //     text += &format!(
    //         "{}, D: {}, R: {}, C: {:.0}%",
    //         process.name(),
    //         format_bytes(process.disk_usage().read_bytes as f64),
    //         format_bytes(process.memory() as f64),
    //         process.cpu_usage() / num_cpus as f32
    //     );
    //     text += "\n";
    // }

    text += "\n";

    // By Memory
    processes.sort_by(|a, b| b.1.memory().cmp(&a.1.memory()));
    add_process_table(ui, 7, &processes, num_cpus, "Proc Ram");
    ui.separator();
    // for (_pid, process) in processes.iter().take(7) {
    //     text += &format!(
    //         "{}, D: {}, R: {}, C: {:.0}%",
    //         process.name(),
    //         format_bytes(process.disk_usage().read_bytes as f64),
    //         format_bytes(process.memory() as f64),
    //         process.cpu_usage() / num_cpus as f32
    //     );
    //     text += "\n";
    // }

    ui.label(text);
}

fn add_process_table(ui: &mut Ui, len: usize, p: &[(&Pid, &Process)], num_cpus: usize, name: &str) {
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
                let p = p[row_index].1;
                row.col(|ui| {
                    // ui.add(Label::new(p.name()).wrap(false));
                    // ui.label(p.name());
                    ui.add(Label::new(RichText::new(p.name()).small().strong()).wrap(false));
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
                            RichText::new(format_bytes(p.memory() as f64))
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
                            RichText::new(format!("{:.0}%", p.cpu_usage() / num_cpus as f32))
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
