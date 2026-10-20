//! Task-list screen and direct task interactions.

use super::{
    DragState, INSERT_ANIMATION_SECONDS, MinimalistApp, SWIPE_ACTIONS, SWIPE_COMPLETE,
    keyboard::{FocusItem, take_key},
    text_input_shortcuts,
    theme::Palette,
};
use crate::model::{Task, TaskSort};
use eframe::egui;
use egui::{
    Align, Align2, Button, Color32, CornerRadius, CursorIcon, Frame, Id, Margin, Pos2, Rect,
    ScrollArea, Sense, Stroke, TextEdit, Vec2,
};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

struct TaskRowLayout {
    reserved: f32,
    galley: Arc<egui::Galley>,
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
fn reorder_target(centers: &[f32], position: f32) -> usize {
    centers
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| ((*a - position).abs()).total_cmp(&(*b - position).abs()))
        .map_or(0, |(index, _)| index)
}

impl MinimalistApp {
    /// Renders one list and handles task creation, editing, completion, and ordering.
    ///
    /// # Arguments
    ///
    /// - `ui` - Destination UI for the list screen.
    /// - `list_id` - Identity of the list to display.
    /// - `palette` - Colors used to render the screen.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index becomes invalid while the screen is rendered.
    pub(super) fn list_ui(&mut self, ui: &mut egui::Ui, list_id: Uuid, palette: Palette) {
        self.keyboard.history_toggle = None;
        let Some(index) = self.list_index(list_id) else {
            self.go_to_overview();
            return;
        };
        let manual_sort = self.task_sort_mode(list_id) == TaskSort::Manual;
        let title = self.lists[index].data.title.clone();
        self.header(ui, palette, &title, Some(list_id));
        ui.add_space(8.0);

        let task_font = self.font_id(self.settings.font_size);
        let create_frame = Frame::NONE
            .fill(palette.surface)
            .corner_radius(CornerRadius::same(14))
            .inner_margin(Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.add(
                    TextEdit::singleline(&mut self.new_task)
                        .id(Id::new(("new-task", list_id)))
                        .font(task_font)
                        .hint_text("Type a task and press Enter")
                        .desired_width(f32::INFINITY)
                        .frame(Frame::NONE),
                )
            });
        let create_rect = create_frame.response.rect;
        let create_response = create_frame.inner;
        if self.request_task_focus {
            create_response.request_focus();
            self.request_task_focus = false;
        }
        let focus_glow = ui.ctx().animate_bool_with_time_and_easing(
            Id::new(("task-input-focus", list_id)),
            create_response.has_focus(),
            0.16,
            egui::emath::easing::cubic_out,
        );
        ui.painter().rect_stroke(
            create_rect.shrink(0.5),
            14.0,
            Stroke::new(
                1.0 + focus_glow,
                palette
                    .raised
                    .lerp_to_gamma(palette.accent, 0.25 + focus_glow * 0.75),
            ),
            egui::StrokeKind::Inside,
        );
        let (submit, _) = text_input_shortcuts(ui, &create_response);
        if submit {
            self.add_task(list_id);
            create_response.request_focus();
        }

        let top_drag = ui.allocate_response(Vec2::new(ui.available_width(), 14.0), Sense::DRAG);
        if top_drag.dragged() && top_drag.drag_delta().y > 0.0 {
            self.request_task_focus = true;
        }

        ui.add_space(4.0);
        let task_count = self.lists[index].data.tasks.len();
        let archive_count = self.lists[index].data.archive.len();
        let completed_count = self.lists[index].data.completed_count();
        let mut actions = Vec::new();
        let mut row_centers = Vec::new();
        let mut row_heights = Vec::new();
        if self.drag.as_ref().is_some_and(|drag| {
            drag.list_id != list_id
                || self.lists[index]
                    .data
                    .tasks
                    .get(drag.source)
                    .map(|task| task.id)
                    != Some(drag.task_id)
        }) {
            self.drag = None;
        }

        ScrollArea::vertical()
            .id_salt(("task-list-scroll", &self.workspace, list_id))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for task_index in 0..task_count {
                    let task = &self.lists[index].data.tasks[task_index];
                    let task_id = task.id;
                    let completed = task.completed;
                    let row_top = ui.next_widget_position().y;
                    let reserved = self.task_controls_width(ui, task);
                    let galley = self.task_galley(ui, &task.text, ui.available_width() - reserved);
                    let row_height = self.task_row_height(&galley);
                    row_centers.push(row_top + row_height * 0.5);
                    let layout = TaskRowLayout { reserved, galley };
                    let entry_progress = self.task_entry_progress(task_id);
                    let removal_progress = self
                        .clear_progress(list_id, completed)
                        .max(self.delete_progress(list_id, task_id));
                    let row_action = ui
                        .scope_builder(
                            egui::UiBuilder::new().id(Id::new(("task-row", list_id, task_id))),
                            |ui| self.task_row(ui, list_id, task_index, layout, palette),
                        )
                        .inner;
                    row_heights.push(ui.next_widget_position().y - row_top);
                    if let Some(row_action) = row_action {
                        actions.push(row_action);
                    }
                    if entry_progress < 1.0 {
                        ui.add_space(-row_height * (1.0 - entry_progress));
                    }
                    if removal_progress > 0.0 {
                        ui.add_space(-row_height * removal_progress);
                    }
                }
                if task_count == 0 {
                    ui.vertical_centered(|ui| {
                        ui.add_space(42.0);
                        ui.scope(|ui| {
                            ui.label(self.rich("Nothing here.", 20.0, palette.muted));
                            ui.label(self.rich("Type above and press Enter.", 14.0, palette.muted));
                        });
                    });
                }
                ui.add_space(24.0);
                if completed_count > 0 {
                    let archive = ui
                        .scope(|ui| {
                            ui.add(
                                Button::new(self.rich(
                                    format!("Archive {completed_count} completed"),
                                    15.0,
                                    palette.muted,
                                ))
                                .frame(false),
                            )
                        })
                        .inner;
                    if archive.clicked() {
                        self.begin_clear_completed(list_id);
                    }
                }
                if archive_count > 0 {
                    let history = ui
                        .scope(|ui| {
                            ui.push_id(("history-toggle", list_id), |ui| {
                                ui.add(
                                    Button::new(self.rich(
                                        format!(
                                            "{} history ({archive_count})",
                                            if self.history_open { "Hide" } else { "Show" }
                                        ),
                                        15.0,
                                        palette.accent,
                                    ))
                                    .frame(false),
                                )
                            })
                            .inner
                        })
                        .inner;
                    self.keyboard.history_toggle = Some(history.id);
                    if history.clicked() {
                        self.history_open = !self.history_open;
                        if self.history_open && !history.clicked_by(egui::PointerButton::Primary) {
                            self.keyboard.pending = Some(
                                FocusItem::Archived(
                                    list_id,
                                    self.lists[index].data.archive[0].task.id,
                                )
                                .id(),
                            );
                        }
                    }
                }
                let history_progress = ui.ctx().animate_bool_with_time_and_easing(
                    Id::new(("history-panel", list_id)),
                    self.history_open && archive_count > 0,
                    0.24,
                    egui::emath::easing::cubic_out,
                );
                if history_progress > 0.001 {
                    ui.add_enabled_ui(self.history_open, |ui| {
                        self.history_ui(ui, list_id, palette, history_progress);
                    });
                }
                // The fixed entry lives outside this scroller. Restoring its
                // focus must not move the list's remembered scroll position.
                if ui.ctx().memory(|memory| memory.focused()) != Some(create_response.id) {
                    self.keyboard_focus_ui(ui, palette);
                }
                if let Some(drag) = self.drag.as_ref()
                    && manual_sort
                    && drag.delta.y.abs() >= drag.delta.x.abs() * 1.25
                    && let Some(pointer) = ui.input(|input| {
                        input
                            .pointer
                            .primary_down()
                            .then(|| input.pointer.latest_pos())
                            .flatten()
                    })
                {
                    let viewport = ui.clip_rect();
                    let edge = 42.0;
                    let direction = if pointer.y < viewport.top() + edge {
                        ((viewport.top() + edge - pointer.y) / edge).clamp(0.0, 1.0)
                    } else {
                        -((pointer.y - viewport.bottom() + edge) / edge).clamp(0.0, 1.0)
                    };
                    if direction != 0.0
                        && pointer.x >= viewport.left()
                        && pointer.x <= viewport.right()
                    {
                        let step = ui.input(|input| input.stable_dt.min(0.05)) * 480.0;
                        ui.scroll_with_delta_animation(
                            Vec2::new(0.0, direction * step),
                            egui::style::ScrollAnimation::none(),
                        );
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_millis(16));
                    }
                }
            });

        if let Some(drag) = &mut self.drag {
            drag.target = if manual_sort {
                reorder_target(&row_centers, drag.source_center + drag.delta.y)
            } else {
                drag.source
            };
            drag.height = row_heights[drag.source];
        }
        // A pointer click can blur one editor and activate another row in the
        // same frame. Save the outgoing text before applying any incoming action,
        // independently of whether that row appears above or below the editor.
        actions.sort_by_key(|action| !matches!(action, RowAction::CommitEdit(..)));
        for action in actions {
            self.apply_row_action(action);
        }
    }

    fn add_task(&mut self, list_id: Uuid) {
        let text = self.new_task.trim();
        if text.is_empty() {
            return;
        }
        if let Some(index) = self.list_index(list_id) {
            let task = Task::new(text);
            self.newest_task = Some((task.id, Instant::now()));
            self.lists[index].data.tasks.insert(0, task);
            self.new_task.clear();
            self.save_list_index(index);
            self.request_task_focus = true;
            self.keyboard.pending = Some(Id::new(("new-task", list_id)));
        }
    }

    fn task_entry_progress(&self, task_id: Uuid) -> f32 {
        self.newest_task.map_or(1.0, |(id, started)| {
            if id == task_id {
                egui::emath::easing::cubic_out(
                    (started.elapsed().as_secs_f32() / INSERT_ANIMATION_SECONDS).clamp(0.0, 1.0),
                )
            } else {
                1.0
            }
        })
    }

    fn task_row(
        &mut self,
        parent_ui: &mut egui::Ui,
        list_id: Uuid,
        task_index: usize,
        layout: TaskRowLayout,
        palette: Palette,
    ) -> Option<RowAction> {
        let index = self.list_index(list_id)?;
        let task = &self.lists[index].data.tasks[task_index];
        let task_id = task.id;
        let removal_progress = self
            .clear_progress(list_id, task.completed)
            .max(self.delete_progress(list_id, task_id));
        let TaskRowLayout { reserved, galley } = layout;
        let desired = Vec2::new(parent_ui.available_width(), self.task_row_height(&galley));
        let (rect, _) = parent_ui.allocate_exact_size(desired, Sense::hover());
        // Keep the lightweight primary response for keyboard navigation and
        // scroll targets; only build offscreen contents when their state needs it.
        let sense = if removal_progress > 0.0 {
            Sense::hover()
        } else {
            Sense::click_and_drag()
        };
        let response = parent_ui.interact(rect, FocusItem::Task(list_id, task_id).id(), sense);
        let needs_contents = parent_ui.is_rect_visible(rect)
            || self.drag.is_some()
            || self.editing == Some((list_id, task_id))
            || self.subtasks_for == Some((list_id, task_id))
            || !task.subtasks.is_empty()
            || self.spotlight.as_ref().is_some_and(|spotlight| {
                spotlight.list_id == list_id && spotlight.task_id == task_id
            });
        if !needs_contents {
            self.track_focus(&response, FocusItem::Task(list_id, task_id));
            return None;
        }
        let task = task.clone();
        let task = &task;
        let manual_sort = self.task_sort_mode(list_id) == TaskSort::Manual;
        let entry_progress = self.task_entry_progress(task.id);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("Task: {}", task.text),
            )
        });
        self.track_focus(&response, FocusItem::Task(list_id, task.id));
        self.task_actions_menu(&response, list_id, task.id, false);
        let mut row_ui = parent_ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("row", task.id))
                .max_rect(rect),
        );
        let ui = &mut row_ui;
        let response = response.on_hover_and_drag_cursor(CursorIcon::Grab);
        let revealed = self.revealed == Some((list_id, task.id));
        let is_editing = self.editing == Some((list_id, task.id));
        let native_drag = (!is_editing && removal_progress == 0.0)
            .then(|| {
                self.coalesced_task_drag(
                    ui,
                    (list_id, task.id, task_index),
                    rect,
                    rect.right() - reserved + 44.0,
                )
            })
            .flatten();
        if native_drag.is_some() || response.drag_started() {
            response.request_focus();
        }
        let hover_progress = ui.ctx().animate_bool_with_time_and_easing(
            Id::new(("task-hover", list_id, task.id)),
            response.hovered(),
            0.14,
            egui::emath::easing::cubic_out,
        );
        let completion_progress = ui.ctx().animate_bool_with_time_and_easing(
            Id::new(("task-complete", list_id, task.id)),
            task.completed,
            0.22,
            egui::emath::easing::cubic_out,
        );
        let paint_palette = palette.fade(entry_progress * (1.0 - removal_progress));

        let remembered_drag = self
            .drag
            .as_ref()
            .filter(|drag| drag.list_id == list_id && drag.task_id == task.id)
            .cloned();
        let drag_delta = if native_drag.is_some() {
            remembered_drag
                .as_ref()
                .map_or(Vec2::ZERO, |drag| drag.delta)
        } else if response.dragged() {
            response
                .total_drag_delta()
                .or_else(|| remembered_drag.as_ref().map(|drag| drag.delta))
                .unwrap_or(Vec2::ZERO)
        } else if response.drag_stopped() {
            remembered_drag.as_ref().map_or(Vec2::ZERO, |drag| {
                drag.delta + ui.input(|input| input.pointer.delta())
            })
        } else {
            remembered_drag
                .as_ref()
                .map_or(Vec2::ZERO, |drag| drag.delta)
        };
        let horizontal = drag_delta.x.abs() > drag_delta.y.abs() * 1.25;
        let drag_active = native_drag.is_some() || response.dragged() || response.drag_stopped();

        let resting_x = if revealed { -108.0 } else { 0.0 };
        let animated_x = ui.ctx().animate_value_with_time(
            Id::new(("task-x", list_id, task.id)),
            resting_x,
            0.16,
        );
        let offset_x = if drag_active && horizontal {
            (animated_x + drag_delta.x).clamp(-120.0, 120.0)
        } else {
            animated_x
        };
        let offset_y = if drag_active && !horizontal && manual_sort {
            remembered_drag.as_ref().map_or(drag_delta.y, |drag| {
                drag.source_center + drag_delta.y - rect.center().y
            })
        } else {
            0.0
        };
        let target_reorder_shift = self
            .drag
            .as_ref()
            .filter(|drag| manual_sort && drag.list_id == list_id && drag.task_id != task.id)
            .map_or(0.0, |drag| {
                if drag.source < drag.target
                    && task_index > drag.source
                    && task_index <= drag.target
                {
                    -drag.height
                } else if drag.source > drag.target
                    && task_index >= drag.target
                    && task_index < drag.source
                {
                    drag.height
                } else {
                    0.0
                }
            });
        let reorder_shift = ui.ctx().animate_value_with_time(
            Id::new(("task-reorder-shift", list_id, task.id)),
            target_reorder_shift,
            0.14,
        );

        let action_reveal = (-offset_x / 108.0).clamp(0.0, 1.0);
        let edit_rect = Rect::from_min_max(
            Pos2::new(rect.right() - 108.0, rect.top()),
            Pos2::new(rect.right() - 54.0, rect.bottom()),
        );
        let delete_rect = Rect::from_min_max(
            Pos2::new(rect.right() - 54.0, rect.top()),
            rect.right_bottom(),
        );
        if action_reveal > 0.01 {
            let opacity = action_reveal.max(0.25);
            ui.painter().rect_filled(
                edit_rect,
                12.0,
                paint_palette.raised.linear_multiply(opacity),
            );
            ui.painter().rect_filled(
                delete_rect,
                12.0,
                paint_palette.danger.linear_multiply(opacity),
            );
            ui.painter().text(
                edit_rect.center(),
                Align2::CENTER_CENTER,
                "Edit",
                self.font_id(13.0),
                paint_palette.text.linear_multiply(opacity),
            );
            ui.painter().text(
                delete_rect.center(),
                Align2::CENTER_CENTER,
                "Del",
                self.font_id(13.0),
                Color32::from_rgb(50, 10, 16).linear_multiply(opacity),
            );
        }

        if let Some(drag) = self.drag.as_ref().filter(|drag| drag.list_id == list_id)
            && drag.target == task_index
            && drag.target != drag.source
        {
            let y = if drag.target > drag.source {
                rect.bottom()
            } else {
                rect.top()
            };
            ui.painter().line_segment(
                [
                    Pos2::new(rect.left() + 8.0, y),
                    Pos2::new(rect.right() - 8.0, y),
                ],
                Stroke::new(2.0, paint_palette.accent),
            );
        }

        let row_rect = rect.translate(Vec2::new(
            offset_x,
            offset_y + reorder_shift - (1.0 - entry_progress) * 14.0 - removal_progress * 10.0,
        ));
        let elevation = hover_progress + if response.dragged() { 0.8 } else { 0.0 };
        if elevation > 0.0 {
            let shadow = egui::epaint::Shadow {
                offset: [0, 3],
                blur: 12,
                spread: 0,
                color: Color32::from_black_alpha((30.0 * elevation.min(1.0)) as u8),
            };
            ui.painter()
                .add(shadow.as_shape(row_rect, CornerRadius::same(14)));
        }
        ui.painter().rect_filled(
            row_rect,
            14.0,
            paint_palette
                .surface
                .lerp_to_gamma(paint_palette.raised, hover_progress * 0.24),
        );
        if hover_progress > 0.0 || response.dragged() || response.has_focus() {
            ui.painter().rect_stroke(
                row_rect,
                14.0,
                Stroke::new(
                    1.0,
                    if response.has_focus() {
                        paint_palette.accent
                    } else {
                        paint_palette
                            .raised
                            .lerp_to_gamma(paint_palette.accent, hover_progress * 0.18)
                    },
                ),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(spotlight) = self.spotlight.as_mut().filter(|spotlight| {
            !spotlight.archived && spotlight.list_id == list_id && spotlight.task_id == task.id
        }) {
            if spotlight.scroll_pending && spotlight.subtask_id.is_none() {
                ui.scroll_to_rect(row_rect, Some(Align::Center));
                spotlight.scroll_pending = false;
            }
            let opacity = (1.0 - spotlight.started.elapsed().as_secs_f32() / 1.4).clamp(0.0, 1.0);
            ui.painter().rect_stroke(
                row_rect.shrink(1.0),
                13.0,
                Stroke::new(2.0, paint_palette.accent.linear_multiply(opacity)),
                egui::StrokeKind::Inside,
            );
        }

        if !is_editing {
            let completion_rect = Rect::from_center_size(
                Pos2::new(row_rect.left() + 22.0, row_rect.center().y),
                Vec2::splat(30.0),
            );
            let completion = ui.put(completion_rect, Button::new("").frame(false));
            let completion = self.completion_control(
                ui,
                completion,
                task.completed,
                completion_progress,
                paint_palette,
                &task.text,
            );
            self.track_focus(&completion, FocusItem::Task(list_id, task.id));
            if completion.clicked() {
                return Some(RowAction::Toggle(list_id, task.id));
            }
        }

        let text_color = paint_palette
            .text
            .lerp_to_gamma(paint_palette.muted, completion_progress);
        let text_clip = Rect::from_min_max(
            Pos2::new(row_rect.left() + 44.0, row_rect.top()),
            Pos2::new(row_rect.right() - reserved + 44.0, row_rect.bottom()),
        );
        let strike_progress = if task.completed {
            if drag_active && horizontal && drag_delta.x > 0.0 {
                1.0 - (drag_delta.x / SWIPE_COMPLETE).clamp(0.0, 1.0)
            } else {
                completion_progress
            }
        } else if drag_active && horizontal && drag_delta.x > 0.0 {
            (drag_delta.x / SWIPE_COMPLETE).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let text_response = (!is_editing).then(|| {
            self.paint_task_title(
                ui,
                task,
                text_clip,
                galley,
                Palette {
                    text: text_color,
                    ..paint_palette
                },
                strike_progress,
            )
        });
        if let Some(response) = &text_response {
            self.track_focus(response, FocusItem::Task(list_id, task.id));
            self.task_actions_menu(response, list_id, task.id, false);
            self.wrapped_task_preview(ui, response, task, palette);
        }

        if !is_editing {
            if !task.context.is_empty() {
                let context_rect = Rect::from_center_size(
                    Pos2::new(row_rect.right() - reserved + 62.0, row_rect.center().y),
                    Vec2::new(22.0, 30.0),
                );
                self.task_context_control(ui, context_rect, list_id, task.id, paint_palette);
            }
            if self.task_checklist_control(ui, row_rect, list_id, task, paint_palette) {
                return Some(RowAction::Subtasks(list_id, task.id));
            }

            if self.narrow_row_savings(row_rect.width()) == 0.0 {
                let copy_rect = Rect::from_center_size(
                    Pos2::new(row_rect.right() - 48.0, row_rect.center().y),
                    Vec2::new(26.0, 30.0),
                );
                self.task_copy_control(ui, copy_rect, (list_id, task.id, false), paint_palette);
            }

            let delete_rect = Rect::from_center_size(
                Pos2::new(row_rect.right() - 17.0, row_rect.center().y),
                Vec2::splat(30.0),
            );
            let delete = ui
                .put(
                    delete_rect,
                    Button::new(
                        self.rich(
                            "×",
                            25.0,
                            paint_palette
                                .danger
                                .linear_multiply(0.72 + hover_progress * 0.28),
                        ),
                    )
                    .frame(false),
                )
                .on_hover_text("Delete task");
            self.track_focus(&delete, FocusItem::Task(list_id, task.id));
            delete.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    format!("Delete task: {}", task.text),
                )
            });
            if delete.has_focus() {
                ui.painter().rect_stroke(
                    delete.rect,
                    4.0,
                    Stroke::new(1.0, palette.accent),
                    egui::StrokeKind::Inside,
                );
            }
            if delete.clicked() {
                return Some(RowAction::Delete(list_id, task.id));
            }
        }

        if is_editing {
            let editor_rect = row_rect.shrink2(Vec2::new(12.0, 4.0));
            ui.painter().rect_filled(editor_rect, 10.0, palette.raised);
            let mut editor_ui = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(("task-editor", list_id, task.id))
                    .max_rect(editor_rect),
            );
            let edit_font = self.font_id(self.settings.font_size);
            let edit_response = editor_ui.add(
                TextEdit::singleline(&mut self.edit_text)
                    .id(Id::new(("edit-task", list_id, task.id)))
                    .font(edit_font)
                    .desired_width(f32::INFINITY)
                    .frame(Frame::NONE),
            );
            if self.request_edit_focus {
                edit_response.request_focus();
                self.request_edit_focus = false;
            }
            let (submit, cancel) = text_input_shortcuts(&editor_ui, &edit_response);
            if cancel {
                self.editing = None;
            } else if submit || edit_response.lost_focus() {
                if submit {
                    take_key(ui.ctx(), egui::Key::Enter, egui::Modifiers::NONE);
                    self.keyboard.pending = Some(FocusItem::Task(list_id, task.id).id());
                }
                return Some(RowAction::CommitEdit(
                    list_id,
                    task.id,
                    self.edit_text.clone(),
                ));
            }
        } else {
            if action_reveal > 0.5
                && response.clicked()
                && let Some(position) = response.interact_pointer_pos()
            {
                if edit_rect.contains(position) {
                    return Some(RowAction::BeginEdit(list_id, task.id, task.text.clone()));
                }
                if delete_rect.contains(position) {
                    return Some(RowAction::Delete(list_id, task.id));
                }
            }

            if response.drag_started() && native_drag.is_none() {
                self.drag = Some(DragState {
                    list_id,
                    task_id: task.id,
                    source: task_index,
                    target: task_index,
                    delta: Vec2::ZERO,
                    source_center: rect.center().y,
                    height: rect.height(),
                });
            }
            if (native_drag.is_some() || response.dragged() || response.drag_stopped())
                && let Some(drag) = self
                    .drag
                    .as_mut()
                    .filter(|drag| drag.list_id == list_id && drag.task_id == task.id)
            {
                drag.delta = drag_delta;
            }
            if response.drag_stopped() || native_drag == Some(true) {
                return Some(RowAction::FinishDrag);
            }

            if text_response
                .as_ref()
                .is_some_and(|response| response.clicked())
                && !revealed
            {
                return Some(RowAction::BeginEdit(list_id, task.id, task.text.clone()));
            }
        }

        self.task_children_below(parent_ui, list_id, task, palette);

        None
    }

    /// Applies a pointer or keyboard task action using the shared persistence path.
    ///
    /// # Panics
    ///
    /// Panics if a resolved task position becomes invalid during reordering.
    pub(super) fn apply_row_action(&mut self, action: RowAction) {
        match action {
            RowAction::Toggle(list_id, task_id) => {
                if let Some(index) = self.list_index(list_id)
                    && let Some(task) = self.lists[index]
                        .data
                        .tasks
                        .iter_mut()
                        .find(|task| task.id == task_id)
                {
                    task.toggle();
                    self.save_list_index(index);
                }
            }
            RowAction::Delete(list_id, task_id) => self.begin_delete_task(list_id, task_id),
            RowAction::BeginEdit(list_id, task_id, text) => {
                if self.editing == Some((list_id, task_id)) {
                    return;
                }
                self.finish_inline_edit();
                self.editing = Some((list_id, task_id));
                self.edit_text = text;
                self.request_edit_focus = true;
                self.keyboard.pending = Some(Id::new(("edit-task", list_id, task_id)));
                self.revealed = None;
            }
            RowAction::CommitEdit(list_id, task_id, text) => {
                self.commit_task_edit(list_id, task_id, &text);
            }
            RowAction::Reorder(list_id, task_id, target) => {
                if self.task_sort_mode(list_id) != TaskSort::Manual {
                    return;
                }
                if let Some(index) = self.list_index(list_id) {
                    let tasks = &mut self.lists[index].data.tasks;
                    if let Some(source) = tasks.iter().position(|task| task.id == task_id) {
                        let target = target.min(tasks.len() - 1);
                        if source != target {
                            let task = tasks.remove(source);
                            tasks.insert(target, task);
                            self.save_list_index(index);
                        }
                    }
                }
            }
            RowAction::Subtasks(list_id, task_id) => {
                self.toggle_task_subtasks(list_id, task_id);
            }
            RowAction::FinishDrag => {
                if let Some(drag) = self.drag.take() {
                    if drag.delta.x.abs() > drag.delta.y.abs() * 1.25 {
                        if self.revealed == Some((drag.list_id, drag.task_id)) {
                            if drag.delta.x > SWIPE_ACTIONS * 0.35 {
                                self.revealed = None;
                            }
                        } else if drag.delta.x >= SWIPE_COMPLETE {
                            self.apply_row_action(RowAction::Toggle(drag.list_id, drag.task_id));
                        } else if drag.delta.x <= -SWIPE_ACTIONS {
                            self.revealed = Some((drag.list_id, drag.task_id));
                        }
                    } else if drag.target != drag.source {
                        self.apply_row_action(RowAction::Reorder(
                            drag.list_id,
                            drag.task_id,
                            drag.target,
                        ));
                    }
                }
            }
        }
    }

    /// Saves an inline edit and ends the editing session.
    ///
    /// # Arguments
    ///
    /// - `list_id` - List containing the edited task.
    /// - `task_id` - Identity of the edited task.
    /// - `text` - Proposed title; empty titles leave the existing title intact.
    ///
    /// # Panics
    ///
    /// Panics if the resolved list index becomes invalid during the edit.
    pub(super) fn commit_task_edit(&mut self, list_id: Uuid, task_id: Uuid, text: &str) {
        let text = text.trim();
        if let Some(index) = self.list_index(list_id)
            && let Some(task) = self.lists[index]
                .data
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
            && !text.is_empty()
            && task.text != text
        {
            task.text = text.to_owned();
            self.save_list_index(index);
        }
        if self.editing == Some((list_id, task_id)) {
            self.editing = None;
        }
    }
}

#[derive(Clone)]
/// Task mutations and edit/panel interactions shared by pointer and keyboard input.
pub(super) enum RowAction {
    FinishDrag,
    Toggle(Uuid, Uuid),
    Delete(Uuid, Uuid),
    BeginEdit(Uuid, Uuid, String),
    CommitEdit(Uuid, Uuid, String),
    Reorder(Uuid, Uuid, usize),
    Subtasks(Uuid, Uuid),
}

#[cfg(test)]
#[path = "list_performance_tests.rs"]
mod performance_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reorder_uses_actual_centers_of_unequal_rows() {
        let centers = [18.0, 108.0, 198.0];
        assert_eq!(reorder_target(&centers, -50.0), 0);
        assert_eq!(reorder_target(&centers, 70.0), 1);
        assert_eq!(reorder_target(&centers, 135.0), 1);
        assert_eq!(reorder_target(&centers, 180.0), 2);
        assert_eq!(reorder_target(&centers, 500.0), 2);
    }
}

#[cfg(test)]
#[path = "list_regression_tests.rs"]
mod regression_tests;
