//! User-initiated recovery for separate files that share a list identity.

use super::{MinimalistApp, theme::Palette};
use crate::storage;
use eframe::egui::{self, Button, Color32, CornerRadius, Frame, Margin};
use std::time::Instant;

impl MinimalistApp {
    /// Identifies the next conflicting file and offers to import it as a separate list.
    ///
    /// # Arguments
    /// - `ui` - Status region above the main view.
    /// - `palette` - Current text colors.
    pub(super) fn duplicate_status_ui(&mut self, ui: &mut egui::Ui, palette: Palette) {
        let Some(path) = self.duplicate_paths.first().cloned() else {
            return;
        };
        let filename = path
            .file_name()
            .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy());
        let enabled = !self.has_modal();
        let mut recover = false;
        Frame::NONE
            .fill(Color32::from_rgba_unmultiplied(80, 25, 35, 245))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(self.rich(
                        "A conflicting copy may contain changes missing from your lists.",
                        13.0,
                        palette.text,
                    ))
                    .wrap(),
                );
                ui.add(
                    egui::Label::new(self.rich(filename.as_ref(), 13.0, palette.text)).truncate(),
                )
                .on_hover_text(filename.as_ref());
                if let Some(error) = self.duplicate_recovery_error.as_deref() {
                    ui.add(egui::Label::new(self.rich(error, 13.0, palette.text)).truncate())
                        .on_hover_text(error);
                }
                recover = ui
                    .add_enabled(enabled, Button::new("Recover as separate list"))
                    .clicked();
            });
        if !recover {
            return;
        }
        self.next_scan = Instant::now();
        match storage::recover_duplicate(&self.workspace, &path) {
            Ok(copy) => {
                self.duplicate_recovery_error = None;
                self.duplicate_paths.retain(|duplicate| duplicate != &path);
                self.lists.push(copy);
                self.apply_list_order();
                self.acknowledge("Recovered as a separate list.".into());
            }
            Err(error) => self.duplicate_recovery_error = Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{keyboard_frame, test_app};
    use std::fs;

    #[test]
    fn narrow_duplicate_warning_keeps_recovery_visible_and_imports_the_copy() {
        let (ctx, mut app) = test_app();
        let duplicate = app.lists[0].path.with_file_name(format!(
            "tasks--{} (desktop's conflicted copy).json",
            app.lists[0].key
        ));
        fs::copy(&app.lists[0].path, &duplicate).unwrap();
        app.next_scan = Instant::now();
        app.scan_external_changes();
        assert_eq!(app.duplicate_paths, std::slice::from_ref(&duplicate));
        app.status = Some("Unrelated warning".into());
        app.duplicate_recovery_error = Some("Previous recovery failed".into());

        let width = 260.0;
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 500.0));
        for _ in 0..2 {
            keyboard_frame(&ctx, &mut app, Vec::new(), width);
        }
        let output = keyboard_frame(&ctx, &mut app, Vec::new(), width);
        let text_rect = |label: &str| {
            let (clip, text) = output
                .shapes
                .iter()
                .find_map(|shape| {
                    if let egui::epaint::Shape::Text(text) = &shape.shape
                        && text.galley.text().contains(label)
                    {
                        Some((shape.clip_rect, text))
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| panic!("missing {label}"));
            let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
            assert!(clip.contains_rect(rect), "clipped {label}");
            assert!(viewport.contains_rect(rect), "offscreen {label}");
            rect
        };
        text_rect("missing from your lists.");
        let pos = text_rect("Recover as separate list").center();
        for pressed in [true, false] {
            let click = egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            keyboard_frame(
                &ctx,
                &mut app,
                vec![egui::Event::PointerMoved(pos), click],
                width,
            );
        }
        assert!(app.duplicate_paths.is_empty());
        assert_eq!(app.lists.len(), 2);
        assert!(!duplicate.exists());
        assert!(app.duplicate_recovery_error.is_none());
        assert_eq!(app.status.as_deref(), Some("Unrelated warning"));
        fs::remove_dir_all(storage::backups_dir(&app.workspace)).unwrap();
        fs::remove_dir_all(app.workspace).unwrap();
    }
}
