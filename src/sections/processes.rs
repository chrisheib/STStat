use crate::{
    bytes_format::format_bytes,
    components::section::{section_table, SectionMetrics, SIDEBAR_COMPACT_TABLE_ROW_HEIGHT},
    step_timing, MyApp,
};
use eframe::egui::{Label, Layout, RichText, Sense, Ui};
use eframe::emath::Align::Max;
use itertools::Itertools;
use std::fmt;
use sysinfo::Pid;

#[derive(PartialEq, Eq)]
pub enum ProcessTableDisplayMode {
    All,
    Cpu,
    Ram,
}

#[derive(Clone, Debug)]
pub struct Process {
    pub cpu: f32,
    pub memory: u64,
    pub name: String,
    pub pid: Pid,
    pub parent: Option<Pid>,
}

// Implement `Display` for `MinMax`.
impl fmt::Display for Process {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Use `self.number` to refer to each positional data point.
        write!(
            f,
            "{}, {}, {}, {}, {:?}",
            self.pid, self.name, self.cpu, self.memory, self.parent
        )
    }
}

pub struct Processes;

impl crate::components::section::Section for Processes {
    fn name(&self) -> &'static str {
        "Processes"
    }

    fn render(&self, appdata: &mut MyApp, ui: &mut Ui, layout: SectionMetrics) {
        render_processes_section(ui, layout, appdata);
    }
}

pub fn render_processes_section(ui: &mut Ui, layout: SectionMetrics, appdata: &mut MyApp) {
    let content_width = layout.content_width();
    let p = appdata
        .system_status
        .processes()
        .iter()
        .filter(|(_, p)| p.thread_kind().is_none())
        .map(|(_, a)| Process {
            cpu: a.cpu_usage(),
            memory: a.memory(),
            name: a.name().to_str().unwrap_or_default().to_string(),
            pid: a.pid(),
            parent: a.parent(),
        })
        .collect_vec();

    // By CPU
    let mut cpu_p = p.clone();
    cpu_p.sort_unstable_by(|a, b| b.cpu.total_cmp(&a.cpu));
    let cpu_count = appdata.system_status.cpus().len();
    add_process_table(
        ui,
        5,
        &cpu_p,
        "Proc CPU",
        ProcessTableDisplayMode::Cpu,
        cpu_count,
        content_width,
    );
    step_timing(appdata, crate::CurrentStep::ProcCPU);
    ui.add_space(4.0);

    // By Memory
    let mut mem_p = p.clone();
    clean_memory_process_list(&mut mem_p);
    add_process_table(
        ui,
        5,
        &mem_p,
        "Proc Ram",
        ProcessTableDisplayMode::Ram,
        cpu_count,
        content_width,
    );

    step_timing(appdata, crate::CurrentStep::ProcRAM);
}

pub fn clean_memory_process_list(proc: &mut [Process]) {
    proc.sort_unstable_by_key(|a| a.pid);
    // let mut i = proc.len() - 1;
    // while i > 0 {
    //     if let Some(ppid) = proc[i].parent {
    //         if let Some(parent) = proc.iter().find(|p| p.pid == ppid) {
    //             if parent.memory == proc[i].memory {
    //                 // If the parent has the same memory usage, remove the child
    //                 println!(
    //                     "Removing {} ({}), parent: {}",
    //                     proc[i].name, proc[i].pid, parent.name
    //                 );
    //                 proc.remove(i);
    //             }
    //         }
    //     }
    //     i -= 1;
    // }
    // panic!();
    proc.sort_unstable_by(|a, b| b.memory.cmp(&a.memory));
}

pub fn add_process_table(
    ui: &mut Ui,
    len: usize,
    p: &[Process],
    name: &str,
    display_mode: ProcessTableDisplayMode,
    core_count: usize,
    table_width: f32,
) {
    let mut clicked = false;
    ui.push_id(name, |ui| {
        let mut columns = Vec::with_capacity(3);
        columns.push(
            table_width
                * if display_mode == ProcessTableDisplayMode::All {
                    0.4
                } else {
                    0.7
                },
        );
        if display_mode == ProcessTableDisplayMode::All
            || display_mode == ProcessTableDisplayMode::Ram
        {
            columns.push(table_width * 0.3);
        };
        if display_mode == ProcessTableDisplayMode::All
            || display_mode == ProcessTableDisplayMode::Cpu
        {
            columns.push(table_width * 0.3);
        };

        let table = section_table(ui, &columns, true);
        let table = table.header(10.0, |mut header| {
            header.col(|ui| {
                clicked = clicked
                    || ui
                        .add(
                            Label::new(RichText::new(name).small())
                                .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                        )
                        .interact(Sense::click())
                        .double_clicked();
            });
            if display_mode == ProcessTableDisplayMode::All
                || display_mode == ProcessTableDisplayMode::Ram
            {
                header.col(|ui| {
                    ui.with_layout(Layout::top_down_justified(Max), |ui| {
                        clicked = clicked
                            || ui
                                .add(
                                    Label::new(RichText::new("RAM").small())
                                        .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                                )
                                .interact(Sense::click())
                                .double_clicked();
                    });
                });
            }
            if display_mode == ProcessTableDisplayMode::All
                || display_mode == ProcessTableDisplayMode::Cpu
            {
                header.col(|ui| {
                    ui.with_layout(Layout::top_down_justified(Max), |ui| {
                        clicked = clicked
                            || ui
                                .add(
                                    Label::new(RichText::new("CPU").small())
                                        .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                                )
                                .interact(Sense::click())
                                .double_clicked();
                    });
                });
            }
        });
        table.body(|body| {
            body.rows(SIDEBAR_COMPACT_TABLE_ROW_HEIGHT, len, |mut row| {
                let row_index = row.index();
                if row_index < p.len() {
                    let p = &p[row_index];
                    row.col(|ui| {
                        clicked = clicked
                            || ui
                                .add(
                                    Label::new(RichText::new(p.name.to_string()).small().strong())
                                        .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                                )
                                .interact(Sense::click())
                                .double_clicked();
                    });
                    if display_mode == ProcessTableDisplayMode::All
                        || display_mode == ProcessTableDisplayMode::Ram
                    {
                        row.col(|ui| {
                            ui.with_layout(Layout::top_down_justified(Max), |ui| {
                                clicked = clicked
                                    || ui
                                        .add(
                                            Label::new(
                                                RichText::new(format_bytes(p.memory as f64))
                                                    .small()
                                                    .strong(),
                                            )
                                            .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                                        )
                                        .interact(Sense::click())
                                        .double_clicked()
                            });
                        });
                    }
                    if display_mode == ProcessTableDisplayMode::All
                        || display_mode == ProcessTableDisplayMode::Cpu
                    {
                        row.col(|ui| {
                            ui.with_layout(Layout::top_down_justified(Max), |ui| {
                                clicked = clicked
                                    || ui
                                        .add(
                                            Label::new(
                                                RichText::new(format!(
                                                    "{:.1}%",
                                                    p.cpu / core_count as f32
                                                ))
                                                .small()
                                                .strong(),
                                            )
                                            .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                                        )
                                        .interact(Sense::click())
                                        .double_clicked();
                            });
                        });
                    }
                }
            });
        });
    });
    ui.add_space(1.0);

    // if clicked {
    //     match Command::new("powershell")
    //         .args(["start", "taskmgr", "-v runAs"])
    //         .spawn()
    //     {
    //         Ok(_c) => println!("Starting Task Manager"),
    //         Err(e) => println!("{e}"),
    //     };
    // }
}
