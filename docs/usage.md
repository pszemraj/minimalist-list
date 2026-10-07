# Using Minimalist List

Shortcuts work while the application has keyboard focus. On macOS, use Command in place of Ctrl.

## Manage lists

Use the back control or double-click a list title to open the overview. Select **+ New list**, enter a name, optionally choose an accent, and press Enter to create it. Escape cancels.

Open a list card's settings to rename, recolor, or delete it. **Delete list** requires a second click on **Confirm delete list**; at least one list must remain.

## Capture and edit tasks

Add tasks through the [list entry field](../README.md#start-using-it). Subtask entry stays focused after Enter so you can add several items in succession.

`Ctrl+N` opens Quick add from any screen. Choose a destination list and press Enter to save one task, or Escape to cancel. The selector remembers the last successful destination. The header's `add` action opens the same overlay.

Click task text to edit it inline. Enter or leaving the field saves; Escape cancels. The checklist and delete controls on the right become prominent on hover and remain keyboard-focusable. Use the checklist control to add and complete subtasks.

## Complete, reorder, and remove tasks

Use the circle on the left to complete or reactivate a task. It is keyboard-focusable, so Space or Enter also toggles it.

Drag a task vertically to reorder it. Drag right past the threshold to complete or reactivate it. Drag left to reveal Edit and Delete, then drag right to close those actions.

`Ctrl+Shift+Backspace` or **Clear completed** moves completed tasks into the list's restorable History section. Deleting a task removes it instead of archiving it.

Select **Show history**, then **Restore** to return a task to the bottom of the list as incomplete, with its subtasks intact.

## Find and navigate

`Ctrl+F` or the header's `find` action searches task and subtask text across every list, including completed tasks and History. Use Up and Down to select a result, Enter to open it, and Escape to close the overlay. The selected task scrolls into view and is briefly highlighted.

`Ctrl+Tab` and `Ctrl+Shift+Tab` move between lists.

## Window and appearance settings

The header's `pin` control toggles always-on-top mode. When native window decorations are disabled, drag the title to move the window and use the visible `x` control to close it.

Typography settings control proportional or monospace text, weight, size, and row spacing. Existing typography preferences are retained; reduce row spacing to fit more tasks on screen.

**Long titles** has two modes: **Scroll** (the default) keeps one line, uses an ellipsis at rest, and scrolls horizontally after a short hover delay; **Wrap** shows the full title across as many lines as needed. Scrolling stops when you leave the title, edit, or drag.

**Background opacity** starts at 85% and can be adjusted from 20% to 100%. It affects background surfaces while task text and controls stay readable, including in pinned mode. Desktop transparency depends on the operating system's compositor.

Choose folders and launch-time overrides through [workspace settings](storage.md#workspace-location). Appearance preferences save automatically at the [local settings path](storage.md#default-directories).
