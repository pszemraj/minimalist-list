use super::{
    DragState, INSERT_ANIMATION_SECONDS, MinimalistApp, ROW_HEIGHT, SWIPE_ACTIONS, SWIPE_COMPLETE,
    theme::Palette,
};
use crate::model::Task;
use eframe::egui;
use egui::{
    Align, Align2, Button, Color32, CornerRadius, CursorIcon, Frame, Id, Key, Margin, Pos2, Rect,
    ScrollArea, Sense, Stroke, TextEdit, Vec2,
};
use std::time::Instant;
use uuid::Uuid;

impl MinimalistApp {
    pub(super) fn list_ui(&mut self, ui: &mut egui::Ui, list_id: Uuid, palette: Palette) {
        let Some(index) = self.list_index(list_id) else {
            self.go_to_overview();
            return;
        };
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
        let submit = ui.input(|input| input.key_pressed(Key::Enter))
            && (create_response.has_focus() || create_response.lost_focus());
        if submit {
            self.add_task(list_id);
        }

        let top_drag = ui.allocate_response(Vec2::new(ui.available_width(), 14.0), Sense::drag());
        if top_drag.dragged() && top_drag.drag_delta().y > 0.0 {
            self.request_task_focus = true;
        }

        ui.add_space(4.0);
        let tasks = self.lists[index].data.tasks.clone();
        let archive_count = self.lists[index].data.archive.len();
        let completed_count = self.lists[index].data.completed_count();
        let mut action: Option<RowAction> = None;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (task_index, task) in tasks.iter().enumerate() {
                    let entry_progress = self.task_entry_progress(task.id);
                    let removal_progress = self
                        .clear_progress(list_id, task.completed)
                        .max(self.delete_progress(list_id, task.id));
                    let row_action = self.task_row(ui, list_id, task_index, task, palette);
                    if row_action.is_some() {
                        action = row_action;
                    }
                    if entry_progress < 1.0 {
                        ui.add_space(
                            -(ROW_HEIGHT + self.settings.row_padding) * (1.0 - entry_progress),
                        );
                    }
                    if removal_progress > 0.0 {
                        ui.add_space(-(ROW_HEIGHT + self.settings.row_padding) * removal_progress);
                    }
                }
                if tasks.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(42.0);
                        ui.label(self.rich("Nothing here.", 20.0, palette.muted));
                        ui.label(self.rich("Type above and press Enter.", 14.0, palette.muted));
                    });
                }
                ui.add_space(24.0);
                if completed_count > 0
                    && ui
                        .add(
                            Button::new(self.rich(
                                format!("Clear {completed_count} completed"),
                                15.0,
                                palette.muted,
                            ))
                            .frame(false),
                        )
                        .clicked()
                {
                    self.begin_clear_completed(list_id);
                }
                if archive_count > 0
                    && ui
                        .add(
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
                        .clicked()
                {
                    self.history_open = !self.history_open;
                }
                let history_progress = ui.ctx().animate_bool_with_time_and_easing(
                    Id::new(("history-panel", list_id)),
                    self.history_open && archive_count > 0,
                    0.24,
                    egui::emath::easing::cubic_out,
                );
                if history_progress > 0.001 {
                    self.history_ui(ui, list_id, palette, history_progress);
                }
            });

        if let Some(action) = action {
            self.apply_row_action(action);
        }

        let (cycle, clear_completed) = ui.input(|input| {
            (
                (input.modifiers.command && input.key_pressed(Key::Tab))
                    .then_some(input.modifiers.shift),
                input.modifiers.command
                    && input.modifiers.shift
                    && input.key_pressed(Key::Backspace),
            )
        });
        if let Some(backwards) = cycle {
            self.cycle_list(list_id, backwards);
        }
        if clear_completed && completed_count > 0 {
            self.begin_clear_completed(list_id);
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
        ui: &mut egui::Ui,
        list_id: Uuid,
        task_index: usize,
        task: &Task,
        palette: Palette,
    ) -> Option<RowAction> {
        let entry_progress = self.task_entry_progress(task.id);
        let clear_progress = self.clear_progress(list_id, task.completed);
        let delete_progress = self.delete_progress(list_id, task.id);
        let removal_progress = clear_progress.max(delete_progress);
        let desired = Vec2::new(ui.available_width(), ROW_HEIGHT + self.settings.row_padding);
        let sense = if removal_progress > 0.0 {
            Sense::hover()
        } else {
            Sense::click_and_drag()
        };
        let (rect, response) = ui.allocate_exact_size(desired, sense);
        let response = response.on_hover_and_drag_cursor(CursorIcon::Grab);
        let revealed = self.revealed == Some((list_id, task.id));
        let is_editing = self.editing == Some((list_id, task.id));
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
        let task_count = self
            .list_index(list_id)
            .map_or(1, |index| self.lists[index].data.tasks.len().max(1));
        let drag_delta = if response.dragged() {
            response
                .total_drag_delta()
                .or_else(|| remembered_drag.as_ref().map(|drag| drag.delta))
                .unwrap_or(Vec2::ZERO)
        } else {
            remembered_drag
                .as_ref()
                .map_or(Vec2::ZERO, |drag| drag.delta)
        };
        let horizontal = drag_delta.x.abs() > drag_delta.y.abs() * 1.25;
        let drag_active = response.dragged() || response.drag_stopped();

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
        let offset_y = if drag_active && !horizontal {
            let span = desired.y * task_count as f32;
            drag_delta.y.clamp(-span, span)
        } else {
            0.0
        };
        let target_reorder_shift = self
            .drag
            .as_ref()
            .filter(|drag| drag.list_id == list_id && drag.task_id != task.id)
            .map_or(0.0, |drag| {
                if drag.source < drag.target
                    && task_index > drag.source
                    && task_index <= drag.target
                {
                    -desired.y
                } else if drag.source > drag.target
                    && task_index >= drag.target
                    && task_index < drag.source
                {
                    desired.y
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
                paint_palette.text.linear_multiply(opacity),
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
        if hover_progress > 0.0 || response.dragged() {
            ui.painter().rect_stroke(
                row_rect,
                14.0,
                Stroke::new(
                    1.0,
                    paint_palette
                        .raised
                        .lerp_to_gamma(paint_palette.accent, hover_progress * 0.18),
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
            );
            if completion.clicked() {
                return Some(RowAction::Toggle(list_id, task.id));
            }
        }

        let text_color = paint_palette
            .text
            .lerp_to_gamma(paint_palette.muted, completion_progress);
        let text_clip = Rect::from_min_max(
            Pos2::new(row_rect.left() + 44.0, row_rect.top()),
            Pos2::new(row_rect.right() - 82.0, row_rect.bottom()),
        )
        .intersect(ui.clip_rect());
        let text_pos = Pos2::new(row_rect.left() + 46.0, row_rect.center().y);
        let text_rect = ui.painter().with_clip_rect(text_clip).text(
            text_pos,
            Align2::LEFT_CENTER,
            &task.text,
            self.font_id(self.settings.font_size),
            text_color,
        );
        let text_width = text_rect.width().min(text_clip.width().max(0.0));
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
        if strike_progress > 0.0 {
            ui.painter().line_segment(
                [
                    Pos2::new(text_pos.x, text_pos.y),
                    Pos2::new(text_pos.x + text_width * strike_progress, text_pos.y),
                ],
                Stroke::new(2.0, paint_palette.accent),
            );
        }
        let text_response = (!is_editing).then(|| {
            ui.interact(
                text_rect.expand2(Vec2::new(4.0, 10.0)).intersect(text_clip),
                Id::new(("task-text", list_id, task.id)),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::Text)
        });

        if !is_editing {
            let checklist_rect = Rect::from_center_size(
                Pos2::new(row_rect.right() - 52.0, row_rect.center().y),
                Vec2::new(42.0, 30.0),
            );
            let checklist_label = task
                .progress()
                .map_or_else(|| "+".to_owned(), |(done, total)| format!("{done}/{total}"));
            let checklist_opacity = if task.progress().is_some() {
                0.72 + hover_progress * 0.28
            } else {
                0.24 + hover_progress * 0.76
            };
            let checklist = ui
                .put(
                    checklist_rect,
                    Button::new(self.rich(
                        checklist_label,
                        13.0,
                        paint_palette.accent.linear_multiply(checklist_opacity),
                    ))
                    .frame(false),
                )
                .on_hover_text("Subtasks");
            if checklist.clicked() {
                return Some(RowAction::Subtasks(list_id, task.id));
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
                            "x",
                            14.0,
                            paint_palette
                                .danger
                                .linear_multiply(0.16 + hover_progress * 0.84),
                        ),
                    )
                    .frame(false),
                )
                .on_hover_text("Delete task");
            if delete.clicked() {
                return Some(RowAction::Delete(list_id, task.id));
            }
        }

        if is_editing {
            let editor_rect = row_rect.shrink2(Vec2::new(12.0, 8.0));
            ui.painter().rect_filled(editor_rect, 10.0, palette.raised);
            let mut editor_ui = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(("task-editor", list_id, task.id))
                    .max_rect(editor_rect),
            );
            let edit_font = self.font_id(self.settings.font_size);
            let edit_response = editor_ui.add(
                TextEdit::singleline(&mut self.edit_text)
                    .font(edit_font)
                    .desired_width(f32::INFINITY)
                    .frame(Frame::NONE),
            );
            if self.request_edit_focus {
                edit_response.request_focus();
                self.request_edit_focus = false;
            }
            let escape = editor_ui.input(|input| input.key_pressed(Key::Escape));
            let enter = editor_ui.input(|input| input.key_pressed(Key::Enter));
            if escape {
                self.editing = None;
            } else if (enter && (edit_response.has_focus() || edit_response.lost_focus()))
                || edit_response.lost_focus()
            {
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

            if response.drag_started() {
                self.drag = Some(DragState {
                    list_id,
                    task_id: task.id,
                    source: task_index,
                    target: task_index,
                    delta: Vec2::ZERO,
                });
            }
            if response.dragged()
                && let Some(drag) = self
                    .drag
                    .as_mut()
                    .filter(|drag| drag.list_id == list_id && drag.task_id == task.id)
            {
                drag.delta = drag_delta;
                if !horizontal && task_count > 0 {
                    let shift = (drag_delta.y / desired.y).round() as isize;
                    drag.target = (drag.source as isize + shift)
                        .clamp(0, task_count.saturating_sub(1) as isize)
                        as usize;
                }
            }
            if response.drag_stopped() {
                let drag = self
                    .drag
                    .take()
                    .filter(|drag| drag.list_id == list_id && drag.task_id == task.id);
                if let Some(drag) = drag {
                    let drag_is_horizontal = drag.delta.x.abs() > drag.delta.y.abs() * 1.25;
                    if drag_is_horizontal {
                        if revealed {
                            if drag.delta.x > SWIPE_ACTIONS * 0.35 {
                                self.revealed = None;
                            }
                        } else if drag.delta.x >= SWIPE_COMPLETE {
                            return Some(RowAction::Toggle(list_id, task.id));
                        } else if drag.delta.x <= -SWIPE_ACTIONS {
                            self.revealed = Some((list_id, task.id));
                        }
                    } else if drag.target != drag.source {
                        return Some(RowAction::Reorder(list_id, drag.task_id, drag.target));
                    }
                }
            }

            if text_response
                .as_ref()
                .is_some_and(|response| response.clicked())
                && !revealed
            {
                return Some(RowAction::BeginEdit(list_id, task.id, task.text.clone()));
            }
        }

        let subtasks_open = self.subtasks_for == Some((list_id, task.id));
        let subtasks_progress = ui.ctx().animate_bool_with_time_and_easing(
            Id::new(("subtasks-panel", list_id, task.id)),
            subtasks_open,
            0.22,
            egui::emath::easing::cubic_out,
        );
        if subtasks_progress > 0.001 {
            self.subtasks_panel(ui, list_id, task, palette, subtasks_progress);
        }

        None
    }

    fn apply_row_action(&mut self, action: RowAction) {
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
                self.editing = Some((list_id, task_id));
                self.edit_text = text;
                self.request_edit_focus = true;
                self.revealed = None;
            }
            RowAction::CommitEdit(list_id, task_id, text) => {
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
                self.editing = None;
            }
            RowAction::Reorder(list_id, task_id, target) => {
                if let Some(index) = self.list_index(list_id) {
                    let tasks = &mut self.lists[index].data.tasks;
                    if let Some(source) = tasks.iter().position(|task| task.id == task_id) {
                        let task = tasks.remove(source);
                        let target = target.min(tasks.len());
                        tasks.insert(target, task);
                        self.save_list_index(index);
                    }
                }
            }
            RowAction::Subtasks(list_id, task_id) => {
                self.subtasks_for = if self.subtasks_for == Some((list_id, task_id)) {
                    None
                } else {
                    Some((list_id, task_id))
                };
                self.new_subtask.clear();
            }
        }
    }
}

#[derive(Clone)]
enum RowAction {
    Toggle(Uuid, Uuid),
    Delete(Uuid, Uuid),
    BeginEdit(Uuid, Uuid, String),
    CommitEdit(Uuid, Uuid, String),
    Reorder(Uuid, Uuid, usize),
    Subtasks(Uuid, Uuid),
}
