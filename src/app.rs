//! Application state, navigation, persistence coordination, and root rendering.

#[cfg(test)]
mod adversarial_tests;
mod context;
mod desktop;
mod drafts;
mod keyboard;
mod list;
mod overlays;
mod overview;
mod persistence;
#[cfg(test)]
mod persistence_tests;
mod settings;
mod sorting;
mod task_details;
mod task_text;
mod theme;
mod window;

use self::keyboard::KeyboardState;
use self::overlays::{FindState, QuickCaptureState, ShortcutsState, Spotlight};
use self::theme::Palette;
use crate::model::Accent;
use crate::storage::{self, Settings, StoredList, WorkspaceFingerprint};
use eframe::egui;
use egui::viewport::WindowLevel;
use egui::{Align, Color32, CornerRadius, CursorIcon, Layout, Stroke, Vec2};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uuid::Uuid;

const SCAN_INTERVAL: Duration = Duration::from_secs(5);
const SWIPE_COMPLETE: f32 = 78.0;
const SWIPE_ACTIONS: f32 = 64.0;
const CLEAR_ANIMATION_SECONDS: f32 = 0.24;
const DELETE_ANIMATION_SECONDS: f32 = 0.20;
const INSERT_ANIMATION_SECONDS: f32 = 0.28;

/// Reads submit/cancel shortcuts for a text field.
///
/// Keyboard state must be read before querying `Response` focus: `Ui::input` takes the
/// context's exclusive lock, and focus queries re-enter that lock and self-deadlock.
fn text_input_shortcuts(ui: &egui::Ui, response: &egui::Response) -> (bool, bool) {
    let (enter, cancel) = ui.input(|input| {
        (
            input.key_pressed(egui::Key::Enter),
            input.key_pressed(egui::Key::Escape),
        )
    });
    // Enter surrenders a single-line editor's focus. A recent Tab also reports
    // lost_focus, but Enter then belongs to the newly focused control.
    let focused = response.has_focus()
        || (response.lost_focus() && ui.memory(|memory| memory.focused().is_none()));
    (enter && focused, cancel && focused)
}

#[derive(Clone)]
enum Screen {
    Overview,
    List(Uuid),
    Settings,
}

#[derive(Clone)]
struct DragState {
    list_id: Uuid,
    task_id: Uuid,
    source: usize,
    target: usize,
    delta: Vec2,
    source_center: f32,
    height: f32,
}

/// Owns the loaded workspace and all transient state for the native UI.
pub struct MinimalistApp {
    settings: Settings,
    persisted_workspace_path: String,
    workspace: PathBuf,
    lists: Vec<StoredList>,
    manual_task_order: std::collections::HashMap<Uuid, Vec<Uuid>>,
    fingerprint: WorkspaceFingerprint,
    screen: Screen,
    return_screen: Option<Screen>,
    settings_list_id: Option<Uuid>,
    status: Option<String>,

    new_task: String,
    request_task_focus: bool,
    creating_list: bool,
    new_list_title: String,
    new_list_accent: Accent,
    request_list_focus: bool,

    editing: Option<(Uuid, Uuid)>,
    edit_text: String,
    request_edit_focus: bool,
    subtasks_for: Option<(Uuid, Uuid)>,
    new_subtask: String,

    revealed: Option<(Uuid, Uuid)>,
    drag: Option<DragState>,
    overview_drag: Option<overview::OverviewDrag>,
    overview_hover: Option<(Uuid, f64)>,
    history_open: bool,

    workspace_input: String,
    rename_input: String,
    rename_baseline: String,
    delete_armed: bool,
    next_scan: Instant,
    last_pin_state: Option<bool>,
    last_decorations_state: Option<bool>,
    view_zoom: f32,
    transition_direction: f32,
    displayed_palette: Palette,
    newest_task: Option<(Uuid, Instant)>,
    clear_animation: Option<(Uuid, Instant)>,
    delete_animation: Option<(Uuid, Uuid, Instant)>,
    quick_capture: Option<QuickCaptureState>,
    find: Option<FindState>,
    shortcuts: Option<ShortcutsState>,
    spotlight: Option<Spotlight>,
    last_motion_tick: Instant,
    hovered_title: Option<(Uuid, Instant)>,
    tray: Option<crate::tray::Tray>,
    window_hidden: bool,
    #[cfg(any(target_os = "windows", target_os = "linux", test))]
    taskbar_hidden: Option<bool>,
    #[cfg(any(target_os = "windows", target_os = "linux", test))]
    next_taskbar_check: Instant,
    instance: Option<crate::instance::InstanceGuard>,
    global_shortcut: Option<crate::hotkey::GlobalShortcut>,
    global_shortcut_input: String,
    /// Last shortcut failure shown, cleared by a later successful apply.
    shortcut_error: Option<String>,
    task_context: Option<context::TaskContextEditor>,
    #[cfg(target_os = "macos")]
    reopen: Option<crate::macos::ReopenHandler>,
    quitting: bool,
    keyboard: KeyboardState,
    persistence: persistence::PersistenceState,
    drafts: drafts::Drafts,
    details: task_details::TaskDetailsState,
    capture_draft: Option<QuickCaptureState>,
    find_draft: Option<FindState>,
}

impl MinimalistApp {
    /// Initializes the UI, resolves the workspace, and loads its lists.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Egui context used to configure the interface.
    /// - `settings` - Persisted preferences to apply.
    /// - `workspace_override` - Optional workspace path supplied for this launch.
    /// - `startup_warning` - Optional warning to show after startup.
    ///
    /// # Returns
    ///
    /// A fully initialized application state.
    pub fn new(
        ctx: &egui::Context,
        mut settings: Settings,
        workspace_override: Option<String>,
        startup_warning: Option<String>,
    ) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::TRANSPARENT;
        ctx.set_visuals(visuals);
        ctx.global_style_mut(|style| {
            style.animation_time = 0.18;
            style.spacing.item_spacing = Vec2::new(8.0, 8.0);
            style.spacing.button_padding = Vec2::new(10.0, 6.0);
        });
        theme::install_fonts(ctx);
        let settings_baseline = settings.clone();
        let mut persisted_workspace_path = settings.workspace_path.clone();
        let mut workspace_failure = None;
        if let Some(path) = workspace_override.as_ref() {
            settings.workspace_path.clone_from(path);
        }
        let mut status = startup_warning;
        let workspace = match storage::resolve_launch_workspace(
            &settings.workspace_path,
            workspace_override.is_some(),
        ) {
            Ok(path) => path,
            Err(error) => {
                status = Some(format!(
                    "Workspace {} unavailable: {error}",
                    settings.workspace_path
                ));
                workspace_failure = Some(settings.workspace_path.clone());
                PathBuf::from(&settings.workspace_path)
            }
        };
        settings.workspace_path = workspace.to_string_lossy().into_owned();
        if workspace_override.is_none() && workspace_failure.is_none() {
            persisted_workspace_path.clone_from(&settings.workspace_path);
        }
        let (lists, fingerprint) = if workspace_failure.is_some() {
            (Vec::new(), WorkspaceFingerprint::default())
        } else {
            match storage::load_workspace(&workspace) {
                Ok(snapshot) => {
                    if !snapshot.warnings.is_empty() {
                        status = Some(snapshot.warnings.join("\n"));
                    }
                    (snapshot.lists, snapshot.fingerprint)
                }
                Err(error) => {
                    workspace_failure = Some(settings.workspace_path.clone());
                    status = Some(error);
                    (Vec::new(), WorkspaceFingerprint::default())
                }
            }
        };
        let initial = settings
            .last_list_id
            .filter(|id| lists.iter().any(|item| item.key == *id))
            .or_else(|| lists.first().map(|item| item.key));
        let screen = initial.map(Screen::List).unwrap_or(Screen::Overview);
        let view_zoom = 1.0;
        let displayed_palette = initial
            .and_then(|id| {
                lists
                    .iter()
                    .find(|item| item.key == id)
                    .map(|item| Palette::from_accent(item.data.accent))
            })
            .unwrap_or_else(|| Palette::from_accent(Accent::Charcoal));
        let workspace_input = settings.workspace_path.clone();
        let global_shortcut_input = if settings.global_shortcut.is_empty() {
            String::new()
        } else {
            crate::hotkey::label(&settings.global_shortcut)
        };

        let mut app = Self {
            settings,
            persisted_workspace_path,
            workspace,
            lists,
            manual_task_order: std::collections::HashMap::new(),
            fingerprint,
            screen,
            return_screen: None,
            settings_list_id: None,
            status,
            new_task: String::new(),
            request_task_focus: initial.is_some(),
            creating_list: false,
            new_list_title: String::new(),
            new_list_accent: Accent::default(),
            request_list_focus: false,
            editing: None,
            edit_text: String::new(),
            request_edit_focus: false,
            subtasks_for: None,
            new_subtask: String::new(),
            revealed: None,
            drag: None,
            overview_drag: None,
            overview_hover: None,
            history_open: false,
            workspace_input,
            rename_input: String::new(),
            rename_baseline: String::new(),
            delete_armed: false,
            next_scan: Instant::now() + SCAN_INTERVAL,
            last_pin_state: None,
            last_decorations_state: None,
            view_zoom,
            transition_direction: 1.0,
            displayed_palette,
            newest_task: None,
            clear_animation: None,
            delete_animation: None,
            quick_capture: None,
            find: None,
            shortcuts: None,
            spotlight: None,
            last_motion_tick: Instant::now(),
            hovered_title: None,
            tray: None,
            window_hidden: false,
            #[cfg(any(target_os = "windows", target_os = "linux", test))]
            taskbar_hidden: None,
            #[cfg(any(target_os = "windows", target_os = "linux", test))]
            next_taskbar_check: Instant::now(),
            instance: None,
            global_shortcut: None,
            global_shortcut_input,
            shortcut_error: None,
            task_context: None,
            #[cfg(target_os = "macos")]
            reopen: None,
            quitting: false,
            keyboard: KeyboardState::default(),
            persistence: persistence::PersistenceState::new(settings_baseline, workspace_failure),
            drafts: drafts::Drafts::default(),
            details: task_details::TaskDetailsState::default(),
            capture_draft: None,
            find_draft: None,
        };
        app.apply_list_order();
        app.capture_loaded_task_orders();
        app.apply_task_sorts();
        app
    }

    fn list_index(&self, id: Uuid) -> Option<usize> {
        self.lists.iter().position(|item| item.key == id)
    }

    fn target_palette(&self) -> Palette {
        let accent = match self.screen {
            Screen::List(id) => self.list_index(id).map(|i| self.lists[i].data.accent),
            Screen::Settings => self
                .settings_list_id
                .and_then(|id| self.list_index(id).map(|i| self.lists[i].data.accent)),
            Screen::Overview => None,
        };
        Palette::from_accent(accent.unwrap_or(Accent::Charcoal))
    }

    fn palette(&self) -> Palette {
        self.displayed_palette
    }

    fn apply_window_preferences(&mut self, ctx: &egui::Context) {
        if self.last_pin_state != Some(self.settings.always_on_top) {
            let level = if self.settings.always_on_top {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
            self.last_pin_state = Some(self.settings.always_on_top);
        }

        if self.last_decorations_state != Some(self.settings.window_decorations) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(
                self.settings.window_decorations,
            ));
            #[cfg(target_os = "macos")]
            ctx.send_viewport_cmd(egui::ViewportCommand::EnableButtons {
                close: true,
                minimized: false,
                maximize: true,
            });
            self.last_decorations_state = Some(self.settings.window_decorations);
        }
    }

    fn update_motion(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if let Some((_, started)) = &self.persistence.notice {
            let remaining = Duration::from_secs(3).saturating_sub(started.elapsed());
            if remaining.is_zero() {
                self.persistence.notice = None;
            } else {
                ctx.request_repaint_after(remaining);
            }
        }
        let elapsed = now
            .duration_since(self.last_motion_tick)
            .as_secs_f32()
            .min(0.05);
        self.last_motion_tick = now;

        let target_palette = self.target_palette();
        if self.displayed_palette != target_palette {
            let step = (1.0 - (-elapsed * 14.0).exp()).max(0.08);
            let next = self.displayed_palette.mix(target_palette, step);
            // Rounded channels can stop short of the target; finish instead of repainting forever.
            self.displayed_palette = if next == self.displayed_palette {
                target_palette
            } else {
                next
            };
            ctx.request_repaint();
        }

        let distance = 1.0 - self.view_zoom;
        if distance.abs() > 0.001 {
            let step = (1.0 - (-elapsed * 18.0).exp()).max(0.08);
            self.view_zoom += distance * step.max(0.08);
            if (1.0 - self.view_zoom).abs() < 0.002 {
                self.view_zoom = 1.0;
            }
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        if self.newest_task.is_some_and(|(_, started)| {
            started.elapsed().as_secs_f32() >= INSERT_ANIMATION_SECONDS + 0.04
        }) {
            self.newest_task = None;
        } else if self.newest_task.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        if let Some((list_id, started)) = self.clear_animation {
            if started.elapsed().as_secs_f32() >= CLEAR_ANIMATION_SECONDS {
                self.clear_animation = None;
                self.archive_completed(list_id);
            } else {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        }

        if let Some((list_id, task_id, started)) = self.delete_animation {
            if started.elapsed().as_secs_f32() >= DELETE_ANIMATION_SECONDS {
                self.delete_animation = None;
                self.remove_task(list_id, task_id);
            } else {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        }

        if self
            .spotlight
            .as_ref()
            .is_some_and(|spotlight| spotlight.started.elapsed().as_secs_f32() >= 1.4)
        {
            self.spotlight = None;
        } else if self.spotlight.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn view_presentation(&self) -> f32 {
        self.view_zoom.clamp(0.0, 1.0)
    }

    fn begin_transition(&mut self, direction: f32) {
        self.cancel_overview_drag();
        self.drag = None;
        self.hovered_title = None;
        self.view_zoom = 0.0;
        self.transition_direction = direction.signum();
        self.last_motion_tick = Instant::now();
    }

    fn go_to_overview(&mut self) {
        self.finish_inline_edit();
        self.stash_input_drafts();
        if let Screen::List(id) = self.screen {
            self.keyboard.overview_list = Some(id);
        }
        if !matches!(self.screen, Screen::Overview) {
            self.begin_transition(-1.0);
        }
        self.screen = Screen::Overview;
        self.history_open = false;
        self.revealed = None;
        self.subtasks_for = None;
        self.editing = None;
        self.focus_overview();
    }

    fn open_list(&mut self, id: Uuid) {
        self.finish_inline_edit();
        self.stash_input_drafts();
        self.keyboard.pending = Some(egui::Id::new(("new-task", id)));
        self.keyboard.overview_list = Some(id);
        self.begin_transition(1.0);
        if self.settings.last_list_id != Some(id) {
            self.settings.last_list_id = Some(id);
            self.save_settings();
        }
        self.screen = Screen::List(id);
        self.history_open = false;
        self.revealed = None;
        self.subtasks_for = None;
        self.editing = None;
        self.restore_task_draft(id);
        self.request_task_focus = true;
    }

    fn cycle_list(&mut self, current: Uuid, backwards: bool) {
        if self.lists.len() < 2 {
            return;
        }
        let Some(index) = self.list_index(current) else {
            return;
        };
        let next = if backwards {
            (index + self.lists.len() - 1) % self.lists.len()
        } else {
            (index + 1) % self.lists.len()
        };
        self.open_list(self.lists[next].key);
        self.transition_direction = if backwards { -1.0 } else { 1.0 };
    }

    fn open_settings(&mut self, list_id: Option<Uuid>) {
        if matches!(self.screen, Screen::Settings) {
            return;
        }
        self.keyboard.settings_return = self.navigation_return_focus();
        self.finish_inline_edit();
        let subtasks_for = self.subtasks_for;
        self.stash_input_drafts();
        self.switch_subtask_panel(subtasks_for);
        self.keyboard.pending = Some(list_id.map_or_else(
            || egui::Id::new("settings-workspace"),
            |id| egui::Id::new(("settings-list-name", id)),
        ));
        self.return_screen = Some(self.screen.clone());
        self.settings_list_id = list_id;
        self.rename_input = list_id
            .and_then(|id| self.list_index(id))
            .map(|index| self.lists[index].data.title.clone())
            .unwrap_or_default();
        self.rename_baseline.clone_from(&self.rename_input);
        self.workspace_input = self.settings.workspace_path.clone();
        self.global_shortcut_input = if self.settings.global_shortcut.is_empty() {
            String::new()
        } else {
            crate::hotkey::label(&self.settings.global_shortcut)
        };
        self.delete_armed = false;
        self.begin_transition(1.0);
        self.screen = Screen::Settings;
    }

    fn close_settings(&mut self) {
        if let Some(id) = self.settings_list_id {
            self.rename_list(id);
        }
        self.begin_transition(-1.0);
        self.screen = self.return_screen.take().unwrap_or(Screen::Overview);
        self.settings_list_id = None;
        if let Screen::List(id) = self.screen {
            self.restore_task_draft(id);
        }
        self.keyboard.pending = self.keyboard.settings_return.take();
    }

    fn scan_external_changes(&mut self) {
        if Instant::now() < self.next_scan
            || self.drag.is_some()
            || self.overview_drag.is_some()
            || self.task_context.is_some()
            || self.editing.is_some()
            || self.details.editor.is_some()
            || !self.persistence.dirty_lists.is_empty()
            || !self.persistence.pending_filenames.is_empty()
            || self.persistence.workspace_failure.is_some()
        {
            return;
        }
        self.next_scan = Instant::now() + SCAN_INTERVAL;
        let current = match storage::workspace_fingerprint(&self.workspace) {
            Ok(current) => current,
            Err(error) => {
                self.mark_workspace_unavailable(error);
                return;
            }
        };
        if current == self.fingerprint {
            return;
        }
        if let Err(error) = storage::require_list_files(&self.workspace) {
            self.mark_workspace_unavailable(error);
            return;
        }

        match storage::load_workspace(&self.workspace) {
            Ok(snapshot) => {
                let focus_neighbors = self.focus_neighbors();
                let mut lists = snapshot.lists;
                for failed_path in &snapshot.failed_paths {
                    if let Some(previous) = self
                        .lists
                        .iter()
                        .find(|list| &list.path == failed_path)
                        .cloned()
                        && !lists.iter().any(|list| list.key == previous.key)
                    {
                        lists.push(previous);
                    }
                }
                storage::sort_lists(&mut lists);
                self.cancel_overview_drag();
                self.clear_save_conflicts();
                self.lists = lists;
                self.apply_list_order();
                self.capture_loaded_task_orders_except(&snapshot.failed_paths);
                self.apply_task_sorts();
                self.fingerprint = snapshot.fingerprint;
                if !snapshot.warnings.is_empty() {
                    self.status = Some(snapshot.warnings.join("\n"));
                }

                match self.screen.clone() {
                    Screen::List(id) if self.list_index(id).is_none() => self.go_to_overview(),
                    Screen::Settings => {
                        if let Some(index) =
                            self.settings_list_id.and_then(|id| self.list_index(id))
                        {
                            if self.rename_input.trim() == self.rename_baseline.trim() {
                                self.rename_input.clone_from(&self.lists[index].data.title);
                                self.rename_baseline.clone_from(&self.rename_input);
                            }
                        } else {
                            self.settings_list_id = None;
                        }
                    }
                    Screen::Overview | Screen::List(_) => {}
                }
                self.repair_focus(&focus_neighbors);
            }
            Err(error) => self.mark_workspace_unavailable(error),
        }
    }

    /// Shows workspace recovery after the open folder disappears, becomes unreadable,
    /// or loses all of its list files.
    fn mark_workspace_unavailable(&mut self, error: String) {
        self.status = Some(format!(
            "Workspace {} unavailable: {error}",
            self.settings.workspace_path
        ));
        self.persistence.workspace_failure = Some(self.settings.workspace_path.clone());
    }

    fn switch_workspace(&mut self, raw: &str) {
        self.open_workspace(raw, true);
    }

    /// Opens a workspace; only an explicit choice may create a missing folder or
    /// replace the saved selection.
    ///
    /// # Arguments
    /// - `raw` - Requested workspace path.
    /// - `explicit` - Whether this is a user's folder choice rather than a retry of
    ///   the current selection, which may be a launch-only override.
    fn open_workspace(&mut self, raw: &str, explicit: bool) {
        if raw.trim().is_empty() {
            return;
        }
        if !self.finish_pending_changes() {
            return;
        }
        let resolved = if explicit {
            storage::normalize_workspace_path(raw)
        } else {
            storage::existing_workspace_path(raw)
        };
        let path = match resolved {
            Ok(path) => path,
            Err(error) => {
                self.status = Some(error);
                return;
            }
        };
        let next_instance = if self
            .instance
            .as_ref()
            .is_some_and(|guard| !guard.owns_workspace(&path))
        {
            let state = storage::window_state_path();
            match crate::instance::claim(
                &path,
                state.parent().unwrap_or_else(|| std::path::Path::new(".")),
                false,
            ) {
                Ok(crate::instance::LaunchOutcome::Primary(guard)) => Some(guard),
                Ok(crate::instance::LaunchOutcome::Existing) => {
                    self.status =
                        Some("That workspace is already open; its window has been shown.".into());
                    return;
                }
                Err(error) => {
                    self.status = Some(error);
                    return;
                }
            }
        } else {
            None
        };
        match storage::load_workspace(&path) {
            Ok(snapshot) => {
                if let Some(next) = next_instance {
                    if let Some(previous) = &self.instance {
                        next.inherit_repaint_context(previous);
                    }
                    self.instance = Some(next);
                }
                self.stash_input_drafts();
                self.stash_overlay_drafts();
                self.clear_save_conflicts();
                self.workspace = path;
                self.restore_overlay_drafts();
                let recovered = self.persistence.workspace_failure.take().is_some();
                self.details = task_details::TaskDetailsState::default();
                self.subtasks_for = None;
                self.editing = None;
                self.revealed = None;
                self.history_open = false;
                self.settings.workspace_path = self.workspace.to_string_lossy().into_owned();
                if explicit {
                    self.persisted_workspace_path = self.settings.workspace_path.clone();
                }
                self.workspace_input = self.settings.workspace_path.clone();
                self.lists = snapshot.lists;
                self.apply_list_order();
                self.capture_loaded_task_orders();
                self.apply_task_sorts();
                self.fingerprint = snapshot.fingerprint;
                if !snapshot.warnings.is_empty() {
                    self.status = Some(snapshot.warnings.join("\n"));
                } else if recovered {
                    self.status = None;
                }
                self.settings.last_list_id = self.lists.first().map(|item| item.key);
                self.save_settings();
                self.begin_transition(1.0);
                self.screen = self
                    .settings
                    .last_list_id
                    .map(Screen::List)
                    .unwrap_or(Screen::Overview);
                self.return_screen = None;
                self.settings_list_id = None;
                self.keyboard = KeyboardState::default();
                if let Screen::List(id) = self.screen {
                    self.restore_task_draft(id);
                }
                self.request_task_focus = matches!(self.screen, Screen::List(_));
            }
            Err(error) => self.status = Some(error),
        }
    }
}

impl eframe::App for MinimalistApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.finish_pending_changes();
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let minimized = frame
            .winit_window()
            .is_some_and(|window| window.is_minimized().unwrap_or(false));
        self.handle_window_actions(ctx, minimized);
        self.poll_global_shortcut(ctx);
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        self.update_taskbar_presence(frame.winit_window().map(AsRef::as_ref));
        self.apply_window_preferences(ctx);
        self.update_motion(ctx);
        self.scan_external_changes();
        ctx.request_repaint_after(SCAN_INTERVAL);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.flush_deferred_settings(ui.ctx());
        self.handle_app_shortcuts(ui.ctx());
        let palette = self.palette();
        {
            let style = ui.style_mut();
            let visuals = &mut style.visuals;
            visuals.override_text_color = Some(palette.text);
            visuals.weak_text_color = Some(palette.muted);
            visuals.selection.bg_fill = palette.accent.linear_multiply(0.32);
            visuals.selection.stroke = Stroke::new(1.0, palette.text);
            visuals.hyperlink_color = palette.accent;
            visuals.faint_bg_color = palette.surface;
            visuals.extreme_bg_color = palette.raised;
            visuals.text_edit_bg_color = Some(palette.raised);
            visuals.code_bg_color = palette.surface;
            visuals.panel_fill = Color32::TRANSPARENT;
            visuals.window_fill = palette.surface;
            visuals.window_stroke = Stroke::new(1.0, palette.raised);
            visuals.slider_trailing_fill = true;
            visuals.interact_cursor = Some(CursorIcon::PointingHand);

            visuals.widgets.noninteractive.bg_fill = palette.surface;
            visuals.widgets.noninteractive.weak_bg_fill = palette.surface;
            visuals.widgets.noninteractive.bg_stroke = Stroke::NONE;
            visuals.widgets.noninteractive.corner_radius = CornerRadius::same(10);
            visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text);

            visuals.widgets.inactive.bg_fill = palette.raised;
            visuals.widgets.inactive.weak_bg_fill = palette.surface;
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.raised);
            visuals.widgets.inactive.corner_radius = CornerRadius::same(10);
            visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.muted);

            visuals.widgets.hovered.bg_fill = palette.raised;
            visuals.widgets.hovered.weak_bg_fill = palette.raised;
            visuals.widgets.hovered.bg_stroke =
                Stroke::new(1.0, palette.accent.linear_multiply(0.55));
            visuals.widgets.hovered.corner_radius = CornerRadius::same(10);
            visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text);
            visuals.widgets.hovered.expansion = 0.5;

            visuals.widgets.active.bg_fill = palette.surface.lerp_to_gamma(palette.accent, 0.22);
            visuals.widgets.active.weak_bg_fill =
                palette.surface.lerp_to_gamma(palette.accent, 0.16);
            visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
            visuals.widgets.active.corner_radius = CornerRadius::same(10);
            visuals.widgets.active.fg_stroke = Stroke::new(1.0, palette.text);

            visuals.widgets.open = visuals.widgets.active;
        }

        let viewport = ui.max_rect();
        ui.painter().rect_filled(
            viewport,
            0.0,
            palette
                .background
                .linear_multiply(self.settings.background_opacity),
        );
        ui.painter().circle_filled(
            viewport.right_top() + Vec2::new(-54.0, 24.0),
            210.0,
            palette.accent.linear_multiply(0.045),
        );
        ui.painter().circle_filled(
            viewport.left_bottom() + Vec2::new(16.0, -12.0),
            150.0,
            palette.surface.linear_multiply(0.18),
        );

        let presentation = self.view_presentation();
        let eased = egui::emath::easing::cubic_out(presentation);
        let margin = 16.0 + (1.0 - eased) * 10.0;
        let offset = Vec2::new(
            (1.0 - eased) * 14.0 * self.transition_direction,
            (1.0 - eased) * 10.0,
        );
        let mut content_rect = viewport.shrink(margin).translate(offset);
        // A banner retry shows the reopened workspace next pass, like other navigation;
        // rendering it under that click would make egui surrender the entry's new focus.
        let workspace_available = self.persistence.workspace_failure.is_none();
        if self.status.is_some()
            || self.persistence.notice.is_some()
            || self.persistence.settings_dirty
            || !self.persistence.dirty_lists.is_empty()
            || !self.persistence.pending_filenames.is_empty()
            || !workspace_available
        {
            let mut status_ui = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt("save-status")
                    .max_rect(content_rect)
                    .layout(Layout::top_down(Align::LEFT)),
            );
            self.status_ui(&mut status_ui, palette);
            content_rect.min.y = status_ui.min_rect().bottom() + 8.0;
        }
        let mut content_ui = ui.new_child(
            egui::UiBuilder::new()
                .id(egui::Id::new("main-content"))
                .max_rect(content_rect)
                .layout(Layout::top_down(Align::LEFT)),
        );
        content_ui.set_opacity(0.14 + eased * 0.86);
        if self.quick_capture.is_some()
            || self.find.is_some()
            || self.shortcuts.is_some()
            || self.details.move_task.is_some()
            || self.task_context.is_some()
        {
            content_ui.disable();
        }
        let screen = self.screen.clone();
        if workspace_available || matches!(screen, Screen::Settings) {
            match screen {
                Screen::Overview => self.overview_ui(&mut content_ui, palette),
                Screen::List(id) => self.list_ui(&mut content_ui, id, palette),
                Screen::Settings => self.settings_ui(&mut content_ui, palette),
            }
        }

        self.keyboard_focus_ui(&mut content_ui, palette);
        self.overlay_ui(ui.ctx(), palette);
        self.task_operations_ui(ui.ctx());
        self.task_context_ui(ui.ctx());
        self.finish_keyboard_frame(ui.ctx());
    }
}

#[cfg(test)]
#[path = "app/app_tests.rs"]
mod tests;
