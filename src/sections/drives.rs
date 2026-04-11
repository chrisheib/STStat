use crate::{
    bytes_format::format_bytes,
    color::auto_color_dark,
    components::{
        edgy_progress::EdgyProgressBar,
        section::{add_graph, layout_debug_enabled, section_table, SectionMetrics},
    },
    MyApp,
};
use eframe::{
    egui::{Label, Layout, RichText, TextStyle, Ui, UiBuilder},
    emath::Align::Max,
    epaint::Color32,
};
use egui_plot::{Line, PlotPoints};

/// Left-ellipsizes a drive label based on measured text width.
#[derive(Debug)]
pub struct DriveNameCutoutDebug {
    pub full_text: String,
    pub cutout_text: String,
    pub full_width: f32,
    pub max_width: f32,
    pub cutout_width: f32,
    pub ellipsis_width: f32,
    pub total_chars: usize,
    pub kept_chars: usize,
}

pub fn left_ellipsize_drive_name(
    ui: &Ui,
    name: &str,
    max_width: f32,
) -> (String, DriveNameCutoutDebug) {
    let ellipsis = "...";
    let measure_width = |text: &str| {
        eframe::egui::WidgetText::from(RichText::new(text).small().strong())
            .into_galley(
                ui,
                Some(eframe::egui::TextWrapMode::Extend),
                f32::INFINITY,
                TextStyle::Small,
            )
            .size()
            .x
    };
    let max_width = (max_width - 1.0).max(0.0);
    let full_width = measure_width(name);
    let ellipsis_width = measure_width(ellipsis);
    let total_chars = name.chars().count();

    let mut dbg = DriveNameCutoutDebug {
        full_text: name.to_string(),
        cutout_text: String::new(),
        full_width,
        max_width,
        cutout_width: 0.0,
        ellipsis_width,
        total_chars,
        kept_chars: 0,
    };

    if max_width <= 0.0 {
        return (String::new(), dbg);
    }
    if full_width <= max_width {
        dbg.cutout_text = name.to_string();
        dbg.cutout_width = full_width;
        dbg.kept_chars = total_chars;
        return (name.to_string(), dbg);
    }

    if ellipsis_width > max_width {
        return (String::new(), dbg);
    }

    let chars: Vec<char> = name.chars().collect();
    let total = chars.len();
    for keep in (1..=total).rev() {
        let suffix: String = chars[total - keep..].iter().collect();
        let candidate = format!("{ellipsis}{suffix}");
        let candidate_width = measure_width(&candidate);
        if candidate_width <= max_width {
            dbg.cutout_text = candidate.clone();
            dbg.cutout_width = candidate_width;
            dbg.kept_chars = keep;
            return (candidate, dbg);
        }
    }

    dbg.cutout_text = ellipsis.to_string();
    dbg.cutout_width = ellipsis_width;
    (ellipsis.to_string(), dbg)
}

pub struct Drives;

impl crate::components::section::Section for Drives {
    fn name(&self) -> &'static str {
        "Drives"
    }

    fn render(&self, appdata: &mut MyApp, ui: &mut Ui, layout: SectionMetrics) {
        render_drives_section(ui, layout, appdata);
    }
}

pub fn render_drives_section(ui: &mut Ui, layout: SectionMetrics, appdata: &mut MyApp) {
    let debug = layout_debug_enabled(appdata);
    let table_width = layout.full_width();
    let col_name = table_width * 0.45;
    let col_usage = table_width * 0.2;
    let col_free = table_width * 0.35;
    let row_height = 11.0;

    if debug {
        ui.label(
            RichText::new(format!(
                "DBG d col n:{col_name:.1} u:{col_usage:.1} f:{col_free:.1}"
            ))
            .small()
            .weak(),
        );
    }

    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let mut first_drive_debug: Option<DriveNameCutoutDebug> = None;

        let table = section_table(ui, &[col_name, col_usage, col_free], true);

        table.body(|body| {
            body.rows(row_height, appdata.disks.len(), |mut row| {
                let row_index = row.index();
                if let Some(d) = appdata.disks.get(row_index) {
                    let (blkindex, blk) = appdata
                        .blockdevices
                        .iter()
                        .enumerate()
                        .find(|(_, blk)| blk.name == d.blockdevicename)
                        .unwrap();

                    let history = blk.io_history.read();
                    let usage = history.last().copied().unwrap_or_default();

                    row.col(|ui| {
                        ui.add_space(1.0);
                        let (name_text, name_dbg) = left_ellipsize_drive_name(
                            ui,
                            d.displayname.as_str(),
                            ui.available_width().max(1.0),
                        );
                        if debug && row_index == 0 {
                            first_drive_debug = Some(name_dbg);
                        }
                        ui.add(
                            Label::new(RichText::new(name_text).small().strong())
                                .wrap_mode(eframe::egui::TextWrapMode::Extend),
                        );
                    });

                    row.col(|ui| {
                        let mut label_rect = ui.max_rect();
                        label_rect.max.x -= 2.0;
                        ui.scope_builder(UiBuilder::new().max_rect(label_rect), |ui| {
                            ui.with_layout(Layout::top_down_justified(Max), |ui| {
                                ui.add_space(1.0);
                                ui.add(
                                    Label::new(
                                        RichText::new(format!("{usage:.0}%")).small().strong(),
                                    )
                                    .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                                );
                            });
                        });
                    });

                    row.col(|ui| {
                        let free_cell_width = ui.available_width().max(1.0);
                        ui.add(
                            EdgyProgressBar::new(d.bytes_used as f32 / d.bytes_total as f32)
                                .desired_width(free_cell_width)
                                .desired_height(12.0)
                                .text(
                                    RichText::new(format_bytes(d.bytes_free as f64))
                                        .small()
                                        .strong()
                                        .color(Color32::from_white_alpha(80)),
                                )
                                .text_align_right(true)
                                .fill(auto_color_dark(blkindex as i32)),
                        );
                    });
                }
            });
        });

        if debug {
            if let Some(info) = first_drive_debug {
                ui.label(
                    RichText::new(format!(
                        "DBG d0 full:'{}' cut:'{}' w_full:{:.1} w_max:{:.1} w_cut:{:.1} w_dots:{:.1} chars:{} keep:{}",
                        info.full_text,
                        info.cutout_text,
                        info.full_width,
                        info.max_width,
                        info.cutout_width,
                        info.ellipsis_width,
                        info.total_chars,
                        info.kept_chars,
                    ))
                    .small()
                    .weak(),
                );
            }
        }
    });
    ui.add_space(1.0);
    ui.spacing();

    let mut lines = Vec::new();
    for d in &appdata.blockdevices {
        let values = d.io_history.read();
        lines.push(Line::new(
            (0..d.io_history.capacity())
                .map(|i| [i as f64, { values[i] as f64 }])
                .collect::<PlotPoints>(),
        ));
    }

    add_graph("disk", ui, lines, &[100.5], layout.content_width());
}
