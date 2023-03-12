use human_bytes::human_bytes;
use itertools::Itertools;
use sysinfo::{CpuExt, DiskExt, NetworkExt, NetworksExt, ProcessExt, SystemExt};

use crate::MyApp;

pub(crate) fn get_system_text(appdata: &MyApp) -> String {
    // let mut text = format!("{:#?}", appdata.system_status)
    let mut text = String::new();

    // We display all disks' information:
    text += "\n=> disks:\n";
    for disk in appdata.system_status.disks() {
        text += &format!(
            "{} {} {} / {}\n",
            disk.name().to_str().unwrap_or_default(),
            disk.mount_point().to_str().unwrap_or_default(),
            human_bytes(disk.available_space() as f64),
            human_bytes(disk.total_space() as f64),
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
            human_bytes(data.transmitted() as f64),
            human_bytes(data.received() as f64),
        );
    }

    // Components temperature:
    // text += &format!("=> components:\n");
    // for component in appdata.system_status.components() {
    //     text += &format!("{component:?}\n");
    // }

    text += &format!(
        "memory: {} / {}\n",
        human_bytes(appdata.system_status.used_memory() as f64),
        human_bytes(appdata.system_status.total_memory() as f64),
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
    for (_pid, process) in processes.iter().take(7) {
        text += &format!(
            "{}, D: {}, R: {}, C: {:.0}%",
            process.name(),
            human_bytes(process.disk_usage().read_bytes as f64),
            human_bytes(process.memory() as f64),
            process.cpu_usage() / num_cpus as f32
        );
        text += "\n";
    }

    text += "\n";

    // By Memory
    processes.sort_by(|a, b| b.1.memory().cmp(&a.1.memory()));
    for (_pid, process) in processes.iter().take(7) {
        text += &format!(
            "{}, D: {}, R: {}, C: {:.0}%",
            process.name(),
            human_bytes(process.disk_usage().read_bytes as f64),
            human_bytes(process.memory() as f64),
            process.cpu_usage() / num_cpus as f32
        );
        text += "\n";
    }

    text
}
