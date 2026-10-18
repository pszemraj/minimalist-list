//! EWMH taskbar state operations shared with platform-independent protocol tests.

/// Adds or removes only the taskbar hint, preserving every unrelated window state atom.
///
/// # Arguments
/// - `current` - Existing EWMH state atoms.
/// - `skip` - Server atom identifying the taskbar exclusion hint.
/// - `hidden` - Whether the taskbar exclusion hint should be present.
///
/// # Returns
/// The complete atom list for a window that has not yet been mapped.
pub(crate) fn merge_state(current: &[u32], skip: u32, hidden: bool) -> Vec<u32> {
    let mut state: Vec<_> = current
        .iter()
        .copied()
        .filter(|atom| hidden || *atom != skip)
        .collect();
    if hidden && !state.contains(&skip) {
        state.push(skip);
    }
    state
}

/// Builds the five EWMH client-message words using the application source indication.
///
/// # Arguments
/// - `skip` - Server atom identifying the taskbar exclusion hint.
/// - `hidden` - Whether to add rather than remove the hint.
///
/// # Returns
/// Action (remove=0, add=1), taskbar atom, no second atom, application source=1, and padding.
pub(crate) fn state_message(skip: u32, hidden: bool) -> [u32; 5] {
    [u32::from(hidden), skip, 0, 1, 0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adding_taskbar_hint_preserves_existing_pin_fullscreen_and_unknown_atoms() {
        let existing = [11, 22, 99];
        let hidden = merge_state(&existing, 33, true);
        assert_eq!(hidden, [11, 22, 99, 33]);
        assert_eq!(merge_state(&hidden, 33, true), hidden);
        assert_eq!(merge_state(&hidden, 33, false), existing);
    }

    #[test]
    fn removing_taskbar_hint_preserves_every_other_atom_and_handles_missing_property() {
        assert_eq!(merge_state(&[33, 11, 33, 22], 33, false), [11, 22]);
        assert_eq!(merge_state(&[11, 22], 33, false), [11, 22]);
        assert_eq!(merge_state(&[], 33, true), [33]);
        assert!(merge_state(&[], 33, false).is_empty());
    }

    #[test]
    fn mapped_window_requests_use_add_remove_and_application_source_not_toggle() {
        assert_eq!(state_message(33, true), [1, 33, 0, 1, 0]);
        assert_eq!(state_message(33, false), [0, 33, 0, 1, 0]);
    }
}
