use crate::{
    bytes_format::format_bytes,
    components::section::{add_graph, section_table, SectionMetrics},
    step_timing, MyApp,
};
use eframe::egui::{Label, RichText, Ui};
use egui_plot::{Line, PlotPoints};

pub struct Network;

impl crate::components::section::Section for Network {
    fn name(&self) -> &'static str {
        "Networks"
    }

    fn render(&self, appdata: &mut MyApp, ui: &mut Ui, layout: SectionMetrics) {
        render_network_section(ui, layout, appdata);
    }

    fn after_render(&self, appdata: &mut MyApp, _ui: &mut Ui, _response: &eframe::egui::Response) {
        step_timing(appdata, crate::CurrentStep::Network);
    }
}

pub fn render_network_section(ui: &mut Ui, layout: SectionMetrics, appdata: &MyApp) {
    let content_width = layout.content_width();
    for net in &appdata.networks {
        ui.push_id(format!("network graph {}", net.interface), |ui| {
            let table_col_width = content_width * 0.5;
            let table = section_table(ui, &[table_col_width, table_col_width], true);
            table.header(10.0, |mut header| {
                header.col(|ui| {
                    ui.add(
                        Label::new(
                            RichText::new(format!(
                                "⬆ {}",
                                format_bytes(*net.history_up.read().last().unwrap())
                            ))
                            .size(12.0),
                        )
                        .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                    );
                });
                header.col(|ui| {
                    ui.add(
                        Label::new(
                            RichText::new(format!(
                                "⬇ {}",
                                format_bytes(*net.history_down.read().last().unwrap())
                            ))
                            .size(12.0),
                        )
                        .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                    );
                });
            });
        });

        ui.add_space(3.0);

        let up = net.history_up.read();

        let up_line = Line::new(
            (0..net.history_up.capacity())
                .map(|i| [i as f64, { up[i] }])
                .collect::<PlotPoints>(),
        );

        let down = net.history_down.read();

        let down_line = Line::new(
            (0..net.history_down.capacity())
                .map(|i| [i as f64, { down[i] }])
                .collect::<PlotPoints>(),
        );

        let max_down = down
            .iter()
            .reduce(|acc, v| if acc > v { acc } else { v })
            .copied()
            .unwrap_or_default();
        let max_up = up
            .iter()
            .reduce(|acc, v| if acc > v { acc } else { v })
            .copied()
            .unwrap_or_default();

        add_graph(
            &format!("network-{}", net.interface),
            ui,
            vec![down_line, up_line],
            &[14.0 * 1024.0 * 1024.0, max_down, max_up],
            content_width,
        );
    }
}
