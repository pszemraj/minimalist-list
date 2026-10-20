//! Manual overview ordering, persisted separately for each workspace.

use super::super::{MinimalistApp, keyboard::FocusItem};
use crate::model::ListSort;
use eframe::egui;
use std::collections::HashMap;
use uuid::Uuid;

const EDGE_RECIPROCAL: f32 = 1.0 / 42.0;

/// Stable list identities participating in an overview drag.
pub(in crate::app) struct OverviewDrag {
    pub(in crate::app) list_id: Uuid,
    pub(in crate::app) target: Uuid,
    pub(in crate::app) origin: egui::Pos2,
    pub(in crate::app) started: bool,
}

impl MinimalistApp {
    /// Applies the current workspace's saved manual order, appending new lists.
    pub(in crate::app) fn apply_list_order(&mut self) {
        crate::storage::sort_lists(&mut self.lists);
        let workspace = self.workspace.to_string_lossy();
        if let Some(order) = self.settings.list_order.get(workspace.as_ref()) {
            let ranks: HashMap<_, _> = order
                .iter()
                .enumerate()
                .map(|(rank, id)| (*id, rank))
                .collect();
            self.lists
                .sort_by_key(|list| ranks.get(&list.key).copied().unwrap_or(usize::MAX));
        }
        match self.list_sort() {
            ListSort::Manual => {}
            ListSort::Title => self
                .lists
                .sort_by_cached_key(|list| list.data.title.to_lowercase()),
            ListSort::Newest => self
                .lists
                .sort_by_key(|list| std::cmp::Reverse(list.data.created_at_unix)),
            ListSort::Oldest => self.lists.sort_by_key(|list| list.data.created_at_unix),
        }
    }

    /// Returns this workspace's selected overview ordering mode.
    ///
    /// # Returns
    ///
    /// The stored view order, defaulting to Manual.
    pub(in crate::app) fn list_sort(&self) -> ListSort {
        self.settings
            .list_sort
            .get(self.workspace.to_string_lossy().as_ref())
            .copied()
            .unwrap_or_default()
    }

    /// Selects and saves a temporary overview order without replacing its manual arrangement.
    pub(in crate::app) fn set_list_sort(&mut self, mode: ListSort) {
        if self.list_sort() == mode {
            return;
        }
        let workspace = self.workspace.to_string_lossy().into_owned();
        self.settings
            .list_order
            .entry(workspace.clone())
            .or_insert_with(|| self.lists.iter().map(|list| list.key).collect());
        self.settings.list_sort.insert(workspace, mode);
        self.cancel_overview_drag();
        self.apply_list_order();
        self.save_settings();
        self.keyboard.pending = self
            .keyboard
            .overview_list
            .map(|list| FocusItem::Card(list).id());
    }

    /// Cancels overview drag and hover state when its screen or data changes.
    pub(in crate::app) fn cancel_overview_drag(&mut self) {
        self.overview_drag = None;
        self.overview_hover = None;
    }

    /// Moves one list to another list's position and saves the workspace order once.
    ///
    /// # Arguments
    ///
    /// - `source` - List being moved.
    /// - `target` - List occupying the destination slot.
    pub(in crate::app) fn reorder_list(&mut self, source: Uuid, target: Uuid) {
        if self.list_sort() != ListSort::Manual {
            return;
        }
        let (Some(source_index), Some(target_index)) =
            (self.list_index(source), self.list_index(target))
        else {
            return;
        };
        if source_index == target_index {
            return;
        }
        let list = self.lists.remove(source_index);
        self.lists.insert(target_index, list);
        self.settings.list_order.insert(
            self.workspace.to_string_lossy().into_owned(),
            self.lists.iter().map(|list| list.key).collect(),
        );
        self.save_settings();
        self.keyboard.overview_list = Some(source);
        self.keyboard.pending = Some(FocusItem::Card(source).id());
    }

    /// Tracks the nearest visible destination, paints it, and completes or cancels a drag.
    ///
    /// # Arguments
    ///
    /// - `ui` - Overview UI containing the scroll viewport.
    /// - `cards` - Stable list identities, card hit regions, and settings exclusions.
    /// - `palette` - Drop-indicator colors.
    ///
    /// # Returns
    ///
    /// Whether a drag owns this frame's pointer gesture, suppressing card activation.
    pub(in crate::app) fn finish_overview_drag(
        &mut self,
        ui: &mut egui::Ui,
        cards: &[(Uuid, egui::Rect, egui::Rect)],
        palette: super::Palette,
    ) -> bool {
        if !ui.is_enabled()
            || self.list_sort() != ListSort::Manual
            || !matches!(self.screen, super::super::Screen::Overview)
        {
            self.cancel_overview_drag();
            return false;
        }
        let viewport = ui.clip_rect();
        let threshold = ui
            .ctx()
            .options(|options| options.input_options.max_click_dist);
        let mut released = false;
        let mut event_pointer = None;
        // egui intentionally does not report drag_started when press/movement/
        // release arrive in one frame. Track the same native events in order so
        // short/coalesced drags still own their release and preserve source identity.
        for event in ui.input(|input| input.events.clone()) {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } => {
                    self.overview_drag = cards
                        .iter()
                        .find(|(_, rect, settings)| {
                            viewport.contains(pos)
                                && rect.contains(pos)
                                && !settings.contains(pos)
                                && ui.ctx().layer_id_at(pos) == Some(ui.layer_id())
                        })
                        .map(|(id, _, _)| OverviewDrag {
                            list_id: *id,
                            target: *id,
                            origin: pos,
                            started: false,
                        });
                }
                egui::Event::PointerMoved(pos)
                | egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                } => {
                    event_pointer = Some(pos);
                    if let Some(drag) = self.overview_drag.as_mut() {
                        drag.started |= drag.origin.distance(pos) > threshold;
                    }
                    if matches!(event, egui::Event::PointerButton { pressed: false, .. }) {
                        released = true;
                    }
                }
                _ => {}
            }
        }
        let Some(drag) = self.overview_drag.as_mut() else {
            return false;
        };
        if !cards.iter().any(|(id, _, _)| *id == drag.list_id) {
            self.cancel_overview_drag();
            return false;
        }
        if !drag.started {
            if released || !ui.input(|input| input.pointer.primary_down()) {
                self.cancel_overview_drag();
            }
            return false;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let pointer = event_pointer.or_else(|| ui.input(|input| input.pointer.interact_pos()));
        let hovered = pointer
            .filter(|point| viewport.contains(*point))
            .and_then(|point| {
                cards
                    .iter()
                    .filter(|(_, rect, _)| rect.intersects(viewport))
                    .min_by(|(_, a, _), (_, b, _)| {
                        a.center()
                            .distance_sq(point)
                            .total_cmp(&b.center().distance_sq(point))
                    })
            });
        if let Some((target, rect, _)) = hovered {
            drag.target = *target;
            ui.painter().rect_stroke(
                rect.shrink(2.0),
                16.0,
                egui::Stroke::new(3.0, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        if released {
            if let Some(drag) = self.overview_drag.take()
                && hovered.is_some()
            {
                self.reorder_list(drag.list_id, drag.target);
            }
            return true;
        }
        if !ui.input(|input| input.pointer.primary_down()) {
            self.cancel_overview_drag();
            return true;
        }
        if let Some(point) = pointer {
            let edge = 42.0;
            let direction = if point.y < viewport.top() + edge {
                ((viewport.top() + edge - point.y) * EDGE_RECIPROCAL).clamp(0.0, 1.0)
            } else {
                -((point.y - viewport.bottom() + edge) * EDGE_RECIPROCAL).clamp(0.0, 1.0)
            };
            if direction != 0.0 {
                ui.scroll_with_delta_animation(
                    egui::vec2(0.0, direction * 12.0),
                    egui::style::ScrollAnimation::none(),
                );
                ui.ctx().request_repaint();
            }
        }
        true
    }
}
