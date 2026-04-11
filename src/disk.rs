use std::{sync::Arc, time::Instant};

use itertools::Itertools;
use lsblk::{BlockDevice, Mount};
use procfs::DiskStat;
use sysinfo::Disk;

use crate::{circlevec::CircleVec, MyApp};

/// A specific Partition
pub struct MyDiskInfo {
    pub displayname: String,
    pub mount_point: String,
    pub blockdevicename: String,
    pub bytes_total: u64,
    pub bytes_free: u64,
    pub bytes_used: u64,
    pub last_update: Instant,
}

#[derive(Debug, Clone)]
pub struct MyBlockDeviceStat {
    pub name: String,

    time_reading: u64,
    time_writing: u64,
    time_discarding: u64,
    time_flushing: u64,

    timestamp: Instant,

    pub io_history: Arc<CircleVec<u64, 100>>,
    updated_this_tick: bool,
}

// TODO: die refresh-funktion sollte auch init machen
pub fn refresh_disks(appdata: &mut MyApp) {
    let now = Instant::now();

    // Only update blks that are required, and only once
    for blk in &mut appdata.blockdevices {
        blk.updated_this_tick = false;
    }

    appdata.raw_disks.refresh(true); // 0.2 ms

    // println!();
    // for ele in appdata.raw_disks.iter() {
    //     dbg!(ele);
    //     dbg!(ele.name());
    //     dbg!(&ele.mount_point());
    //     dbg!(ele.file_system());
    //     println!();
    // }
    // println!();

    let lsblk_blockdevicelist = BlockDevice::list().unwrap(); // < 1 ms

    // for b in &lsblk_blockdevicelist {
    //     println!("{b:?}");
    // }
    // println!();

    let lsblk_mountlist = Mount::list().unwrap().collect_vec(); // < 0.1 ms

    // for m in &lsblk_mountlist {
    //     println!("{m:?}");
    //     println!("{}", m.mountpoint.to_str().unwrap());
    // }
    // println!();

    let procfs_diskstats = procfs::diskstats().unwrap(); // 0.1 ms

    // for disk in &procfs_diskstats {
    //     println!("{disk:?}");
    // }
    // println!();

    // panic!();

    let mut temp_disks = Vec::new();

    for disk in get_filtered_sysinfo_disks(appdata) {
        if let Some(mydisk) =
            MyDiskInfo::resolve_disk_info(disk, &lsblk_mountlist, &lsblk_blockdevicelist)
        {
            temp_disks.push(mydisk);
        }
    }

    // for every disk refresh the fitting blockdevice
    for mydisk in &temp_disks {
        update_and_register_blockstat(
            appdata,
            mydisk,
            &lsblk_blockdevicelist,
            &lsblk_mountlist,
            &procfs_diskstats,
            now,
        );
    }

    appdata.disks.clear();
    appdata.disks.append(&mut temp_disks);

    // Remove all blockdevices and disk infos that are not coupled to active partitions anymore.
    // Disk info list gets rebuild every tick, so it doesn't need to be cleared manually.
    appdata.blockdevices.retain(|blk| blk.updated_this_tick);
}

fn update_and_register_blockstat(
    appdata: &mut MyApp,
    mydisk: &MyDiskInfo,
    lsblk_blockdevicelist: &[BlockDevice],
    _lsblk_mountlist: &[Mount],
    procfs_diskstats: &[DiskStat],
    now: Instant,
) {
    // find blockdevice in appstate, or init it
    if !appdata
        .blockdevices
        .iter()
        .any(|blk| mydisk.blockdevicename == blk.name)
    {
        let blk = lsblk_blockdevicelist
            .iter()
            .find(|blk| blk.name == mydisk.blockdevicename)
            .expect("this blockdevice better exist");
        let diskstat = procfs_diskstats
            .iter()
            .find(|ds| ds.name == blk.disk_name().expect("this better have a diskname"))
            .unwrap();
        println!("New blockdevice: {}", blk.name);
        let b = MyBlockDeviceStat {
            name: blk.name.clone(),
            time_reading: diskstat.time_reading,
            time_writing: diskstat.time_writing,
            time_discarding: diskstat.time_discarding.unwrap_or_default(),
            time_flushing: diskstat.time_flushing.unwrap_or_default(),
            timestamp: now,
            io_history: CircleVec::new(),
            updated_this_tick: true,
        };
        appdata.blockdevices.push(b);
        return;
    }
    let oldblockdevice = appdata
        .blockdevices
        .iter_mut()
        .find(|blk| mydisk.blockdevicename == blk.name)
        .expect("haben wir gerade angelegt");
    if oldblockdevice.updated_this_tick {
        // Dont update blockdevices twice
        return;
    }

    let blk = lsblk_blockdevicelist
        .iter()
        .find(|blk| blk.name == mydisk.blockdevicename)
        .expect("this blockdevice better exist");
    let diskstat = procfs_diskstats
        .iter()
        .find(|ds: &&DiskStat| ds.name == blk.disk_name().expect("this better have a diskname"))
        .unwrap();

    // let read_delta = diskstat.time_reading - oldblockdevice.time_reading;
    // let write_delta = diskstat.time_writing - oldblockdevice.time_writing;
    // let discard_delta =
    //     diskstat.time_discarding.unwrap_or_default() - oldblockdevice.time_discarding;
    // let flush_delta = diskstat.time_flushing.unwrap_or_default() - oldblockdevice.time_flushing;

    // println!("{}", oldblockdevice.name);
    // let total_delta = read_delta + write_delta + discard_delta + flush_delta;

    // let elapsed = now.duration_since(oldblockdevice.timestamp).as_micros() as f32 / 1000.0;

    // See: https://github.com/Slimbook-Team/mission-center/blob/main/src/sys_info_v2/gatherer/src/platform/linux/disk_info.rs
    // Arbitrary math is arbitrary
    // let busy_percent = (total_delta as f32 / (elapsed * 8.0)).min(100.);

    oldblockdevice.timestamp = now;
    oldblockdevice.time_reading = diskstat.time_reading;
    oldblockdevice.time_writing = diskstat.time_writing;
    oldblockdevice.time_discarding = diskstat.time_discarding.unwrap_or_default();
    oldblockdevice.time_flushing = diskstat.time_flushing.unwrap_or_default();

    // get util from iostat:
    if let Ok(list) = appdata.current_disk_util_data.lock() {
        let util = list
            .get(&mydisk.blockdevicename)
            .cloned()
            .unwrap_or_default();

        // println!("{} : {util}", mydisk.blockdevicename);

        oldblockdevice.io_history.add(util as u64);
    } else {
        println!("möp")
    }
    // oldblockdevice.io_history.add(busy_percent as u64);

    oldblockdevice.updated_this_tick = true;
}

impl MyDiskInfo {
    fn resolve_disk_info(
        sysinfo_disk: &Disk,
        mountlist: &Vec<Mount>,
        blockdevicelist: &Vec<BlockDevice>,
    ) -> Option<Self> {
        let mountpoint_name = sysinfo_disk.mount_point().to_str().unwrap();
        // println!("{m}");
        let mount = mountlist
            .iter()
            .find(|mlm| mlm.mountpoint.to_str().unwrap().replace("\\040", " ") == mountpoint_name)
            .unwrap();
        // println!("{m} -> {mount:?}");
        let blockdev;
        if mount.device.contains("/by-uuid/") {
            let uuid = mount.device.replace("/dev/disk/by-uuid/", "");

            blockdev = blockdevicelist
                .iter()
                .find(|bd| &bd.uuid.clone().unwrap_or_default() == &uuid);
        } else {
            blockdev = blockdevicelist
                .iter()
                .find(|bd| bd.fullname.to_str().unwrap() == mount.device);
        }
        if let Some(bd) = blockdev {
            // println!("{m} -> {mount:?} -> {blockdev:?} -> {}", bd.name);

            let drive_letter = sysinfo_disk
                .mount_point()
                .to_str()
                .unwrap()
                .replace('\\', "");

            let drive_letter = drive_letter.split("/").last().unwrap();

            let displayname = if drive_letter.trim().is_empty() {
                "/".to_string()
            } else {
                drive_letter.to_string()
            };
            let mut blockdevicename = bd.name.clone();
            let diskseq = bd.diskseq.clone().unwrap_or_default();
            if diskseq.contains("-") {
                let parent_seq = diskseq.split('-').next().unwrap().to_owned();
                if let Some(parent_bd) = blockdevicelist
                    .iter()
                    .find(|bd| bd.diskseq == Some(parent_seq.clone()))
                {
                    blockdevicename = parent_bd.name.clone();
                }
            }

            Some(MyDiskInfo {
                displayname,
                mount_point: mountpoint_name.to_string(),
                blockdevicename,
                last_update: Instant::now(),
                bytes_free: sysinfo_disk.available_space(),
                bytes_total: sysinfo_disk.total_space(),
                bytes_used: sysinfo_disk.total_space() - sysinfo_disk.available_space(),
            })
        } else {
            None
        }
    }

    // FIX
    // fn refresh_disk_io_time(
    //     &mut self,
    //     _blockdevices: &Vec<BlockDevice>,
    //     _mountlist: &Vec<Mount>,
    //     diskstats: &Vec<DiskStat>,
    // ) {
    //     let blockdevicestat = diskstats
    //         .iter()
    //         .find(|ds| ds.name == self.blockdevicename)
    //         .unwrap();
    //     if self.last_io_time == 0 {
    //         self.last_io_time = blockdevicestat.time_in_progress;
    //         self.last_update = Instant::now();
    //         return;
    //     }

    //     let diff = blockdevicestat.time_in_progress - self.last_io_time;

    //     let timediff_ms = self.last_update.elapsed().as_millis() as u64;
    //     let loadpercent = diff * 100 / timediff_ms; // x100 for %

    //     // println!(
    //     //     "{}: {} -> Diff: {}ms, timediff: {timediff_ms}ms, Load: {}%",
    //     //     self.mount_point, self.last_io_time, diff, loadpercent
    //     // );

    //     self.io_history.add(loadpercent);
    //     self.last_io_time = blockdevicestat.time_in_progress;
    //     self.last_update = Instant::now();
    //     // println!();
    // }

    // See: https://github.com/Slimbook-Team/mission-center/blob/main/src/sys_info_v2/gatherer/src/platform/linux/disk_info.rs
    // fn disk_io_percent_from_raw(
    //     &mut self,
    //     _blockdevices: &Vec<BlockDevice>,
    //     _mountlist: &Vec<Mount>,
    //     diskstats: &Vec<DiskStat>,
    // ) {
    // let blockdevicestat = diskstats
    //     .iter()
    //     .find(|ds| ds.name == self.blockdevicename)
    //     .unwrap();
    // if self.last_io_time == 0 {
    //     self.last_io_time = blockdevicestat.time_in_progress;
    //     self.last_update = Instant::now();
    //     return;
    // }

    // let read_ticks_weighted_ms_prev =
    //     if read_ticks_weighted_ms < disk_stat.read_ticks_weighted_ms {
    //         read_ticks_weighted_ms
    //     } else {
    //         disk_stat.read_ticks_weighted_ms
    //     };

    // let write_ticks_weighted_ms_prev =
    //     if write_ticks_weighted_ms < disk_stat.write_ticks_weighted_ms {
    //         write_ticks_weighted_ms
    //     } else {
    //         disk_stat.write_ticks_weighted_ms
    //     };

    // let discard_ticks_weighted_ms_prev =
    //     if discard_ticks_weighted_ms < disk_stat.discard_ticks_weighted_ms {
    //         discard_ticks_weighted_ms
    //     } else {
    //         disk_stat.discard_ticks_weighted_ms
    //     };

    // let flush_ticks_weighted_ms_prev =
    //     if flush_ticks_weighted_ms < disk_stat.flush_ticks_weighted_ms {
    //         flush_ticks_weighted_ms
    //     } else {
    //         disk_stat.flush_ticks_weighted_ms
    //     };

    // let elapsed = disk_stat.read_time_ms.elapsed().as_secs_f32();

    // let delta_read_ticks_weighted_ms = read_ticks_weighted_ms - read_ticks_weighted_ms_prev;
    // let delta_write_ticks_weighted_ms = write_ticks_weighted_ms - write_ticks_weighted_ms_prev;
    // let delta_discard_ticks_weighted_ms =
    //     discard_ticks_weighted_ms - discard_ticks_weighted_ms_prev;
    // let delta_flush_ticks_weighted_ms = flush_ticks_weighted_ms - flush_ticks_weighted_ms_prev;
    // let delta_ticks_weighted_ms = delta_read_ticks_weighted_ms
    //     + delta_write_ticks_weighted_ms
    //     + delta_discard_ticks_weighted_ms
    //     + delta_flush_ticks_weighted_ms;

    // // Arbitrary math is arbitrary
    // let busy_percent: f32 = (delta_ticks_weighted_ms as f32 / (elapsed * 8.0)).min(100.);
    // }
}

fn get_filtered_sysinfo_disks(appdata: &MyApp) -> Vec<&Disk> {
    appdata
        .raw_disks
        .iter()
        .sorted_by_key(|d| d.mount_point())
        .filter(|d| d.file_system() != "vfat")
        .group_by(|d| d.name())
        .into_iter()
        .map(|(_, g)| g.into_iter().next().unwrap())
        .collect_vec()
}
