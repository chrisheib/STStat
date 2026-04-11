use crate::{step_timing, MyApp};
use eframe::egui::Ui;

pub struct Battery;

impl crate::components::section::Section for Battery {
    fn name(&self) -> &'static str {
        "Battery"
    }

    fn is_visible(&self, appdata: &MyApp) -> bool {
        appdata.battery_enabled
    }

    fn render(
        &self,
        appdata: &mut MyApp,
        _ui: &mut Ui,
        _layout: crate::components::section::SectionMetrics,
    ) {
        // let level = appdata.battery_level_buffer.read();
        // let level_line = Line::new(
        //     (0..appdata.battery_level_buffer.capacity())
        //         .map(|i| {
        //             [i as f64, {
        //                 (if level[i] == 0.0 { 100.0 } else { level[i] }) - 50.0
        //             }]
        //         })
        //         .collect::<PlotPoints>(),
        // );
        // let charge = appdata.battery_change_buffer.read();
        // let charge_line = Line::new(
        //     (0..appdata.battery_change_buffer.capacity())
        //         .map(|i| [i as f64, { charge[i] * 25.0 }])
        //         .collect::<PlotPoints>(),
        // );

        // add_graph("battery", ui, vec![level_line, charge_line], &[-50.0, 50.0]);
        step_timing(appdata, crate::CurrentStep::Ping);
    }
}
