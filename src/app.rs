mod list;
mod overlays;
mod overview;
mod settings;
mod task_details;
mod theme;

use self::overlays::{FindState, QuickCaptureState, Spotlight};
use self::theme::Palette;
use crate::model::Accent;
use crate::storage::{self, Settings, StoredList, WorkspaceFingerprint};
use eframe::egui;
use egui::viewport::WindowLevel;
use egui::{Align, Button, Color32, CornerRadius, CursorIcon, Frame, Layout, Margin, Stroke, Vec2};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uuid::Uuid;

const SCAN_INTERVAL: Duration = Duration::from_millis(800);
const SWIPE_COMPLETE: f32 = 78.0;
const SWIPE_ACTIONS: f32 = 64.0;
const ROW_HEIGHT: f32 = 58.0;
const CLEAR_ANIMATION_SECONDS: f32 = 0.24;
const DELETE_ANIMATION_SECONDS: f32 = 0.20;
const INSERT_ANIMATION_SECONDS: f32 = 0.28;

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
}

pub struct MinimalistApp {
    settings: Settings,
    persisted_workspace_path: String,
    workspace: PathBuf,
    lists: Vec<StoredList>,
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
    history_open: bool,

    workspace_input: String,
    rename_input: String,
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
    spotlight: Option<Spotlight>,
    last_motion_tick: Instant,
}

impl MinimalistApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        mut settings: Settings,
        workspace_override: Option<String>,
        startup_warning: Option<String>,
    ) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        cc.egui_ctx.global_style_mut(|style| {
            style.animation_time = 0.18;
            style.spacing.item_spacing = Vec2::new(8.0, 8.0);
            style.spacing.button_padding = Vec2::new(10.0, 6.0);
        });
        let mut persisted_workspace_path = settings.workspace_path.clone();
        if let Some(path) = workspace_override.as_ref() {
            settings.workspace_path.clone_from(path);
        }
        let mut status = startup_warning;
        let workspace = match storage::normalize_workspace_path(&settings.workspace_path) {
            Ok(path) => path,
            Err(error) => {
                status = Some(error);
                let fallback = storage::default_workspace_dir();
                storage::normalize_workspace_path(&fallback.to_string_lossy()).unwrap_or(fallback)
            }
        };
        settings.workspace_path = workspace.to_string_lossy().into_owned();
        if workspace_override.is_none() {
            persisted_workspace_path.clone_from(&settings.workspace_path);
        }
        let (lists, fingerprint) = match storage::load_workspace(&workspace) {
            Ok(snapshot) => {
                if !snapshot.warnings.is_empty() {
                    status = Some(snapshot.warnings.join("\n"));
                }
                (snapshot.lists, snapshot.fingerprint)
            }
            Err(error) => {
                status = Some(error);
                (Vec::new(), WorkspaceFingerprint::default())
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

        Self {
            settings,
            persisted_workspace_path,
            workspace,
            lists,
            fingerprint,
            screen,
            return_screen: None,
            settings_list_id: None,
            status,
            new_task: String::new(),
            request_task_focus: initial.is_some(),
            creating_list: false,
            new_list_title: String::new(),
            new_list_accent: Accent::Mint,
            request_list_focus: false,
            editing: None,
            edit_text: String::new(),
            request_edit_focus: false,
            subtasks_for: None,
            new_subtask: String::new(),
            revealed: None,
            drag: None,
            history_open: false,
            workspace_input,
            rename_input: String::new(),
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
            spotlight: None,
            last_motion_tick: Instant::now(),
        }
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

    fn save_settings(&mut self) {
        let mut persisted = self.settings.clone();
        persisted
            .workspace_path
            .clone_from(&self.persisted_workspace_path);
        if let Err(error) = storage::save_settings(&persisted) {
            self.status = Some(error);
        }
    }

    fn save_list_index(&mut self, index: usize) {
        let path = self.lists[index].path.clone();
        match storage::save_list(&self.lists[index]) {
            Ok(()) => {
                if let Err(error) =
                    storage::update_fingerprint_for_file(&mut self.fingerprint, &path)
                {
                    self.status = Some(error);
                }
            }
            Err(error) => self.status = Some(error),
        }
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
            self.last_decorations_state = Some(self.settings.window_decorations);
        }
    }

    fn update_motion(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let elapsed = now
            .duration_since(self.last_motion_tick)
            .as_secs_f32()
            .min(0.05);
        self.last_motion_tick = now;

        let target_palette = self.target_palette();
        if self.displayed_palette != target_palette {
            let step = (1.0 - (-elapsed * 14.0).exp()).max(0.08);
            self.displayed_palette = self.displayed_palette.mix(target_palette, step);
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
        self.view_zoom = 0.0;
        self.transition_direction = direction.signum();
        self.last_motion_tick = Instant::now();
    }

    fn go_to_overview(&mut self) {
        if !matches!(self.screen, Screen::Overview) {
            self.begin_transition(-1.0);
        }
        self.screen = Screen::Overview;
        self.history_open = false;
        self.revealed = None;
        self.subtasks_for = None;
        self.editing = None;
    }

    fn open_list(&mut self, id: Uuid) {
        self.begin_transition(1.0);
        self.settings.last_list_id = Some(id);
        self.save_settings();
        self.screen = Screen::List(id);
        self.history_open = false;
        self.revealed = None;
        self.subtasks_for = None;
        self.editing = None;
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
        self.return_screen = Some(self.screen.clone());
        self.settings_list_id = list_id;
        self.rename_input = list_id
            .and_then(|id| self.list_index(id))
            .map(|index| self.lists[index].data.title.clone())
            .unwrap_or_default();
        self.workspace_input = self.settings.workspace_path.clone();
        self.delete_armed = false;
        self.begin_transition(1.0);
        self.screen = Screen::Settings;
    }

    fn close_settings(&mut self) {
        self.begin_transition(-1.0);
        self.screen = self.return_screen.take().unwrap_or(Screen::Overview);
        self.settings_list_id = None;
    }

    fn scan_external_changes(&mut self) {
        if Instant::now() < self.next_scan || self.drag.is_some() || self.editing.is_some() {
            return;
        }
        self.next_scan = Instant::now() + SCAN_INTERVAL;
        let Ok(current) = storage::workspace_fingerprint(&self.workspace) else {
            return;
        };
        if current == self.fingerprint {
            return;
        }

        match storage::load_workspace(&self.workspace) {
            Ok(snapshot) => {
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
                self.lists = lists;
                self.fingerprint = snapshot.fingerprint;
                if !snapshot.warnings.is_empty() {
                    self.status = Some(snapshot.warnings.join("\n"));
                }

                match self.screen.clone() {
                    Screen::List(id) if self.list_index(id).is_none() => self.go_to_overview(),
                    Screen::Settings => {
                        if self
                            .settings_list_id
                            .is_some_and(|id| self.list_index(id).is_none())
                        {
                            self.settings_list_id = None;
                        }
                    }
                    Screen::Overview | Screen::List(_) => {}
                }
            }
            Err(error) => self.status = Some(error),
        }
    }

    fn switch_workspace(&mut self) {
        let path = match storage::normalize_workspace_path(&self.workspace_input) {
            Ok(path) => path,
            Err(error) => {
                self.status = Some(error);
                return;
            }
        };
        match storage::load_workspace(&path) {
            Ok(snapshot) => {
                self.workspace = path;
                self.settings.workspace_path = self.workspace.to_string_lossy().into_owned();
                self.persisted_workspace_path = self.settings.workspace_path.clone();
                self.workspace_input = self.settings.workspace_path.clone();
                self.lists = snapshot.lists;
                self.fingerprint = snapshot.fingerprint;
                if !snapshot.warnings.is_empty() {
                    self.status = Some(snapshot.warnings.join("\n"));
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
            }
            Err(error) => self.status = Some(error),
        }
    }

    fn status_ui(&mut self, ui: &mut egui::Ui, palette: Palette) {
        let Some(message) = self.status.clone() else {
            return;
        };
        ui.add_space(8.0);
        Frame::NONE
            .fill(Color32::from_rgba_unmultiplied(80, 25, 35, 230))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(self.rich(message, 13.0, palette.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(Button::new(self.rich("x", 16.0, palette.text)).frame(false))
                            .clicked()
                        {
                            self.status = None;
                        }
                    });
                });
            });
    }
}

impl eframe::App for MinimalistApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_window_preferences(ctx);
        self.update_motion(ctx);
        self.scan_external_changes();
        ctx.request_repaint_after(SCAN_INTERVAL);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
            visuals.panel_fill = palette.background;
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
        ui.painter().rect_filled(viewport, 0.0, palette.background);
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
        let margin = 22.0 + (1.0 - eased) * 10.0;
        let offset = Vec2::new(
            (1.0 - eased) * 14.0 * self.transition_direction,
            (1.0 - eased) * 10.0,
        );
        let content_rect = viewport.shrink(margin).translate(offset);
        let mut content_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("main-content")
                .max_rect(content_rect)
                .layout(Layout::top_down(Align::LEFT)),
        );
        content_ui.set_opacity(0.14 + eased * 0.86);
        let screen = self.screen.clone();
        match screen {
            Screen::Overview => self.overview_ui(&mut content_ui, palette),
            Screen::List(id) => self.list_ui(&mut content_ui, id, palette),
            Screen::Settings => self.settings_ui(&mut content_ui, palette),
        }
        self.status_ui(&mut content_ui, palette);
        self.overlay_ui(ui.ctx(), palette);
    }
}
