use std::{
    fs,
    io::{self, Read},
    str,
    sync::Arc,
};

use sysinfo::Networks;

use crate::{circlevec::CircleVec, MyApp};

#[derive(Debug)]
pub struct NetworkTracker {
    pub interface: String,
    pub last_data: NetworkStats,
    pub history_down: Arc<CircleVec<f64, 100>>,
    pub history_up: Arc<CircleVec<f64, 100>>,
}

#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    // pub rx_packets: u64,
    // pub tx_packets: u64,
    // pub rx_errors: u64,
    // pub tx_errors: u64,
}

impl NetworkTracker {
    pub fn update(&mut self) -> io::Result<()> {
        let new_data = network_stats(&self.interface)?;
        if self.last_data.rx_bytes > 0 || self.last_data.tx_bytes > 0 {
            // only update history if we have previous data
            let delta_rx = new_data.rx_bytes as f64 - self.last_data.rx_bytes as f64;
            let delta_tx = new_data.tx_bytes as f64 - self.last_data.tx_bytes as f64;

            self.history_down.add(delta_rx);
            self.history_up.add(delta_tx);
        }
        self.last_data = new_data;
        Ok(())
    }
}

fn read_file(path: &str) -> io::Result<String> {
    let mut s = String::new();
    fs::File::open(path)
        .and_then(|mut f| f.read_to_string(&mut s))
        .map(|_| s)
}

fn value_from_file<T: str::FromStr>(path: &str) -> io::Result<T> {
    read_file(path)?
        .trim_end_matches('\n')
        .parse()
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("File: \"{}\" doesn't contain an int value", &path),
            )
        })
}

pub fn network_stats(interface: &str) -> io::Result<NetworkStats> {
    let path_root: String = ("/sys/class/net/".to_string() + interface) + "/statistics/";
    let stats_file = |file: &str| (&path_root).to_string() + file;

    let rx_bytes: u64 = value_from_file::<u64>(&stats_file("rx_bytes"))?;
    let tx_bytes: u64 = value_from_file::<u64>(&stats_file("tx_bytes"))?;
    // let rx_packets: u64 = value_from_file::<u64>(&stats_file("rx_packets"))?;
    // let tx_packets: u64 = value_from_file::<u64>(&stats_file("tx_packets"))?;
    // let rx_errors: u64 = value_from_file::<u64>(&stats_file("rx_errors"))?;
    // let tx_errors: u64 = value_from_file::<u64>(&stats_file("tx_errors"))?;

    Ok(NetworkStats {
        rx_bytes,
        tx_bytes,
        // rx_packets,
        // tx_packets,
        // rx_errors,
        // tx_errors,
    })
}

// fn filter_networks(appdata: &mut MyApp) -> Vec<(String, MyNetworkData)> {
//     appdata
//         .networks
//         .iter()
//         .filter(|i| {
//             *appdata
//                 .settings
//                 .lock()
//                 .current_settings
//                 .networks
//                 .entry(i.0.to_string())
//                 .or_default()
//         })
//         .map(|(n, d)| {
//             (
//                 n.to_string(),
//                 MyNetworkData {
//                     tx: d.transmitted() as f64,
//                     rx: d.received() as f64,
//                 },
//             )
//         })
//         .collect_vec()
// }

// pub struct MyNetworkData {
//     tx: f64,
//     rx: f64,
// }

pub fn refresh_networks(appdata: &mut MyApp) {
    // sync knowm networks with settings
    let lock = appdata.settings.lock();
    let known_networks = lock.current_settings.networks.clone();
    drop(lock);
    for (name, enabled) in &known_networks {
        if *enabled {
            if !appdata.networks.iter().any(|n| &n.interface == name) {
                let new_tracker = NetworkTracker {
                    interface: name.clone(),
                    last_data: NetworkStats {
                        rx_bytes: 0,
                        tx_bytes: 0,
                    },
                    history_down: CircleVec::new(),
                    history_up: CircleVec::new(),
                };
                appdata.networks.push(new_tracker);
            }
        }
    }
    // remove networks that are not in settings
    appdata.networks.retain(|n| {
        if let Some(enabled) = known_networks.get(&n.interface) {
            *enabled
        } else {
            false
        }
    });

    for tracker in &mut appdata.networks {
        if let Err(e) = tracker.update() {
            eprintln!("Error updating network {}: {}", tracker.interface, e);
        }
    }

    // appdata.networks.refresh(true);
    // for (name, data) in filter_networks(appdata) {
    //     let e = appdata
    //         .net_down_buffer
    //         .entry(name.clone())
    //         .or_insert(CircleVec::new());
    //     e.add(data.rx);
    //     let e = appdata
    //         .net_up_buffer
    //         .entry(name.clone())
    //         .or_insert(CircleVec::new());
    //     e.add(data.tx);
    // }
}

pub fn init_networks(appdata: &mut MyApp) {
    let networks = Networks::new_with_refreshed_list();
    let mut lock = appdata.settings.lock();
    // sync settings networks with real networks
    for (name, _data) in &networks {
        lock.current_settings
            .networks
            .entry(name.clone())
            .or_insert(false);
    }
    lock.current_settings
        .networks
        .retain(|name, _enabled| networks.contains_key(name));
    drop(lock);
}
