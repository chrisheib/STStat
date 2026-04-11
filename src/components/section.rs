use eframe::egui::{vec2, Layout, Separator, Ui, UiBuilder};
use egui_extras::{Column, TableBuilder};
use egui_plot::{GridInput, GridMark, Line, Plot};

use crate::MyApp;

pub const SIDEBAR_SECTION_SIDE_MARGIN: f32 = 1.0;
pub const SIDEBAR_SECTION_GRID_SPACING: f32 = 2.0;
pub const SIDEBAR_SECTION_RIGHT_GUARD: f32 = 0.0;
pub const SIDEBAR_COMPACT_TABLE_ROW_HEIGHT: f32 = 9.0;

/// Returns whether the sidebar layout debug overlay should be shown.
pub fn layout_debug_enabled(appdata: &MyApp) -> bool {
    appdata
        .settings
        .lock()
        .current_settings
        .layout_debug_overlay
}

#[derive(Clone, Copy)]
pub struct SectionMetrics {
    width: f32,
    half_width: f32,
}

pub type SectionLayout = SectionMetrics;

impl SectionMetrics {
    /// Full usable section width (after wrapper margins).
    pub fn full_width(self) -> f32 {
        self.width.max(1.0)
    }

    /// Canonical content lane used by tables/plots to avoid right-edge clipping.
    pub fn content_width(self) -> f32 {
        (self.width - 1.0).max(1.0)
    }

    /// Precomputed half-width lane for two-column progress rows.
    pub fn half_lane(self) -> f32 {
        self.half_width
    }
}

/// Computes canonical width metrics for any sidebar section body.
/// This is the single source of truth for section width and half-width math.
pub fn section_metrics_from_ui(ui: &Ui) -> SectionMetrics {
    let inner_width = (ui.available_width() - SIDEBAR_SECTION_RIGHT_GUARD).max(1.0);
    SectionMetrics {
        width: inner_width,
        half_width: ((inner_width - SIDEBAR_SECTION_GRID_SPACING) / 2.0).max(1.0),
    }
}

/// Shared sidebar section container that owns heading, body lane, and separator behavior.
pub struct SectionWidget<'a> {
    title: &'a str,
    show_separator: bool,
}

impl<'a> SectionWidget<'a> {
    /// Creates a section widget with a centered title and trailing separator.
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            show_separator: true,
        }
    }

    /// Controls whether a section renders a trailing separator.
    #[allow(dead_code)]
    pub fn with_separator(mut self, show_separator: bool) -> Self {
        self.show_separator = show_separator;
        self
    }

    /// Renders a section body using canonical section metrics.
    pub fn render(
        self,
        ui: &mut Ui,
        body: impl FnOnce(&mut Ui, SectionMetrics),
    ) -> eframe::egui::Response {
        ui.scope(|ui| {
            let outer_width = ui.available_width().max(1.0);
            ui.allocate_ui(vec2(outer_width, 0.0), |ui| {
                let inner_rect = ui
                    .max_rect()
                    .shrink2(vec2(SIDEBAR_SECTION_SIDE_MARGIN, 0.0));
                ui.scope_builder(UiBuilder::new().max_rect(inner_rect), |ui| {
                    let metrics = section_metrics_from_ui(ui);

                    ui.allocate_ui_with_layout(
                        vec2(metrics.width, 0.0),
                        Layout::top_down_justified(eframe::emath::Align::Center),
                        |ui| {
                            ui.label(self.title);
                        },
                    );

                    ui.allocate_ui(vec2(metrics.width, 0.0), |ui| {
                        body(ui, metrics);
                        if self.show_separator {
                            ui.add(Separator::default().spacing(1.0));
                        }
                    });
                });
            });
        })
        .response
    }
}

/// Renders a sidebar block with consistent 2 px side margins and centered section title.
pub fn show_sidebar_block(
    ui: &mut Ui,
    title: &str,
    body: impl FnOnce(&mut Ui, SectionLayout),
) -> eframe::egui::Response {
    SectionWidget::new(title).render(ui, body)
}

/// Builds a section table with canonical horizontal spacing and exact-width columns.
pub fn section_table<'a>(ui: &'a mut Ui, columns: &[f32], striped: bool) -> TableBuilder<'a> {
    ui.spacing_mut().item_spacing.x = 0.0;

    let mut table = TableBuilder::new(ui).striped(striped);
    for width in columns {
        table = table.column(Column::exact((*width).max(1.0)));
    }
    table
}

/// Builds a section plot with shared styling, interaction, and grid configuration.
pub fn section_plot<'a>(id: &'a str, desired_size: eframe::egui::Vec2) -> Plot<'a> {
    Plot::new(id)
        .show_axes([false, false])
        .x_grid_spacer(|input| proportional_grid_marks(input, 10))
        .y_grid_spacer(|input| proportional_grid_marks(input, 5))
        .grid_spacing(eframe::egui::Rangef::new(2.0, 300.0))
        .label_formatter(|_, _| "".to_string())
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
        .show_x(false)
        .show_y(false)
        .x_axis_formatter(|_, _| String::new())
        .y_axis_formatter(|_, _| String::new())
        .min_size(desired_size)
        .set_margin_fraction((0.0, 0.0).into())
        .width(desired_size.x)
        .height(desired_size.y)
}

pub fn add_graph(id: &str, ui: &mut Ui, line: Vec<Line>, max_y: &[f64], width: f32) {
    let desired_size = vec2((width + 1.0).max(1.0), 30.0);

    let mut p = section_plot(id, desired_size).include_y(0.0);

    for y in max_y {
        p = p.include_y(*y);
    }

    p.show(ui, |plot_ui: &mut egui_plot::PlotUi| {
        for l in line {
            plot_ui.line(l)
        }
    });
}

/// Generates evenly spaced grid marks over the visible range.
/// Skips marks at 0 and at max bound to avoid edge artifacts.
pub fn proportional_grid_marks(input: GridInput, divisions: usize) -> Vec<GridMark> {
    if divisions == 0 {
        return Vec::new();
    }

    let (min, max) = input.bounds;
    let range = max - min;
    if !range.is_finite() || range <= f64::EPSILON {
        return Vec::new();
    }

    let step = range / divisions as f64;
    let eps = step * 0.001;
    let mut marks = Vec::with_capacity(divisions.saturating_sub(1));

    for i in 1..divisions {
        let value = min + step * i as f64;
        if (value - 0.0).abs() <= eps || (value - max).abs() <= eps {
            continue;
        }
        marks.push(GridMark {
            value,
            step_size: step,
        });
    }

    marks
}

/// Trait defining the common interface for all sidebar sections.
/// Each section implements this to describe how it renders and updates.
pub trait Section: Send + Sync {
    /// Display name of this section (shown as heading).
    fn name(&self) -> &'static str;

    /// Controls whether the section should be rendered in the current frame.
    fn is_visible(&self, _appdata: &MyApp) -> bool {
        true
    }

    /// Renders section body content inside the shared section container.
    fn render(&self, appdata: &mut MyApp, ui: &mut Ui, layout: SectionMetrics);

    /// Hook invoked after the section block has been rendered.
    fn after_render(&self, _appdata: &mut MyApp, _ui: &mut Ui, _response: &eframe::egui::Response) {
    }

    /// Renders a full sidebar section using the shared section container and hooks.
    fn show(&self, appdata: &mut MyApp, ui: &mut Ui) {
        if !self.is_visible(appdata) {
            return;
        }

        let response = show_sidebar_block(ui, self.name(), |ui, layout| {
            self.render(appdata, ui, layout);
        });
        self.after_render(appdata, ui, &response);
    }
}
