use std::{
    collections::HashMap,
    fs::read_to_string,
    io::{BufRead, BufReader},
    process::Stdio,
    sync::{Arc, Mutex},
    thread::{self},
    time::{Duration, Instant},
};

use crate::{
    color::get_base_background,
    components::section::{
        layout_debug_enabled, SIDEBAR_SECTION_RIGHT_GUARD, SIDEBAR_SECTION_SIDE_MARGIN,
    },
    disk::refresh_disks,
    network::refresh_networks,
    sections::{
        gpu::{refresh_gpu, GpuData},
        render_sections,
    },
    step_timing,
    tasks::schedule_tasks_refresh,
    CurrentStep, MyApp,
};
use eframe::egui::RichText;
use eframe::egui::Ui;
use eframe::epaint::Color32;
use itertools::Itertools;
use sysinfo::{CpuRefreshKind, ProcessRefreshKind};
use tokio::process::Command;

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

    render_sections(appdata, ui);
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
