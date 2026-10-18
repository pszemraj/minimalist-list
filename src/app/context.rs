//! Hidden task context, complete-task clipboard actions, and wrapped-row previews.

use super::{MinimalistApp, keyboard::FocusItem, theme::Palette};
use crate::{
    model::{Task, TitleOverflow},
    storage,
};
use eframe::egui::{self, Id, Sense, Stroke, Vec2};
use uuid::Uuid;

/// An explicit context draft or read-only archived context inspection.
pub(super) struct TaskContextEditor {
    pub(super) list: Uuid,
    pub(super) task: Uuid,
    pub(super) archived: bool,
    pub(super) text: String,
    title: String,
    failure: Option<String>,
    request_focus: bool,
    return_focus: Option<Id>,
}

impl MinimalistApp {
    fn task_snapshot(&self, list: Uuid, task: Uuid, archived: bool) -> Option<Task> {
        let index = self.list_index(list)?;
        if archived {
            self.lists[index]
                .data
                .archive
                .iter()
                .find(|item| item.task.id == task)
                .map(|item| item.task.clone())
        } else {
            self.lists[index]
                .data
                .tasks
                .iter()
                .find(|item| item.id == task)
                .cloned()
        }
    }

    /// Opens the task's hidden notes after saving outgoing inline text.
    ///
    /// # Arguments
    ///
    /// - `list` - Parent list identity.
    /// - `task` - Task identity.
    /// - `archived` - Whether the task is read-only in History.
    pub(super) fn open_task_context(&mut self, list: Uuid, task: Uuid, archived: bool) {
        let return_focus = Some(if archived {
            FocusItem::Archived(list, task).id()
        } else {
            FocusItem::Task(list, task).id()
        });
        self.finish_inline_edit();
        let Some(snapshot) = self.task_snapshot(list, task, archived) else {
            return;
        };
        self.drag = None;
        self.keyboard.pending = None;
        self.task_context = Some(TaskContextEditor {
            list,
            task,
            archived,
            text: snapshot.context,
            title: snapshot.text,
            failure: None,
            request_focus: true,
            return_focus,
        });
    }

    /// Saves the explicit context draft, retaining it on failure for the same retry.
    ///
    /// # Returns
    ///
    /// Whether no editable draft remains unsaved; false keeps its modal open.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index becomes invalid.
    pub(super) fn save_task_context(&mut self) -> bool {
        let Some(mut editor) = self.task_context.take() else {
            return true;
        };
        if editor.archived {
            self.keyboard.pending = editor.return_focus;
            return true;
        }
        let Some(index) = self.list_index(editor.list) else {
            editor.failure =
                Some("The original list is unavailable. Your context draft is still here.".into());
            self.task_context = Some(editor);
            return false;
        };
        let mut candidate = self.lists[index].clone();
        let Some(task) = candidate
            .data
            .tasks
            .iter_mut()
            .find(|task| task.id == editor.task)
        else {
            editor.failure =
                Some("The original task is unavailable. Your context draft is still here.".into());
            self.task_context = Some(editor);
            return false;
        };
        task.context.clone_from(&editor.text);
        self.prepare_list_for_save(&mut candidate);
        match storage::save_list(&candidate) {
            Ok(()) => {
                self.lists[index] = candidate;
                self.apply_task_sorts();
                self.complete_list_save(index);
                self.keyboard.pending = editor.return_focus;
                true
            }
            Err(error) => {
                editor.failure = Some(format!("Not saved. {error}"));
                self.task_context = Some(editor);
                false
            }
        }
    }

    /// Renders the explicit notes editor or read-only History inspection.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Context hosting the modal.
    pub(super) fn task_context_ui(&mut self, ctx: &egui::Context) {
        if self.quick_capture.is_some() || self.find.is_some() || self.shortcuts.is_some() {
            if let Some(editor) = &mut self.task_context {
                editor.request_focus = true;
            }
            return;
        }
        let Some(mut editor) = self.task_context.take() else {
            return;
        };
        let mut save = false;
        let mut cancel = false;
        let palette = self.palette();
        let modal = egui::Modal::new(Id::new("task-context-modal")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 48.0).clamp(120.0, 440.0));
            ui.label(self.rich(
                if editor.archived {
                    "Archived task context"
                } else {
                    "Task context"
                },
                20.0,
                palette.text,
            ));
            ui.add(
                egui::Label::new(self.content_rich(&editor.title, 15.0, palette.muted)).truncate(),
            )
            .on_hover_text(&editor.title);
            let font = self.content_font_id(15.0);
            egui::ScrollArea::vertical()
                .id_salt("task-context-scroll")
                .max_height((ctx.content_rect().height() - 220.0).max(40.0))
                .show(ui, |ui| {
                    if editor.archived {
                        ui.add(
                            egui::Label::new(self.content_rich(
                                if editor.text.is_empty() {
                                    "No context"
                                } else {
                                    &editor.text
                                },
                                15.0,
                                palette.text,
                            ))
                            .wrap(),
                        );
                    } else {
                        let response = ui.add(
                            egui::TextEdit::multiline(&mut editor.text)
                                .id(Id::new("task-context-text"))
                                .font(font)
                                .hint_text("Notes, links, or supporting details")
                                .desired_width(f32::INFINITY)
                                .desired_rows(5),
                        );
                        if editor.request_focus {
                            response.request_focus();
                            editor.request_focus = false;
                        }
                    }
                });
            if let Some(error) = &editor.failure {
                egui::ScrollArea::vertical()
                    .id_salt("task-context-error")
                    .max_height(40.0)
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(self.rich(error, 13.0, palette.danger)).wrap());
                    });
            }
            ui.horizontal(|ui| {
                if !editor.archived {
                    save = ui
                        .button(if editor.failure.is_some() {
                            "Retry save"
                        } else {
                            "Save"
                        })
                        .clicked();
                }
                let close = ui.button(if editor.archived { "Close" } else { "Cancel" });
                if editor.archived && editor.request_focus {
                    close.request_focus();
                    editor.request_focus = false;
                }
                cancel = close.clicked();
            });
        });
        let return_focus = editor.return_focus;
        let read_only = editor.archived;
        self.task_context = Some(editor);
        // Notes use explicit Save/Cancel; an outside click must not discard a draft.
        let dismiss = modal.should_close() && (read_only || !modal.backdrop_response.clicked());
        if cancel || dismiss {
            self.task_context = None;
            self.keyboard.pending = return_focus;
        } else if save {
            self.save_task_context();
        }
    }

    /// Copies the full task, including current editor text, without ending the editor.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Context receiving the native clipboard command.
    /// - `list` - Parent list identity.
    /// - `task` - Task identity.
    /// - `archived` - Whether the task resides in History.
    pub(super) fn copy_task(
        &mut self,
        ctx: &egui::Context,
        list: Uuid,
        task: Uuid,
        archived: bool,
    ) {
        let Some(mut snapshot) = self.task_snapshot(list, task, archived) else {
            return;
        };
        if !archived && self.editing == Some((list, task)) {
            snapshot.text.clone_from(&self.edit_text);
        }
        if let Some(editor) = &self.details.editor
            && editor.list == list
            && editor.task == task
            && let Some(child) = snapshot
                .subtasks
                .iter_mut()
                .find(|child| child.id == editor.subtask)
        {
            child.text.clone_from(&editor.text);
        }
        if let Some(editor) = &self.task_context
            && editor.list == list
            && editor.task == task
            && editor.archived == archived
        {
            snapshot.context.clone_from(&editor.text);
        }
        ctx.copy_text(snapshot.full_text());
        self.acknowledge("Copied complete task".into());
    }

    /// Shows the task's hidden context, copy, and move actions in its existing menu.
    ///
    /// # Arguments
    ///
    /// - `response` - Row or text control owning the menu.
    /// - `list` - Parent list identity.
    /// - `task` - Task identity.
    /// - `archived` - Whether the task is in read-only History.
    pub(super) fn task_actions_menu(
        &mut self,
        response: &egui::Response,
        list: Uuid,
        task: Uuid,
        archived: bool,
    ) {
        response.context_menu(|ui| {
            self.details.menu_task = Some((list, task, archived));
            self.details.menu_popup = Some(egui::Popup::default_response_id(response));
            let has_context = self
                .task_snapshot(list, task, archived)
                .is_some_and(|task| !task.context.is_empty());
            let label = if archived {
                "View context…"
            } else if has_context {
                "Edit context…"
            } else {
                "Add context…"
            };
            if ui.button(label).clicked() {
                self.open_task_context(list, task, archived);
                ui.close();
            }
            if ui.button("Copy complete task").clicked() {
                self.copy_task(ui.ctx(), list, task, archived);
                ui.close();
            }
            if !archived && ui.button("Move to list…").clicked() {
                self.begin_move_task(list, task);
                ui.close();
            }
        });
    }

    /// Renders the context presence control without exposing any note text.
    ///
    /// # Arguments
    /// - `ui` - Row UI.
    /// - `rect` - Reserved control bounds.
    /// - `list` - List identity.
    /// - `task` - Task identity.
    /// - `palette` - Control colors.
    pub(super) fn task_context_control(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        list: Uuid,
        task: Uuid,
        palette: Palette,
    ) {
        let response = ui
            .interact(
                rect,
                Id::new(("task-context-presence", task)),
                Sense::click(),
            )
            .on_hover_text("Edit context");
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Edit context")
        });
        self.track_focus(&response, FocusItem::Task(list, task));
        let center = rect.center();
        ui.painter()
            .circle_stroke(center, 8.0, Stroke::new(1.5, palette.accent));
        for offset in [-3.0, 0.0, 3.0] {
            ui.painter().line_segment(
                [
                    center + Vec2::new(-3.5, offset),
                    center + Vec2::new(3.5, offset),
                ],
                Stroke::new(1.0, palette.accent),
            );
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                4.0,
                Stroke::new(1.0, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        if response.clicked() {
            self.open_task_context(list, task, false);
        }
    }

    /// Renders a compact, named copy control in reserved task action space.
    ///
    /// # Arguments
    ///
    /// - `ui` - Destination UI.
    /// - `rect` - Reserved control bounds.
    /// - `target` - List, task, and archived identities.
    /// - `palette` - Control colors.
    pub(super) fn task_copy_control(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        target: (Uuid, Uuid, bool),
        palette: Palette,
    ) {
        let (list, task, archived) = target;
        let response = ui
            .interact(
                rect,
                Id::new(("copy-task", list, task, archived)),
                Sense::click(),
            )
            .on_hover_text(if cfg!(target_os = "macos") {
                "Copy complete task (Cmd+Shift+C)"
            } else {
                "Copy complete task (Ctrl+Shift+C)"
            });
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Copy complete task")
        });
        let icon = egui::Rect::from_center_size(rect.center(), Vec2::new(14.0, 16.0));
        ui.painter().rect_stroke(
            icon.translate(Vec2::new(-2.0, -2.0)),
            1.0,
            Stroke::new(1.5, palette.muted),
            egui::StrokeKind::Inside,
        );
        ui.painter()
            .rect_filled(icon.translate(Vec2::new(2.0, 2.0)), 1.0, palette.surface);
        ui.painter().rect_stroke(
            icon.translate(Vec2::new(2.0, 2.0)),
            1.0,
            Stroke::new(1.5, palette.muted),
            egui::StrokeKind::Inside,
        );
        self.track_focus(
            &response,
            if archived {
                FocusItem::Archived(list, task)
            } else {
                FocusItem::Task(list, task)
            },
        );
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                4.0,
                Stroke::new(1.0, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        if response.clicked() {
            self.copy_task(ui.ctx(), list, task, archived);
        }
    }

    /// Shows a delayed metadata preview only for wrapped task rows.
    ///
    /// # Arguments
    ///
    /// - `ui` - Row UI.
    /// - `response` - Hovered task row or title.
    /// - `task` - Task whose public title and basic metadata are shown.
    /// - `palette` - Tooltip colors.
    pub(super) fn wrapped_task_preview(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        task: &Task,
        palette: Palette,
    ) {
        let id = Id::new(("task-preview-hover", task.id));
        let now = ui.input(|input| input.time);
        let hovered = self.settings.title_overflow == TitleOverflow::Wrap
            && response.hovered()
            && self.drag.is_none()
            && self.editing.is_none();
        let started = ui.ctx().data_mut(|data| {
            if hovered {
                if let Some(started) = data.get_temp::<f64>(id) {
                    Some(started)
                } else {
                    data.insert_temp(id, now);
                    Some(now)
                }
            } else {
                data.remove::<f64>(id);
                None
            }
        });
        if let Some(started) = started {
            if now - started >= 2.0 {
                egui::Tooltip::for_widget(response).show(|ui| {
                    ui.add(
                        egui::Label::new(self.content_rich(&task.text, 16.0, palette.text)).wrap(),
                    );
                    let (done, total) = task.progress().unwrap_or((0, 0));
                    ui.label(self.rich(
                        format!(
                            "{} · {done}/{total} subtasks",
                            if task.completed {
                                "Completed"
                            } else {
                                "Active"
                            }
                        ),
                        12.0,
                        palette.muted,
                    ));
                    let age = crate::model::now_unix_seconds().saturating_sub(task.created_at_unix);
                    let created = if age >= 86_400 {
                        format!("Created {} days ago", age / 86_400)
                    } else if age >= 3_600 {
                        format!("Created {} hours ago", age / 3_600)
                    } else if age >= 60 {
                        format!("Created {} minutes ago", age / 60)
                    } else {
                        "Created just now".into()
                    };
                    ui.label(self.rich(created, 12.0, palette.muted));
                });
            } else {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(
                        2.0 - (now - started),
                    ));
            }
        }
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
