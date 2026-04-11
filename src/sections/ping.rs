use crate::{
    components::section::{add_graph, SectionMetrics},
    step_timing, MyApp,
};
use eframe::egui::{RichText, Ui};
use egui_plot::{Line, PlotPoints};

pub struct Ping;

impl crate::components::section::Section for Ping {
    fn name(&self) -> &'static str {
        "Ping"
    }

    fn render(&self, appdata: &mut MyApp, ui: &mut Ui, layout: SectionMetrics) {
        render_ping_section(ui, layout, appdata);
    }

    fn after_render(&self, appdata: &mut MyApp, _ui: &mut Ui, _response: &eframe::egui::Response) {
        step_timing(appdata, crate::CurrentStep::Ping);
    }
}

pub fn render_ping_section(ui: &mut Ui, layout: SectionMetrics, appdata: &MyApp) {
    let plot_width = layout.content_width();
    let pings = appdata.ping_buffer.read();
    let last_ping = pings.last().copied().unwrap_or_default();
    let max_ping = pings.iter().max().copied().unwrap_or_default();
    let line = Line::new(
        (0..appdata.ping_buffer.capacity())
            .map(|i| [i as f64, { pings[i] as f64 }])
            .collect::<PlotPoints>(),
    );

    let lp_str = if last_ping == 0 {
        "ERR".to_string()
    } else {
        format!("{last_ping:.0} ms")
    };

    ui.label(RichText::new(format!("M: {max_ping:.0}ms, C: {lp_str}")).size(12.0));
    add_graph("ping", ui, vec![line], &[50.0, max_ping as f64], plot_width);
}
