use eframe::egui::*;

#[allow(dead_code)]
enum EdgyProgressBarText {
    Custom(WidgetText),
    Percentage,
}

/// A simple progress bar.
///
/// See also: [`crate::Spinner`].
#[must_use = "You should put this widget in an ui with `ui.add(widget);`"]
pub struct EdgyProgressBar {
    progress: f32,
    desired_width: Option<f32>,
    desired_height: Option<f32>,
    text: Option<EdgyProgressBarText>,
    fill: Option<Color32>,
    animate: bool,
}

impl EdgyProgressBar {
    /// Progress in the `[0, 1]` range, where `1` means "completed".
    pub fn new(progress: f32) -> Self {
        Self {
            progress: progress.clamp(0.0, 1.0),
            desired_width: None,
            desired_height: None,
            text: None,
            fill: None,
            animate: false,
        }
    }

    /// The desired width of the bar. Will use all horizontal space if not set.
    pub fn desired_width(mut self, desired_width: f32) -> Self {
        self.desired_width = Some(desired_width);
        self
    }

    /// The desired height of the bar. Will use ui.spacing().interact_size.y if not set.
    #[allow(dead_code)]
    pub fn desired_height(mut self, desired_height: f32) -> Self {
        self.desired_height = Some(desired_height);
        self
    }

    /// The fill color of the bar.
    #[allow(dead_code)]
    pub fn fill(mut self, color: Color32) -> Self {
        self.fill = Some(color);
        self
    }

    /// A custom text to display on the progress bar.
    pub fn text(mut self, text: impl Into<WidgetText>) -> Self {
        self.text = Some(EdgyProgressBarText::Custom(text.into()));
        self
    }

    /// Show the progress in percent on the progress bar.
    #[allow(dead_code)]
    pub fn show_percentage(mut self) -> Self {
        self.text = Some(EdgyProgressBarText::Percentage);
        self
    }

    /// Whether to display a loading animation when progress `< 1`.
    /// Note that this will cause the UI to be redrawn.
    /// Defaults to `false`.
    #[allow(dead_code)]
    pub fn animate(mut self, animate: bool) -> Self {
        self.animate = animate;
        self
    }
}

impl Widget for EdgyProgressBar {
    fn ui(self, ui: &mut Ui) -> Response {
        let EdgyProgressBar {
            progress,
            desired_width,
            desired_height,
            text,
            fill,
            animate,
        } = self;

        // let animate = animate && progress < 1.0;

        let desired_width =
            desired_width.unwrap_or_else(|| ui.available_size_before_wrap().x.at_least(96.0));
        let height = desired_height.unwrap_or(ui.spacing().interact_size.y);
        let (outer_rect, response) =
            ui.allocate_exact_size(vec2(desired_width, height), Sense::hover());

        if ui.is_rect_visible(response.rect) {
            let visuals = ui.style().visuals.clone();
            ui.painter().rect(
                outer_rect,
                Rounding::none(),
                visuals.extreme_bg_color,
                Stroke::NONE,
            );
            let inner_rect = Rect::from_min_size(
                outer_rect.min,
                vec2(outer_rect.width() * progress, outer_rect.height()),
            );

            let (dark, bright) = (0.7, 1.0);
            let color_factor = if animate {
                let time = ui.input(|i| i.time);
                lerp(dark..=bright, time.cos().abs())
            } else {
                bright
            };

            ui.painter().rect(
                inner_rect,
                Rounding::none(),
                Color32::from(
                    Rgba::from(fill.unwrap_or(visuals.selection.bg_fill)) * color_factor as f32,
                ),
                Stroke::NONE,
            );

            if let Some(text_kind) = text {
                let text = match text_kind {
                    EdgyProgressBarText::Custom(text) => text,
                    EdgyProgressBarText::Percentage => WidgetText::RichText(
                        RichText::new(format!("{}%", (progress * 100.0) as usize))
                            .small()
                            .strong(),
                    ),
                };
                let galley = text.into_galley(ui, Some(false), f32::INFINITY, TextStyle::Button);
                let text_pos = outer_rect.left_center() - Vec2::new(0.0, galley.size().y / 2.0)
                    + vec2(ui.spacing().item_spacing.x / 2.0, 0.0);
                let text_color = visuals
                    .override_text_color
                    .unwrap_or(visuals.selection.stroke.color);
                galley.paint_with_fallback_color(
                    &ui.painter().with_clip_rect(outer_rect),
                    text_pos,
                    text_color,
                );
            }
        }

        response
    }
}
