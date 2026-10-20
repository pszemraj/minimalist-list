//! Preserves text-editor input that arrived before an application shortcut.

use super::*;

fn needs_current_editor_input(event: &Event) -> bool {
    if matches!(event, Event::Copy) {
        return true;
    }
    let Event::Key {
        key,
        pressed: true,
        repeat,
        modifiers,
        ..
    } = event
    else {
        return false;
    };
    (*key == Key::Escape && !repeat && modifiers.matches_exact(Modifiers::NONE))
        || (*key == Key::C && modifiers.matches_exact(Modifiers::COMMAND | Modifiers::SHIFT))
        || [
            SHOW_SHORTCUTS,
            OPEN_SETTINGS,
            QUICK_ADD,
            FIND,
            CLOSE_WINDOW,
            NEW_LIST,
            NEXT_LIST,
            PREVIOUS_LIST,
            BACK_TO_LISTS,
        ]
        .iter()
        .any(|binding| {
            *key == binding.logical_key && modifiers.matches_logically(binding.modifiers)
        })
}

impl MinimalistApp {
    /// Lets the current text widget process earlier input before routing a later shortcut.
    ///
    /// # Arguments
    /// - `ctx` - Context containing the ordered event batch.
    ///
    pub(super) fn prepare_editor_event_order(&mut self, ctx: &egui::Context) {
        if !self.keyboard.deferred_events.is_empty() {
            let mut events = std::mem::take(&mut self.keyboard.deferred_events);
            ctx.input_mut(|input| {
                events.append(&mut input.events);
                input.events = events;
            });
        }
        // Escape clears egui focus at pass start, before app routing sees the
        // event. The preceding widget identity still identifies its TextEdit.
        let focused = ctx
            .memory(|memory| memory.focused())
            .or(self.keyboard.last_focus);
        let Some(focused) = focused.filter(|id| egui::TextEdit::load_state(ctx, *id).is_some())
        else {
            return;
        };
        let boundary = ctx.input(|input| {
            let mut editing_input = false;
            for (index, event) in input.events.iter().enumerate() {
                if needs_current_editor_input(event) {
                    return editing_input.then_some(index);
                }
                editing_input |= matches!(
                    event,
                    Event::Text(_)
                        | Event::Paste(_)
                        | Event::Cut
                        | Event::Copy
                        | Event::Ime(_)
                        | Event::Key { pressed: true, .. }
                );
            }
            None
        });
        let Some(boundary) = boundary else {
            return;
        };
        self.keyboard.deferred_events = ctx.input_mut(|input| input.events.split_off(boundary));
        ctx.memory_mut(|memory| {
            memory.move_focus(FocusDirection::None);
            memory.request_focus(focused);
        });
        ctx.request_repaint();
    }
}
