//! Task title layout and hover-only horizontal scrolling.

use super::{MinimalistApp, theme::Palette};
use crate::model::{Task, TitleOverflow};
use eframe::egui;
use egui::{Color32, Galley, Id, Rect, Sense, Stroke, Vec2, text::LayoutJob};
use std::sync::Arc;
use std::time::{Duration, Instant};

const HOVER_DELAY: f32 = 0.6;
const SCROLL_SPEED: f32 = 30.0;
const END_PAUSE: f32 = 1.2;

fn scroll_offset(elapsed: f32, overflow: f32) -> f32 {
    let travel = overflow / SCROLL_SPEED;
    let phase = elapsed % (HOVER_DELAY + travel + END_PAUSE);
    ((phase - HOVER_DELAY) * SCROLL_SPEED).clamp(0.0, overflow)
}

/// Finds the closest task center, accounting for rows of different heights.
///
/// # Arguments
///
/// - `centers` - Visible row centers in list order.
/// - `position` - Dragged row center in the same coordinates.
///
/// # Returns
///
/// The nearest row index, or zero for an empty list.
pub(super) fn reorder_target(centers: &[f32], position: f32) -> usize {
    centers
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| ((*a - position).abs()).total_cmp(&(*b - position).abs()))
        .map_or(0, |(index, _)| index)
}

impl MinimalistApp {
    /// Lays out a title with either unlimited horizontal space or full wrapping.
    ///
    /// # Arguments
    ///
    /// - `ui` - UI providing fonts and text layout.
    /// - `text` - Task title to lay out.
    /// - `width` - Space available for a wrapped title.
    ///
    /// # Returns
    ///
    /// The title's laid-out rows and dimensions.
    pub(super) fn task_galley(&self, ui: &egui::Ui, text: &str, width: f32) -> Arc<Galley> {
        let width = match self.settings.title_overflow {
            TitleOverflow::Scroll => f32::INFINITY,
            TitleOverflow::Wrap => width.max(1.0),
        };
        ui.painter().layout(
            text.to_owned(),
            self.font_id(self.settings.font_size),
            Color32::PLACEHOLDER,
            width,
        )
    }

    /// Derives row height from the title and the user's saved padding.
    ///
    /// # Arguments
    ///
    /// - `galley` - Laid-out task title.
    ///
    /// # Returns
    ///
    /// Row height in points, with a 36-point minimum.
    pub(super) fn task_row_height(&self, galley: &Galley) -> f32 {
        (galley.size().y + self.settings.row_padding * 2.0).max(36.0)
    }

    /// Paints an accessible title, scrolling only when an overflowing title is hovered.
    ///
    /// # Arguments
    ///
    /// - `ui` - UI containing the task row.
    /// - `task` - Task whose title is displayed.
    /// - `rect` - Title's available bounds.
    /// - `galley` - Title layout for the selected overflow mode.
    /// - `palette` - Title and focus colors.
    /// - `strike_progress` - Completion strike animation progress.
    ///
    /// # Returns
    ///
    /// The title's interaction response.
    pub(super) fn paint_task_title(
        &mut self,
        ui: &egui::Ui,
        task: &Task,
        rect: Rect,
        galley: Arc<Galley>,
        palette: Palette,
        strike_progress: f32,
    ) -> egui::Response {
        let response = ui.interact(rect, Id::new(("task-text", task.id)), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("Edit task: {}", task.text),
            )
        });
        let overflow = (galley.size().x - rect.width()).max(0.0);
        let scrolling = self.settings.title_overflow == TitleOverflow::Scroll
            && overflow > 0.0
            && response.hovered()
            && self.drag.is_none()
            && self.editing.is_none()
            && ui.is_rect_visible(rect);
        let mut offset = 0.0;
        let mut shown = galley;
        let mut truncate = !scrolling;
        if scrolling {
            let started = match self.hovered_title {
                Some((id, started)) if id == task.id => started,
                _ => {
                    let now = Instant::now();
                    self.hovered_title = Some((task.id, now));
                    now
                }
            };
            let elapsed = started.elapsed().as_secs_f32();
            truncate = elapsed < HOVER_DELAY;
            offset = scroll_offset(elapsed, overflow);
            ui.ctx().request_repaint_after(if elapsed < HOVER_DELAY {
                Duration::from_secs_f32(HOVER_DELAY - elapsed)
            } else {
                Duration::from_millis(16)
            });
        } else if self.hovered_title.is_some_and(|(id, _)| id == task.id) {
            self.hovered_title = None;
        }
        if truncate && overflow > 0.0 && self.settings.title_overflow == TitleOverflow::Scroll {
            let mut job = LayoutJob::simple(
                task.text.clone(),
                self.font_id(self.settings.font_size),
                Color32::PLACEHOLDER,
                rect.width(),
            );
            job.wrap.max_rows = 1;
            job.wrap.break_anywhere = true;
            shown = ui.painter().layout_job(job);
        }
        let position = rect.left_center() - Vec2::new(offset, shown.size().y * 0.5);
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        if strike_progress > 0.0 {
            for row in &shown.rows {
                let line = row.rect().translate(position.to_vec2());
                painter.line_segment(
                    [
                        line.left_center(),
                        line.left_center() + Vec2::new(line.width() * strike_progress, 0.0),
                    ],
                    Stroke::new(2.0, palette.accent),
                );
            }
        }
        painter.galley(position, shown, palette.text);
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                4.0,
                Stroke::new(1.0, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        response.on_hover_cursor(egui::CursorIcon::Text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolling_delays_pauses_and_restarts() {
        assert_eq!(scroll_offset(0.5, 60.0), 0.0);
        assert!((scroll_offset(1.6, 60.0) - 30.0).abs() < 0.001);
        assert_eq!(scroll_offset(3.0, 60.0), 60.0);
        assert!(scroll_offset(3.9, 60.0) < 0.001);
    }

    #[test]
    fn reorder_uses_actual_centers_of_unequal_rows() {
        let centers = [18.0, 108.0, 198.0];
        assert_eq!(reorder_target(&centers, -50.0), 0);
        assert_eq!(reorder_target(&centers, 70.0), 1);
        assert_eq!(reorder_target(&centers, 135.0), 1);
        assert_eq!(reorder_target(&centers, 180.0), 2);
        assert_eq!(reorder_target(&centers, 500.0), 2);
    }

    #[test]
    fn scroll_keeps_one_line_and_wrap_changes_row_height() {
        let (ctx, mut app) = super::super::tests::test_app();
        let title = "A long task title that should remain one line until the user chooses wrapping";
        super::super::tests::run_frame(&ctx, Default::default(), |ui| {
            let single = app.task_galley(ui, title, 120.0);
            assert_eq!(single.rows.len(), 1);
            let single_height = app.task_row_height(&single);
            app.settings.title_overflow = TitleOverflow::Wrap;
            let wrapped = app.task_galley(ui, title, 120.0);
            assert!(wrapped.rows.len() > 1);
            assert!(app.task_row_height(&wrapped) > single_height);
            assert!(wrapped.size().x <= 120.0);
        });
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}
