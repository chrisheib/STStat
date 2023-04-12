use itertools::Itertools;
use std::{self, ops::Add};
use windows::{
    core::PCSTR,
    Win32::{
        Foundation::{ERROR_SUCCESS, WIN32_ERROR},
        System::Performance::{
            PdhAddEnglishCounterA, PdhGetFormattedCounterArrayA, PDH_CSTATUS_VALID_DATA, PDH_FMT,
            PDH_FMT_COUNTERVALUE_ITEM_A, PDH_FMT_LARGE,
        },
    },
};

#[derive(Default, Debug, Clone)]
pub struct Process {
    pub name: String,
    pub cpu: f64,
    pub memory: i64,
    pub count: u64,
}

#[derive(Default, Debug)]
pub struct ProcessMetricHandles {
    pub cpu_handle: isize,
    pub ram_handle: isize,
}

impl Add for Process {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            name: self.name,
            cpu: self.cpu + rhs.cpu,
            memory: self.memory + rhs.memory,
            // disk_read: self.disk_read + rhs.disk_read,
            // disk_write: self.disk_write + rhs.disk_write,
            count: self.count + rhs.count,
        }
    }
}

/// (cpu_metric_handle, ram_metric_handle)
pub fn init_process_metrics(metric_query_handle: isize) -> ProcessMetricHandles {
    let cpu_metric_handle = add_english_counter(
        r"\Process V2(*)\% Processor Time".to_string(),
        metric_query_handle,
    );

    let ram_metric_handle = add_english_counter(
        r"\Process V2(*)\Working Set".to_string(),
        metric_query_handle,
    );
    ProcessMetricHandles {
        cpu_handle: cpu_metric_handle,
        ram_handle: ram_metric_handle,
    }
}

pub fn add_english_counter(mut path_str: String, query_handle: isize) -> isize {
    path_str.push('\0');
    let path = PCSTR::from_raw(path_str.as_bytes().as_ptr());

    let mut metric_handle = 0;
    let mut result = u32::MAX;
    while result != PDH_CSTATUS_VALID_DATA {
        result = unsafe { PdhAddEnglishCounterA(query_handle, path, 0, &mut metric_handle) };
        if result != PDH_CSTATUS_VALID_DATA {
            println!("Fehler beim Registrieren von path: '{path_str}', result: {result:X}");
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
    metric_handle
}

pub fn get_pdh_process_data(process_metric_handles: &ProcessMetricHandles) -> Vec<Process> {
    unsafe {
        let mut cpu_itembuffer = [PDH_FMT_COUNTERVALUE_ITEM_A::default(); 1500];
        let mut ram_itembuffer = [PDH_FMT_COUNTERVALUE_ITEM_A::default(); 1500];

        let ram_dwformat = PDH_FMT(PDH_FMT_LARGE.0 | 0x00008000);
        let mut ram_itemcount = 1500;
        let mut ram_lpdwbuffersize = (24 * ram_itemcount) as u32;
        let result = PdhGetFormattedCounterArrayA(
            process_metric_handles.ram_handle,
            ram_dwformat,
            &mut ram_lpdwbuffersize,
            &mut ram_itemcount,
            Some(ram_itembuffer.as_mut_ptr()),
        );

        if WIN32_ERROR(result) != ERROR_SUCCESS {
            println!("read ram array error: {result:X}, itemcount: {ram_itemcount}");
            // panic!();
        }

        // https://tyleo.github.io/sharedlib/doc/winapi/pdh/constant.PDH_FMT_NOSCALE.html
        let cpu_dwformat = PDH_FMT(512 | 0x8000); // double: 512 noscale: 4096, fmt1000: 8192, nocap: 32768
        let mut cpu_itemcount = 1500;
        let mut cpu_lpdwbuffersize = (24 * cpu_itemcount) as u32;
        let result = PdhGetFormattedCounterArrayA(
            process_metric_handles.cpu_handle,
            cpu_dwformat,
            &mut cpu_lpdwbuffersize,
            &mut cpu_itemcount,
            Some(cpu_itembuffer.as_mut_ptr()),
        );
        if WIN32_ERROR(result) != ERROR_SUCCESS {
            println!("read cpu array error: {result:X}, itemcount: {cpu_itemcount}");
            // panic!();
        }

        let cpu_procs = cpu_itembuffer[..cpu_itemcount as usize]
            .iter()
            .map(|p| {
                (
                    p.szName
                        .to_string()
                        .unwrap_or_default()
                        .split(':')
                        .next()
                        .unwrap_or_default()
                        .to_string(),
                    p.FmtValue.Anonymous.doubleValue,
                    1,
                )
            })
            .filter(|(n, _, _)| n != "Idle" && n != "_Total")
            .sorted_unstable_by_key(|p| p.0.clone())
            .group_by(|p| p.0.clone())
            .into_iter()
            .map(|(_name, group)| {
                group
                    .reduce(|acc, p| (acc.0, acc.1 + p.1, acc.2 + p.2))
                    .unwrap()
            })
            .collect_vec();

        let ram_procs = ram_itembuffer[..ram_itemcount as usize]
            .iter()
            .map(|p| {
                (
                    p.szName
                        .to_string()
                        .unwrap_or_default()
                        .split(':')
                        .next()
                        .unwrap_or_default()
                        .to_string(),
                    p.FmtValue.Anonymous.largeValue,
                )
            })
            .filter(|(n, _)| n != "Idle" && n != "_Total")
            .sorted_unstable_by_key(|p| p.0.clone())
            .group_by(|p| p.0.clone())
            .into_iter()
            .map(|(_name, group)| group.reduce(|acc, p| (acc.0, acc.1 + p.1)).unwrap())
            .collect_vec();

        cpu_procs
            .iter()
            .zip(ram_procs.iter())
            .map(|((name, cpu, count), (name2, ram))| {
                if name != name2 {
                    println!("{name} != {name2}")
                };
                Process {
                    name: name.to_string(),
                    cpu: *cpu,
                    memory: *ram,
                    count: *count,
                }
            })
            .collect_vec()
    }
}
