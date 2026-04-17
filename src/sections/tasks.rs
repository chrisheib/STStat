use crate::{
    components::section::{section_table, SectionMetrics, SIDEBAR_COMPACT_TABLE_ROW_HEIGHT},
    tasks::{tasks_browser_url, tasks_status_line},
    MyApp,
};
use eframe::{
    egui::{CursorIcon, Label, Layout, RichText, Sense, Ui},
    emath::Align::Center,
};

pub struct Tasks;

impl crate::components::section::Section for Tasks {
    fn name(&self) -> &'static str {
        "Tasks"
    }

    fn is_visible(&self, appdata: &MyApp) -> bool {
        let settings = appdata.settings.lock().current_settings.clone();
        settings.tasks_enabled
    }

    fn render(&self, appdata: &mut MyApp, ui: &mut Ui, layout: SectionMetrics) {
        let settings = appdata.settings.lock().current_settings.clone();
        render_tasks_section(ui, layout, appdata, &settings);
    }

    fn after_render(&self, appdata: &mut MyApp, ui: &mut Ui, response: &eframe::egui::Response) {
        let settings = appdata.settings.lock().current_settings.clone();
        let tasks_page_url = tasks_browser_url(&settings);
        let response = ui
            .interact(
                response.rect,
                ui.id().with("tasks_block_link"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        if response.clicked() {
            let _ = webbrowser::open(tasks_page_url);
        }
    }
}

/// Renders task items from Google Tasks to provide a compact, actionable queue.
pub fn render_tasks_section(
    ui: &mut Ui,
    layout: SectionMetrics,
    appdata: &MyApp,
    settings: &crate::settings::InnerSettings,
) {
    let content_width = layout.content_width();
    ui.set_min_width(content_width);
    ui.set_max_width(content_width);

    let tasks = appdata.tasks_state.lock().items.clone();
    if tasks.is_empty() {
        ui.label(RichText::new("No upcoming tasks").small());
        ui.with_layout(Layout::right_to_left(Center), |ui| {
            ui.label(
                RichText::new(tasks_status_line(&appdata.tasks_state))
                    .small()
                    .weak(),
            );
        });
        return;
    }

    ui.scope(|ui| {
        let row_count = tasks.len().min(settings.tasks_max_items);
        let table = section_table(ui, &[content_width], true).id_salt("tasks_table");

        table.body(|body| {
            body.rows(SIDEBAR_COMPACT_TABLE_ROW_HEIGHT, row_count, |mut row| {
                if let Some(task) = tasks.get(row.index()) {
                    let label = task.title.clone();

                    row.col(|ui| {
                        ui.add(
                            Label::new(RichText::new(label).small().strong())
                                .wrap_mode(eframe::egui::TextWrapMode::Truncate),
                        );
                    });
                }
            });
        });
    });

    ui.add_space(-1.0);

    ui.with_layout(Layout::right_to_left(Center), |ui| {
        ui.label(
            RichText::new(tasks_status_line(&appdata.tasks_state))
                .small()
                .weak(),
        );
    });
}
