use crate::model::{Accent, ArchivedTask, FontChoice, Subtask, Task};
use crate::storage::{self, Settings, StoredList, WorkspaceFingerprint};
use eframe::egui;
use egui::viewport::WindowLevel;
use egui::{
    Align, Align2, Button, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Id, Key,
    Label, Layout, Margin, Pos2, Rect, RichText, ScrollArea, Sense, Stroke, TextEdit, Vec2,
};
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
const TASK_CLICK_DELAY: Duration = Duration::from_millis(310);

#[derive(Clone)]
enum Screen {
    Overview,
    List(Uuid),
    Focus,
    Settings,
}

#[derive(Clone)]
struct FocusState {
    list_id: Uuid,
    task_id: Uuid,
    remaining: f32,
    running: bool,
    finished: bool,
    last_tick: Instant,
}

#[derive(Clone)]
struct DragState {
    list_id: Uuid,
    task_id: Uuid,
    source: usize,
    target: usize,
    delta: Vec2,
}

#[derive(Clone, Copy, PartialEq)]
struct Palette {
    background: Color32,
    surface: Color32,
    raised: Color32,
    accent: Color32,
    text: Color32,
    muted: Color32,
    danger: Color32,
}

impl Palette {
    fn from_accent(accent: Accent) -> Self {
        match accent {
            Accent::Mint => Self::new((22, 38, 35), (31, 53, 48), (41, 67, 60), (137, 220, 187)),
            Accent::Charcoal => {
                Self::new((27, 29, 33), (39, 42, 47), (52, 56, 62), (200, 205, 215))
            }
            Accent::Crimson => Self::new((45, 24, 30), (65, 33, 41), (82, 43, 52), (235, 142, 159)),
            Accent::Sand => Self::new((43, 38, 28), (61, 53, 37), (78, 67, 46), (228, 194, 127)),
            Accent::Sky => Self::new((24, 35, 47), (34, 50, 67), (44, 64, 85), (137, 193, 235)),
            Accent::Lavender => {
                Self::new((35, 28, 46), (50, 40, 64), (65, 52, 82), (187, 158, 233))
            }
        }
    }

    fn new(
        background: (u8, u8, u8),
        surface: (u8, u8, u8),
        raised: (u8, u8, u8),
        accent: (u8, u8, u8),
    ) -> Self {
        Self {
            background: Color32::from_rgb(background.0, background.1, background.2),
            surface: Color32::from_rgb(surface.0, surface.1, surface.2),
            raised: Color32::from_rgb(raised.0, raised.1, raised.2),
            accent: Color32::from_rgb(accent.0, accent.1, accent.2),
            text: Color32::from_rgb(242, 244, 243),
            muted: Color32::from_rgb(158, 171, 166),
            danger: Color32::from_rgb(240, 116, 132),
        }
    }

    fn mix(self, other: Self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        Self {
            background: self.background.lerp_to_gamma(other.background, amount),
            surface: self.surface.lerp_to_gamma(other.surface, amount),
            raised: self.raised.lerp_to_gamma(other.raised, amount),
            accent: self.accent.lerp_to_gamma(other.accent, amount),
            text: self.text.lerp_to_gamma(other.text, amount),
            muted: self.muted.lerp_to_gamma(other.muted, amount),
            danger: self.danger.lerp_to_gamma(other.danger, amount),
        }
    }

    fn fade(self, opacity: f32) -> Self {
        let opacity = opacity.clamp(0.0, 1.0);
        Self {
            background: self.background.linear_multiply(opacity),
            surface: self.surface.linear_multiply(opacity),
            raised: self.raised.linear_multiply(opacity),
            accent: self.accent.linear_multiply(opacity),
            text: self.text.linear_multiply(opacity),
            muted: self.muted.linear_multiply(opacity),
            danger: self.danger.linear_multiply(opacity),
        }
    }
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
    focus: Option<FocusState>,
    next_scan: Instant,
    last_pin_state: Option<bool>,
    last_decorations_state: Option<bool>,
    last_fullscreen_state: Option<bool>,
    view_zoom: f32,
    transition_direction: f32,
    displayed_palette: Palette,
    newest_task: Option<(Uuid, Instant)>,
    clear_animation: Option<(Uuid, Instant)>,
    delete_animation: Option<(Uuid, Uuid, Instant)>,
    pending_task_click: Option<(Uuid, Uuid, Instant)>,
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
            focus: None,
            next_scan: Instant::now() + SCAN_INTERVAL,
            last_pin_state: None,
            last_decorations_state: None,
            last_fullscreen_state: None,
            view_zoom,
            transition_direction: 1.0,
            displayed_palette,
            newest_task: None,
            clear_animation: None,
            delete_animation: None,
            pending_task_click: None,
            last_motion_tick: Instant::now(),
        }
    }

    fn list_index(&self, id: Uuid) -> Option<usize> {
        self.lists.iter().position(|item| item.key == id)
    }

    fn target_palette(&self) -> Palette {
        let accent = match self.screen {
            Screen::List(id) => self.list_index(id).map(|i| self.lists[i].data.accent),
            Screen::Focus => self.focus.as_ref().and_then(|f| {
                self.list_index(f.list_id)
                    .map(|i| self.lists[i].data.accent)
            }),
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

        let fullscreen = self.settings.focus_fullscreen && matches!(self.screen, Screen::Focus);
        if self.last_fullscreen_state != Some(fullscreen) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(fullscreen));
            self.last_fullscreen_state = Some(fullscreen);
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

        if let Some((list_id, task_id, started)) = self.pending_task_click {
            let still_on_list = matches!(self.screen, Screen::List(id) if id == list_id);
            if !still_on_list {
                self.pending_task_click = None;
            } else if started.elapsed() >= TASK_CLICK_DELAY {
                self.pending_task_click = None;
                let task_is_active = self.list_index(list_id).is_some_and(|index| {
                    self.lists[index]
                        .data
                        .tasks
                        .iter()
                        .any(|task| task.id == task_id && !task.completed)
                });
                if task_is_active {
                    self.start_focus(list_id, task_id);
                }
            } else {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        }
    }

    fn view_presentation(&self) -> f32 {
        self.view_zoom.clamp(0.0, 1.0)
    }

    fn begin_transition(&mut self, direction: f32) {
        self.pending_task_click = None;
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
                    Screen::Focus => {
                        let focus_missing = self.focus.as_ref().is_none_or(|focus| {
                            self.list_index(focus.list_id).is_none_or(|index| {
                                !self.lists[index]
                                    .data
                                    .tasks
                                    .iter()
                                    .any(|task| task.id == focus.task_id)
                            })
                        });
                        if focus_missing {
                            self.focus = None;
                            self.go_to_overview();
                        }
                    }
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

    fn font_id(&self, size: f32) -> FontId {
        let family = match self.settings.font {
            FontChoice::Sans => FontFamily::Proportional,
            FontChoice::Mono => FontFamily::Monospace,
        };
        FontId::new(size, family)
    }

    fn rich(&self, text: impl Into<String>, size: f32, color: Color32) -> RichText {
        let mut rich = RichText::new(text).font(self.font_id(size)).color(color);
        if self.settings.bold_text {
            rich = rich.strong();
        }
        rich
    }

    fn flat_text_input(
        ui: &mut egui::Ui,
        id: Id,
        text: &mut String,
        hint: &str,
        font: FontId,
        palette: Palette,
    ) -> egui::Response {
        let shown = Frame::NONE
            .fill(palette.raised)
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.add(
                    TextEdit::singleline(text)
                        .id(id)
                        .font(font)
                        .hint_text(hint)
                        .desired_width(f32::INFINITY)
                        .frame(Frame::NONE),
                )
            });
        let rect = shown.response.rect;
        let response = shown.inner;
        let focus = ui.ctx().animate_bool_with_time_and_easing(
            id.with("focus"),
            response.has_focus(),
            0.16,
            egui::emath::easing::cubic_out,
        );
        ui.painter().rect_stroke(
            rect.shrink(0.5),
            12.0,
            Stroke::new(
                1.0 + focus,
                palette
                    .surface
                    .lerp_to_gamma(palette.accent, 0.24 + focus * 0.76),
            ),
            egui::StrokeKind::Inside,
        );
        response
    }

    fn toggle_row(
        ui: &mut egui::Ui,
        id: Id,
        label: &str,
        value: &mut bool,
        font: FontId,
        palette: Palette,
    ) -> bool {
        let mut response = ui.add_sized([ui.available_width(), 36.0], Button::new("").frame(false));
        if response.clicked() {
            *value = !*value;
            response.mark_changed();
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *value, label)
        });

        let hover = ui.ctx().animate_bool_with_time_and_easing(
            id.with("hover"),
            response.hovered(),
            0.14,
            egui::emath::easing::cubic_out,
        );
        let enabled = ui.ctx().animate_bool_with_time_and_easing(
            id.with("enabled"),
            *value,
            0.18,
            egui::emath::easing::cubic_out,
        );
        if hover > 0.0 {
            ui.painter().rect_filled(
                response.rect,
                10.0,
                palette.raised.linear_multiply(hover * 0.45),
            );
        }
        ui.painter().text(
            Pos2::new(response.rect.left() + 4.0, response.rect.center().y),
            Align2::LEFT_CENTER,
            label,
            font,
            palette.text.lerp_to_gamma(palette.accent, enabled * 0.12),
        );

        let track = Rect::from_center_size(
            Pos2::new(response.rect.right() - 23.0, response.rect.center().y),
            Vec2::new(42.0, 23.0),
        );
        ui.painter().rect(
            track,
            12.0,
            palette
                .surface
                .lerp_to_gamma(palette.accent, enabled * 0.76),
            Stroke::new(
                1.0,
                palette
                    .raised
                    .lerp_to_gamma(palette.accent, 0.20 + hover * 0.42),
            ),
            egui::StrokeKind::Inside,
        );
        let knob_x = egui::lerp((track.left() + 11.5)..=(track.right() - 11.5), enabled);
        let knob_center = Pos2::new(knob_x, track.center().y);
        ui.painter().circle_filled(
            knob_center,
            8.0 + hover * 0.4,
            palette
                .text
                .lerp_to_gamma(palette.background, enabled * 0.58),
        );
        if response.has_focus() {
            ui.painter().rect_stroke(
                response.rect.shrink(1.0),
                10.0,
                Stroke::new(1.0, palette.accent),
                egui::StrokeKind::Inside,
            );
        }

        response.changed()
    }

    fn surface_card<R>(
        ui: &mut egui::Ui,
        palette: Palette,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        let content_width = (ui.available_width() - 32.0).max(0.0);
        Frame::NONE
            .fill(palette.surface)
            .corner_radius(CornerRadius::same(16))
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_min_width(content_width);
                add_contents(ui)
            })
    }

    fn completion_control(
        &self,
        ui: &mut egui::Ui,
        response: egui::Response,
        completed: bool,
        progress: f32,
        palette: Palette,
    ) -> egui::Response {
        let label = if completed {
            "Mark task active"
        } else {
            "Complete task"
        };
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, completed, label)
        });

        let center = response.rect.center();
        let radius = response.rect.width().min(response.rect.height()) * 0.22;
        let hover = ui.ctx().animate_bool_with_time_and_easing(
            Id::new(("completion-hover", response.id)),
            response.hovered(),
            0.12,
            egui::emath::easing::cubic_out,
        );
        if hover > 0.0 {
            ui.painter().circle_filled(
                center,
                radius + 4.0 + hover,
                palette.raised.linear_multiply(hover),
            );
        }
        ui.painter().circle_stroke(
            center,
            radius,
            Stroke::new(
                1.8,
                palette.accent.lerp_to_gamma(palette.text, hover * 0.18),
            ),
        );
        if progress > 0.0 {
            let scale = 0.68 + progress * 0.32;
            ui.painter().circle_filled(
                center,
                radius * scale,
                palette.accent.linear_multiply(progress),
            );
            let check = palette.background.linear_multiply(progress);
            ui.painter().line_segment(
                [center + Vec2::new(-4.5, 0.0), center + Vec2::new(-1.2, 3.2)],
                Stroke::new(1.8, check),
            );
            ui.painter().line_segment(
                [center + Vec2::new(-1.2, 3.2), center + Vec2::new(5.0, -4.0)],
                Stroke::new(1.8, check),
            );
        }
        if response.has_focus() {
            ui.painter()
                .circle_stroke(center, radius + 4.0, Stroke::new(1.5, palette.text));
        }

        response.on_hover_text(label)
    }

    fn header(&mut self, ui: &mut egui::Ui, palette: Palette, title: &str, list_id: Option<Uuid>) {
        ui.horizontal(|ui| {
            if list_id.is_some()
                && ui
                    .add(Button::new(self.rich("<", 24.0, palette.text)).frame(false))
                    .clicked()
            {
                self.go_to_overview();
            }

            let title_response = ui
                .add(
                    Label::new(self.rich(title, 28.0, palette.text)).sense(Sense::click_and_drag()),
                )
                .on_hover_cursor(CursorIcon::Grab);
            if title_response.double_clicked() {
                self.go_to_overview();
            } else if title_response.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.settings.window_decorations
                    && ui
                        .add(Button::new(self.rich("x", 18.0, palette.muted)).frame(false))
                        .on_hover_text("Close")
                        .clicked()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui
                    .add(Button::new(self.rich("...", 18.0, palette.muted)).frame(false))
                    .on_hover_text("Settings")
                    .clicked()
                {
                    self.open_settings(list_id);
                }
                let pin = if self.settings.always_on_top {
                    "pinned"
                } else {
                    "pin"
                };
                if ui
                    .add(Button::new(self.rich(pin, 14.0, palette.accent)).frame(false))
                    .on_hover_text("Always on top")
                    .clicked()
                {
                    self.settings.always_on_top = !self.settings.always_on_top;
                    self.last_pin_state = None;
                    self.save_settings();
                }
            });
        });
    }

    fn overview_ui(&mut self, ui: &mut egui::Ui, palette: Palette) {
        self.header(ui, palette, "Lists", None);
        ui.add_space(12.0);
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let width = ui.available_width();
                let columns = if width > 760.0 {
                    3
                } else if width > 460.0 {
                    2
                } else {
                    1
                };
                let card_width = (width - 12.0 * (columns as f32 - 1.0)) / columns as f32;

                let cards: Vec<(Uuid, String, Accent, usize, usize, Vec<String>)> = self
                    .lists
                    .iter()
                    .map(|item| {
                        (
                            item.key,
                            item.data.title.clone(),
                            item.data.accent,
                            item.data.active_count(),
                            item.data.completed_count(),
                            item.data
                                .tasks
                                .iter()
                                .filter(|task| !task.completed)
                                .take(3)
                                .map(|task| task.text.clone())
                                .collect(),
                        )
                    })
                    .collect();

                for row in cards.chunks(columns) {
                    ui.horizontal(|ui| {
                        for (id, title, accent, active, completed, preview) in row {
                            let response = self.list_card(
                                ui, *id, title, *accent, *active, *completed, preview, card_width,
                            );
                            if response.clicked() {
                                self.open_list(*id);
                            }
                        }
                    });
                    ui.add_space(12.0);
                }

                let create_progress = ui.ctx().animate_bool_with_time_and_easing(
                    Id::new("new-list-panel"),
                    self.creating_list,
                    0.20,
                    egui::emath::easing::cubic_out,
                );
                if create_progress > 0.001 {
                    ui.add_space(8.0 * create_progress);
                    let input_font = self.font_id(18.0);
                    Frame::NONE
                        .fill(
                            palette
                                .surface
                                .lerp_to_gamma(palette.raised, (1.0 - create_progress) * 0.18),
                        )
                        .corner_radius(CornerRadius::same(16))
                        .inner_margin(Margin::same(14))
                        .show(ui, |ui| {
                            ui.set_opacity(create_progress);
                            ui.add_space((1.0 - create_progress) * 8.0);
                            let response = Self::flat_text_input(
                                ui,
                                Id::new("new-list-title"),
                                &mut self.new_list_title,
                                "List name",
                                input_font,
                                palette,
                            );
                            if self.request_list_focus {
                                response.request_focus();
                                self.request_list_focus = false;
                            }
                            ui.add_space(4.0);
                            ui.horizontal_wrapped(|ui| {
                                for accent in Accent::ALL {
                                    let p = Palette::from_accent(accent);
                                    let selected = accent == self.new_list_accent;
                                    let response = ui.add(
                                        Button::new(self.rich(accent.label(), 13.0, p.text))
                                            .fill(if selected { p.raised } else { p.surface })
                                            .stroke(Stroke::new(
                                                if selected { 2.0 } else { 0.0 },
                                                p.accent,
                                            ))
                                            .corner_radius(12.0),
                                    );
                                    if response.clicked() {
                                        self.new_list_accent = accent;
                                    }
                                }
                            });
                            ui.label(self.rich("Enter creates · Esc cancels", 12.0, palette.muted));
                            let (submit, cancel) = ui.input(|input| {
                                (
                                    input.key_pressed(Key::Enter)
                                        && (response.has_focus() || response.lost_focus()),
                                    input.key_pressed(Key::Escape),
                                )
                            });
                            if submit {
                                self.create_list();
                            } else if cancel {
                                self.new_list_title.clear();
                                self.creating_list = false;
                            }
                        });
                } else if ui
                    .add(Button::new(self.rich("+ New list", 18.0, palette.accent)).frame(false))
                    .clicked()
                {
                    self.creating_list = true;
                    self.request_list_focus = true;
                }
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn list_card(
        &self,
        ui: &mut egui::Ui,
        id: Uuid,
        title: &str,
        accent: Accent,
        active: usize,
        completed: usize,
        preview: &[String],
        width: f32,
    ) -> egui::Response {
        let card_palette = Palette::from_accent(accent);
        let (slot_rect, response) = ui.allocate_exact_size(Vec2::new(width, 166.0), Sense::click());
        let response = response.on_hover_cursor(CursorIcon::PointingHand);
        let hover = ui.ctx().animate_bool_with_time_and_easing(
            Id::new(("list-card-hover", id)),
            response.hovered(),
            0.18,
            egui::emath::easing::cubic_out,
        );
        let pressed = ui.ctx().animate_bool_with_time_and_easing(
            Id::new(("list-card-pressed", id)),
            response.is_pointer_button_down_on(),
            0.08,
            egui::emath::easing::quadratic_out,
        );
        let lift = hover * 3.5 - pressed * 1.5;
        let card_rect = slot_rect
            .shrink2(Vec2::new(2.0 + hover, 3.0 + hover))
            .translate(Vec2::new(0.0, -lift));

        let shadow = egui::epaint::Shadow {
            offset: [0, 5],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha((44.0 * hover) as u8),
        };
        ui.painter()
            .add(shadow.as_shape(card_rect, CornerRadius::same(18)));
        ui.painter().rect(
            card_rect,
            18.0,
            card_palette
                .surface
                .lerp_to_gamma(card_palette.raised, hover * 0.48),
            Stroke::new(
                1.0,
                card_palette.accent.linear_multiply(0.14 + hover * 0.46),
            ),
            egui::StrokeKind::Inside,
        );

        let content_rect = card_rect.shrink2(Vec2::new(18.0, 16.0));
        let mut card_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("list-card-content", id))
                .max_rect(content_rect)
                .layout(Layout::top_down(Align::LEFT)),
        );
        card_ui.set_opacity(0.88 + hover * 0.12);
        card_ui.add(Label::new(self.rich(title, 22.0, card_palette.text)).wrap());
        card_ui.add_space(8.0);
        for line in preview {
            card_ui.add(
                Label::new(self.rich(format!("— {line}"), 15.0, card_palette.muted)).truncate(),
            );
        }
        if preview.is_empty() {
            card_ui.label(self.rich("Empty", 15.0, card_palette.muted));
        }
        card_ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            ui.label(self.rich(
                format!("{active} active / {completed} done"),
                13.0,
                card_palette.accent,
            ));
        });

        response
    }

    fn create_list(&mut self) {
        let title = self.new_list_title.trim();
        if title.is_empty() {
            return;
        }
        match storage::create_list(&self.workspace, title, self.new_list_accent) {
            Ok(list) => {
                let id = list.key;
                let path = list.path.clone();
                self.lists.push(list);
                self.new_list_title.clear();
                self.creating_list = false;
                if let Err(error) =
                    storage::update_fingerprint_for_file(&mut self.fingerprint, &path)
                {
                    self.status = Some(error);
                }
                self.open_list(id);
            }
            Err(error) => self.status = Some(error),
        }
    }

    fn list_ui(&mut self, ui: &mut egui::Ui, list_id: Uuid, palette: Palette) {
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

        let (focus_new, cycle, clear_completed) = ui.input(|input| {
            (
                input.modifiers.command && input.key_pressed(Key::N),
                (input.modifiers.command && input.key_pressed(Key::Tab))
                    .then_some(input.modifiers.shift),
                input.modifiers.command
                    && input.modifiers.shift
                    && input.key_pressed(Key::Backspace),
            )
        });
        if focus_new {
            self.request_task_focus = true;
        }
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
        let progress_width = if task.progress().is_some() {
            58.0
        } else {
            18.0
        };
        let text_clip = Rect::from_min_max(
            Pos2::new(row_rect.left() + 44.0, row_rect.top()),
            Pos2::new(row_rect.right() - progress_width, row_rect.bottom()),
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
        if let Some((done, total)) = task.progress() {
            ui.painter().text(
                Pos2::new(row_rect.right() - 16.0, row_rect.center().y),
                Align2::RIGHT_CENTER,
                format!("{done}/{total}"),
                self.font_id(13.0),
                paint_palette.accent,
            );
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

            if response.double_clicked() {
                return Some(RowAction::Subtasks(list_id, task.id));
            }
            if response.clicked() && !revealed && !task.completed {
                return Some(RowAction::Focus(list_id, task.id));
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

    fn subtasks_panel(
        &mut self,
        ui: &mut egui::Ui,
        list_id: Uuid,
        task: &Task,
        palette: Palette,
        progress: f32,
    ) {
        let full_height = 66.0 + task.subtasks.len() as f32 * 36.0;
        let visible_height = (full_height * progress).max(1.0);
        ui.add_space(4.0 * progress);
        let (clip_rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), visible_height),
            Sense::hover(),
        );
        let content_rect = Rect::from_min_size(
            clip_rect.min + Vec2::new(0.0, -(1.0 - progress) * 10.0),
            Vec2::new(clip_rect.width(), full_height),
        );
        let mut panel_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("subtasks-panel-content", list_id, task.id))
                .max_rect(content_rect)
                .layout(Layout::top_down(Align::LEFT)),
        );
        panel_ui.set_clip_rect(clip_rect.intersect(ui.clip_rect()));
        panel_ui.set_opacity(progress);
        Frame::NONE
            .fill(palette.raised)
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(12))
            .show(&mut panel_ui, |ui| {
                for subtask in &task.subtasks {
                    ui.horizontal(|ui| {
                        let completion_progress = ui.ctx().animate_bool_with_time_and_easing(
                            Id::new(("subtask-complete", list_id, task.id, subtask.id)),
                            subtask.completed,
                            0.20,
                            egui::emath::easing::cubic_out,
                        );
                        let completion = ui.add_sized([26.0, 26.0], Button::new("").frame(false));
                        let completion = self.completion_control(
                            ui,
                            completion,
                            subtask.completed,
                            completion_progress,
                            palette,
                        );
                        if completion.clicked() {
                            self.toggle_subtask(list_id, task.id, subtask.id);
                        }
                        ui.label(
                            self.rich(
                                &subtask.text,
                                15.0,
                                palette
                                    .text
                                    .lerp_to_gamma(palette.muted, completion_progress),
                            ),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add(Button::new(self.rich("x", 14.0, palette.muted)).frame(false))
                                .clicked()
                            {
                                self.delete_subtask(list_id, task.id, subtask.id);
                            }
                        });
                    });
                }
                let subtask_response = ui.add(
                    TextEdit::singleline(&mut self.new_subtask)
                        .hint_text("Add subtask")
                        .desired_width(f32::INFINITY),
                );
                let submit = ui.input(|input| input.key_pressed(Key::Enter))
                    && (subtask_response.has_focus() || subtask_response.lost_focus());
                if submit {
                    self.add_subtask(list_id, task.id);
                    subtask_response.request_focus();
                }
            });
    }

    fn apply_row_action(&mut self, action: RowAction) {
        self.pending_task_click = None;
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
            RowAction::Focus(list_id, task_id) => {
                self.pending_task_click = Some((list_id, task_id, Instant::now()));
            }
        }
    }

    fn complete_task(&mut self, list_id: Uuid, task_id: Uuid) {
        if let Some(index) = self.list_index(list_id)
            && let Some(task) = self.lists[index]
                .data
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
            && !task.completed
        {
            task.toggle();
            self.save_list_index(index);
        }
    }

    fn add_subtask(&mut self, list_id: Uuid, task_id: Uuid) {
        let text = self.new_subtask.trim();
        if text.is_empty() {
            return;
        }
        if let Some(index) = self.list_index(list_id)
            && let Some(task) = self.lists[index]
                .data
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
        {
            task.subtasks.push(Subtask::new(text));
            self.new_subtask.clear();
            self.save_list_index(index);
        }
    }

    fn toggle_subtask(&mut self, list_id: Uuid, task_id: Uuid, subtask_id: Uuid) {
        if let Some(index) = self.list_index(list_id)
            && let Some(subtask) = self.lists[index]
                .data
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
                .and_then(|task| {
                    task.subtasks
                        .iter_mut()
                        .find(|subtask| subtask.id == subtask_id)
                })
        {
            subtask.completed = !subtask.completed;
            self.save_list_index(index);
        }
    }

    fn delete_subtask(&mut self, list_id: Uuid, task_id: Uuid, subtask_id: Uuid) {
        if let Some(index) = self.list_index(list_id)
            && let Some(task) = self.lists[index]
                .data
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
        {
            task.subtasks.retain(|subtask| subtask.id != subtask_id);
            self.save_list_index(index);
        }
    }

    fn begin_clear_completed(&mut self, list_id: Uuid) {
        if self.clear_animation.is_none() {
            self.revealed = None;
            self.clear_animation = Some((list_id, Instant::now()));
        }
    }

    fn begin_delete_task(&mut self, list_id: Uuid, task_id: Uuid) {
        if self.delete_animation.is_none() {
            self.revealed = None;
            self.subtasks_for = None;
            self.delete_animation = Some((list_id, task_id, Instant::now()));
        }
    }

    fn delete_progress(&self, list_id: Uuid, task_id: Uuid) -> f32 {
        self.delete_animation
            .map_or(0.0, |(active_list, active_task, started)| {
                if active_list != list_id || active_task != task_id {
                    0.0
                } else {
                    egui::emath::easing::cubic_in_out(
                        (started.elapsed().as_secs_f32() / DELETE_ANIMATION_SECONDS)
                            .clamp(0.0, 1.0),
                    )
                }
            })
    }

    fn remove_task(&mut self, list_id: Uuid, task_id: Uuid) {
        if let Some(index) = self.list_index(list_id) {
            self.lists[index]
                .data
                .tasks
                .retain(|task| task.id != task_id);
            self.save_list_index(index);
        }
    }

    fn clear_progress(&self, list_id: Uuid, completed: bool) -> f32 {
        if !completed {
            return 0.0;
        }
        self.clear_animation.map_or(0.0, |(active_list, started)| {
            if active_list != list_id {
                0.0
            } else {
                egui::emath::easing::cubic_in_out(
                    (started.elapsed().as_secs_f32() / CLEAR_ANIMATION_SECONDS).clamp(0.0, 1.0),
                )
            }
        })
    }

    fn archive_completed(&mut self, list_id: Uuid) {
        if let Some(index) = self.list_index(list_id) {
            let mut active = Vec::new();
            let mut completed = Vec::new();
            for task in self.lists[index].data.tasks.drain(..) {
                if task.completed {
                    completed.push(ArchivedTask::new(task));
                } else {
                    active.push(task);
                }
            }
            self.lists[index].data.tasks = active;
            self.lists[index].data.archive.splice(0..0, completed);
            self.save_list_index(index);
        }
    }

    fn history_ui(&mut self, ui: &mut egui::Ui, list_id: Uuid, palette: Palette, progress: f32) {
        let Some(index) = self.list_index(list_id) else {
            return;
        };
        let archived = self.lists[index].data.archive.clone();
        let full_height = 62.0 + archived.len() as f32 * 34.0;
        let visible_height = (full_height * progress).max(1.0);
        ui.add_space(8.0 * progress);
        let (clip_rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), visible_height),
            Sense::hover(),
        );
        let content_rect = Rect::from_min_size(
            clip_rect.min + Vec2::new(0.0, -(1.0 - progress) * 12.0),
            Vec2::new(clip_rect.width(), full_height),
        );
        let mut history_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("history-panel-content", list_id))
                .max_rect(content_rect)
                .layout(Layout::top_down(Align::LEFT)),
        );
        history_ui.set_clip_rect(clip_rect.intersect(ui.clip_rect()));
        history_ui.set_opacity(progress);
        Frame::NONE
            .fill(palette.surface)
            .corner_radius(CornerRadius::same(16))
            .inner_margin(Margin::same(14))
            .show(&mut history_ui, |ui| {
                ui.label(self.rich("History", 18.0, palette.text));
                for item in archived {
                    ui.horizontal(|ui| {
                        ui.label(
                            self.rich(item.task.text.clone(), 15.0, palette.muted)
                                .strikethrough(),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add(
                                    Button::new(self.rich("Restore", 13.0, palette.accent))
                                        .frame(false),
                                )
                                .clicked()
                            {
                                self.restore_archived(list_id, item.task.id);
                            }
                        });
                    });
                }
            });
    }

    fn restore_archived(&mut self, list_id: Uuid, task_id: Uuid) {
        if let Some(index) = self.list_index(list_id)
            && let Some(position) = self.lists[index]
                .data
                .archive
                .iter()
                .position(|item| item.task.id == task_id)
        {
            let mut task = self.lists[index].data.archive.remove(position).task;
            task.completed = false;
            task.completed_at_unix = None;
            self.lists[index].data.tasks.push(task);
            self.save_list_index(index);
        }
    }

    fn start_focus(&mut self, list_id: Uuid, task_id: Uuid) {
        self.focus = Some(FocusState {
            list_id,
            task_id,
            remaining: self.settings.focus_minutes.max(1) as f32 * 60.0,
            running: false,
            finished: false,
            last_tick: Instant::now(),
        });
        self.begin_transition(1.0);
        self.screen = Screen::Focus;
    }

    fn focus_ui(&mut self, ui: &mut egui::Ui, palette: Palette) {
        let Some(mut focus) = self.focus.clone() else {
            self.go_to_overview();
            return;
        };
        let elapsed = focus.last_tick.elapsed().as_secs_f32();
        focus.last_tick = Instant::now();
        if focus.running && !focus.finished {
            focus.remaining = (focus.remaining - elapsed).max(0.0);
            if focus.remaining <= 0.0 {
                focus.running = false;
                focus.finished = true;
            }
        }
        let task_text = self
            .list_index(focus.list_id)
            .and_then(|index| {
                self.lists[index]
                    .data
                    .tasks
                    .iter()
                    .find(|task| task.id == focus.task_id)
            })
            .map(|task| task.text.clone())
            .unwrap_or_else(|| "Task".to_owned());
        self.focus = Some(focus.clone());

        let mut go_back = false;
        ui.horizontal(|ui| {
            if ui
                .add(Button::new(self.rich("< Back", 17.0, palette.muted)).frame(false))
                .clicked()
            {
                go_back = true;
            }
            let title_response = ui
                .add(Label::new(self.rich("Focus", 18.0, palette.text)).sense(Sense::drag()))
                .on_hover_cursor(CursorIcon::Grab);
            if title_response.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.settings.window_decorations
                    && ui
                        .add(Button::new(self.rich("x", 18.0, palette.muted)).frame(false))
                        .on_hover_text("Close")
                        .clicked()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
        if go_back {
            self.open_list(focus.list_id);
            self.transition_direction = -1.0;
            self.focus = None;
            return;
        }

        ui.vertical_centered(|ui| {
            ui.add_space(42.0);
            ui.add(Label::new(self.rich(task_text, 26.0, palette.text)).wrap());
            ui.add_space(22.0);

            let total_seconds = self.settings.focus_minutes.max(1) as f32 * 60.0;
            let elapsed_progress = (1.0 - focus.remaining / total_seconds).clamp(0.0, 1.0);
            let (timer_rect, _) = ui.allocate_exact_size(Vec2::splat(252.0), Sense::hover());
            let center = timer_rect.center();
            let radius = 104.0;
            let pulse = if focus.running {
                let time = ui.input(|input| input.time) as f32;
                (time * 2.2).sin() * 0.5 + 0.5
            } else {
                0.0
            };
            ui.painter().circle_stroke(
                center,
                radius,
                Stroke::new(4.0, palette.surface.lerp_to_gamma(palette.raised, 0.5)),
            );
            if pulse > 0.0 {
                ui.painter().circle_stroke(
                    center,
                    radius + 7.0 + pulse * 2.0,
                    Stroke::new(2.0, palette.accent.linear_multiply(0.10 + pulse * 0.08)),
                );
            }
            if elapsed_progress > 0.0 {
                let start = -std::f32::consts::FRAC_PI_2;
                let sweep = std::f32::consts::TAU * elapsed_progress;
                let points = (0..=96)
                    .map(|step| {
                        let angle = start + sweep * step as f32 / 96.0;
                        center + Vec2::angled(angle) * radius
                    })
                    .collect::<Vec<_>>();
                ui.painter()
                    .add(egui::Shape::line(points, Stroke::new(4.0, palette.accent)));
            }

            let remaining = focus.remaining.ceil() as u32;
            let minutes = remaining / 60;
            let seconds = remaining % 60;
            ui.painter().text(
                center,
                Align2::CENTER_CENTER,
                format!("{minutes:02}:{seconds:02}"),
                self.font_id(62.0),
                palette.accent,
            );
            ui.painter().text(
                center + Vec2::new(0.0, 46.0),
                Align2::CENTER_CENTER,
                if focus.finished {
                    "session complete"
                } else if focus.running {
                    "stay with it"
                } else {
                    "ready"
                },
                self.font_id(13.0),
                palette.muted,
            );
            ui.add_space(18.0);
            let label = if focus.finished {
                "Done"
            } else if focus.running {
                "Pause"
            } else {
                "Start"
            };
            if ui
                .add(
                    Button::new(self.rich(label, 20.0, palette.text))
                        .fill(palette.surface)
                        .corner_radius(18.0)
                        .min_size(Vec2::new(150.0, 52.0)),
                )
                .clicked()
                && let Some(state) = &mut self.focus
            {
                if state.finished {
                    state.remaining = self.settings.focus_minutes.max(1) as f32 * 60.0;
                    state.finished = false;
                } else {
                    state.running = !state.running;
                }
                state.last_tick = Instant::now();
            }
            if focus.finished {
                ui.add_space(12.0);
                if ui
                    .add(Button::new(self.rich("Complete task", 17.0, palette.accent)).frame(false))
                    .clicked()
                {
                    self.complete_task(focus.list_id, focus.task_id);
                    self.open_list(focus.list_id);
                    self.transition_direction = -1.0;
                    self.focus = None;
                }
            }
        });
        ui.ctx().request_repaint_after(if focus.running {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(100)
        });
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, palette: Palette) {
        ui.horizontal(|ui| {
            if ui
                .add(Button::new(self.rich("<", 24.0, palette.text)).frame(false))
                .clicked()
            {
                self.close_settings();
            }
            let title_response = ui
                .add(
                    Label::new(self.rich("Settings", 28.0, palette.text))
                        .sense(Sense::click_and_drag()),
                )
                .on_hover_cursor(CursorIcon::Grab);
            if title_response.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.settings.window_decorations
                    && ui
                        .add(Button::new(self.rich("x", 18.0, palette.muted)).frame(false))
                        .on_hover_text("Close")
                        .clicked()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
        ui.add_space(18.0);

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| self.settings_contents(ui, palette));
    }

    fn settings_contents(&mut self, ui: &mut egui::Ui, palette: Palette) {
        let workspace_font = self.font_id(15.0);
        Self::surface_card(ui, palette, |ui| {
            ui.label(self.rich("Workspace", 18.0, palette.text));
            ui.label(self.rich(
                "One readable JSON file per list. Point this at Dropbox when wanted.",
                13.0,
                palette.muted,
            ));
            ui.add_space(5.0);
            let workspace_response = Self::flat_text_input(
                ui,
                Id::new("settings-workspace"),
                &mut self.workspace_input,
                "Workspace folder",
                workspace_font,
                palette,
            );
            ui.horizontal(|ui| {
                let use_folder = ui
                    .add(
                        Button::new(self.rich("Use this folder", 15.0, palette.accent))
                            .frame(false),
                    )
                    .clicked();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(self.rich("Enter applies", 12.0, palette.muted));
                });
                let enter = ui.input(|input| input.key_pressed(Key::Enter));
                if use_folder
                    || (enter
                        && (workspace_response.has_focus() || workspace_response.lost_focus()))
                {
                    self.switch_workspace();
                }
            });
        });

        ui.add_space(12.0);
        let toggle_font = self.font_id(15.0);
        let mut window_changed = false;
        Self::surface_card(ui, palette, |ui| {
            ui.label(self.rich("Window", 18.0, palette.text));
            ui.add_space(4.0);
            window_changed |= Self::toggle_row(
                ui,
                Id::new("settings-always-on-top"),
                "Always on top",
                &mut self.settings.always_on_top,
                toggle_font.clone(),
                palette,
            );
            window_changed |= Self::toggle_row(
                ui,
                Id::new("settings-window-decorations"),
                "Native window decorations",
                &mut self.settings.window_decorations,
                toggle_font.clone(),
                palette,
            );
            window_changed |= Self::toggle_row(
                ui,
                Id::new("settings-focus-fullscreen"),
                "Fullscreen focus timer",
                &mut self.settings.focus_fullscreen,
                toggle_font,
                palette,
            );
            ui.add_space(2.0);
            ui.label(self.rich(
                "Without decorations, drag the title to move the window and use x to close it.",
                12.0,
                palette.muted,
            ));
        });
        if window_changed {
            self.last_pin_state = None;
            self.last_decorations_state = None;
            self.last_fullscreen_state = None;
            self.save_settings();
        }

        ui.add_space(12.0);
        let mut appearance_changed = false;
        Self::surface_card(ui, palette, |ui| {
            ui.label(self.rich("Typography and focus", 18.0, palette.text));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for (choice, label) in [(FontChoice::Sans, "Sans"), (FontChoice::Mono, "Mono")] {
                    let selected = self.settings.font == choice;
                    let response = ui.add(
                        Button::new(self.rich(label, 14.0, palette.text))
                            .fill(if selected {
                                palette.surface.lerp_to_gamma(palette.accent, 0.34)
                            } else {
                                palette.raised
                            })
                            .stroke(Stroke::new(
                                if selected { 1.5 } else { 0.0 },
                                palette.accent,
                            ))
                            .corner_radius(12.0),
                    );
                    if response.clicked() && !selected {
                        self.settings.font = choice;
                        appearance_changed = true;
                    }
                }
                let selected = self.settings.bold_text;
                if ui
                    .add(
                        Button::new(self.rich("Bold", 14.0, palette.text))
                            .fill(if selected {
                                palette.surface.lerp_to_gamma(palette.accent, 0.34)
                            } else {
                                palette.raised
                            })
                            .stroke(Stroke::new(
                                if selected { 1.5 } else { 0.0 },
                                palette.accent,
                            ))
                            .corner_radius(12.0),
                    )
                    .clicked()
                {
                    self.settings.bold_text = !self.settings.bold_text;
                    appearance_changed = true;
                }
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(self.rich("Text size", 14.0, palette.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(self.rich(
                        format!("{:.0}", self.settings.font_size),
                        13.0,
                        palette.accent,
                    ));
                });
            });
            appearance_changed |= ui
                .add_sized(
                    [ui.available_width(), 18.0],
                    egui::Slider::new(&mut self.settings.font_size, 14.0..=30.0)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed();

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(self.rich("Row spacing", 14.0, palette.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(self.rich(
                        format!("{:.0}", self.settings.row_padding),
                        13.0,
                        palette.accent,
                    ));
                });
            });
            appearance_changed |= ui
                .add_sized(
                    [ui.available_width(), 18.0],
                    egui::Slider::new(&mut self.settings.row_padding, 4.0..=24.0)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed();

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(self.rich("Focus minutes", 14.0, palette.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(self.rich(
                        self.settings.focus_minutes.to_string(),
                        13.0,
                        palette.accent,
                    ));
                });
            });
            appearance_changed |= ui
                .add_sized(
                    [ui.available_width(), 18.0],
                    egui::Slider::new(&mut self.settings.focus_minutes, 1..=90)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed();
        });
        if appearance_changed {
            self.save_settings();
        }

        if let Some(list_id) = self.settings_list_id {
            ui.add_space(12.0);
            let rename_font = self.font_id(16.0);
            Self::surface_card(ui, palette, |ui| {
                ui.label(self.rich("Current list", 18.0, palette.text));
                ui.add_space(5.0);
                let rename_response = Self::flat_text_input(
                    ui,
                    Id::new(("settings-list-name", list_id)),
                    &mut self.rename_input,
                    "List name",
                    rename_font,
                    palette,
                );
                let enter = ui.input(|input| input.key_pressed(Key::Enter));
                if rename_response.lost_focus() || (enter && rename_response.has_focus()) {
                    self.rename_list(list_id);
                }
                ui.add_space(5.0);
                ui.horizontal_wrapped(|ui| {
                    for accent in Accent::ALL {
                        let accent_palette = Palette::from_accent(accent);
                        let selected = self
                            .list_index(list_id)
                            .is_some_and(|index| self.lists[index].data.accent == accent);
                        if ui
                            .add(
                                Button::new(self.rich(accent.label(), 13.0, accent_palette.text))
                                    .fill(if selected {
                                        accent_palette.raised
                                    } else {
                                        accent_palette.surface
                                    })
                                    .stroke(Stroke::new(
                                        if selected { 2.0 } else { 0.0 },
                                        accent_palette.accent,
                                    ))
                                    .corner_radius(12.0),
                            )
                            .clicked()
                        {
                            self.set_accent(list_id, accent);
                        }
                    }
                });
                ui.add_space(10.0);
                let text = if self.delete_armed {
                    "Confirm delete list"
                } else {
                    "Delete list"
                };
                if ui
                    .add(Button::new(self.rich(text, 14.0, palette.danger)).frame(false))
                    .clicked()
                {
                    if self.delete_armed {
                        self.delete_list(list_id);
                    } else {
                        self.delete_armed = true;
                    }
                }
            });
        }
    }

    fn rename_list(&mut self, list_id: Uuid) {
        let title = self.rename_input.trim();
        if title.is_empty() {
            return;
        }
        if let Some(index) = self.list_index(list_id)
            && self.lists[index].data.title != title
        {
            self.lists[index].data.title = title.to_owned();
            self.save_list_index(index);
        }
    }

    fn set_accent(&mut self, list_id: Uuid, accent: Accent) {
        if let Some(index) = self.list_index(list_id)
            && self.lists[index].data.accent != accent
        {
            self.lists[index].data.accent = accent;
            self.save_list_index(index);
        }
    }

    fn delete_list(&mut self, list_id: Uuid) {
        if self.lists.len() <= 1 {
            self.status = Some("At least one list must remain.".to_owned());
            return;
        }
        if let Some(index) = self.list_index(list_id) {
            let list = self.lists.remove(index);
            match storage::delete_list(&list) {
                Ok(()) => {
                    storage::remove_file_from_fingerprint(&mut self.fingerprint, &list.path);
                    self.settings.last_list_id = self.lists.first().map(|item| item.key);
                    self.save_settings();
                    self.go_to_overview();
                    self.return_screen = None;
                    self.settings_list_id = None;
                }
                Err(error) => {
                    self.status = Some(error);
                    self.lists.insert(index, list);
                }
            }
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

#[derive(Clone)]
enum RowAction {
    Toggle(Uuid, Uuid),
    Delete(Uuid, Uuid),
    BeginEdit(Uuid, Uuid, String),
    CommitEdit(Uuid, Uuid, String),
    Reorder(Uuid, Uuid, usize),
    Subtasks(Uuid, Uuid),
    Focus(Uuid, Uuid),
}

impl eframe::App for MinimalistApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_window_preferences(ctx);
        self.update_motion(ctx);
        self.scan_external_changes();
        ctx.request_repaint_after(SCAN_INTERVAL);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
            Screen::Focus => self.focus_ui(&mut content_ui, palette),
            Screen::Settings => self.settings_ui(&mut content_ui, palette),
        }
        self.status_ui(&mut content_ui, palette);
    }
}
