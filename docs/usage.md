# Using Minimalist List

## Capture and edit tasks

The field at the top of a list receives focus when the list opens. Enter adds the task at the top and returns focus to the field.

`Ctrl+N` opens Quick add from any screen while the application is focused. Choose a destination list and press Enter to save one task, or Escape to cancel. The selector remembers the last successful destination. The header's `add` action opens the same overlay.

Click task text to edit it inline. The checklist and delete controls on the right become prominent on hover and remain keyboard-focusable. Use the checklist control to add and complete subtasks.

## Complete, reorder, and remove tasks

Use the circle on the left to complete or reactivate a task. It is keyboard-focusable, so Space or Enter also toggles it.

Drag a task vertically to reorder it. Drag right past the threshold to complete or reactivate it. Drag left to reveal Edit and Delete, then drag right to close those actions.

`Ctrl+Shift+Backspace` or **Clear completed** moves completed tasks into the list's restorable History section. Deleting a task removes it instead of archiving it.

## Find and navigate

`Ctrl+F` or the header's `find` action searches task and subtask text across every list, including completed tasks and History. Use Up and Down to select a result, Enter to open it, and Escape to close the overlay. The selected task scrolls into view and is briefly highlighted.

Use the back control or double-click a list title to return to the overview. `Ctrl+Tab` and `Ctrl+Shift+Tab` move between lists. Each overview card has a settings action for renaming, recoloring, or deleting that list.

All shortcuts require the Minimalist List window to have keyboard focus.

## Window and appearance settings

The header's `pin` control toggles always-on-top mode. When native window decorations are disabled, drag the title to move the window and use the visible `x` control to close it.

Typography settings control proportional or monospace text, weight, size, and row spacing. Each list has its own accent color. User-facing settings save immediately to:

```text
${XDG_CONFIG_HOME:-$HOME/.config}/minimalist-list/settings.json
```

Workspace selection and launch-time overrides are described in [storage and sync](storage.md#workspace-location).
