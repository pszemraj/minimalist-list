# Using Minimalist List

Shortcuts work while the application has keyboard focus. On macOS, use Command in place of Ctrl, except list switching uses Control on every platform. Home and End are Fn+Left and Fn+Right on MacBook keyboards; Page Up and Page Down are Fn+Up and Fn+Down.

Press `Ctrl+/` (`Cmd+/` on macOS) to show the in-app keyboard shortcut reference. Keyboard layouts that require Shift to type `/` are supported. Use Up/Down, Page Up/Page Down, or Home/End to scroll it. Escape or **Close** returns focus to the control you left. Quick add drafts and Find queries remain available underneath the reference.

<img src="../assets/shortcuts.jpg" alt="The keyboard shortcut reference on Windows" width="360">

## Manage lists

Press Backspace (`Delete` on macOS) from an empty task entry or a focused task row to open the overview, with the card you left focused. Backspace keeps deleting text while an entry or editor contains text, and holding it until the entry is empty does not navigate away. Close subtasks, History, or revealed row actions with Escape first. `Alt+Left` (`Cmd+[` on macOS), the back control, and double-clicking a list title also open the overview. Arrow keys move between list cards in the displayed grid; Enter or Space opens the focused list and puts the cursor in task entry.

`Ctrl+Shift+N` opens list creation from Lists, a list, or Settings. You can also select **+ New list**. New lists default to Charcoal, a muted grey palette. Enter a name, optionally choose an accent, and press Enter to create it. Down from the name focuses the selected accent; Left/Right moves between colors. Enter or Space chooses the color and returns to the name; the next Enter creates the list. Up returns from colors to the name without selecting. Escape cancels.

Open a list card's settings to rename, recolor, or delete it. **Delete list** requires activating **Confirm delete list** a second time; holding Enter or Space cannot confirm it. At least one list must remain.

<img src="../assets/lists.jpg" alt="Four example lists with accent colors, task previews, and completion counts" width="360">

## Capture and edit tasks

Add tasks through the [list entry field](../README.md#start-using-it). Subtask entry stays focused after Enter so you can add several items in succession.

Down from task entry focuses the first task. Up and Down move between task rows, including from their secondary controls; Home and End focus the first and last task. Up from the first task returns to entry. Enter on a task row edits its title; Enter saves and returns focus to that row, while Escape cancels the edit.

Right opens a task's checklist and focuses its first child, or its add field when empty. Up/Down moves through children and the add field; Down from an expanded parent enters its checklist. Up from the first child returns to its parent. Home selects the first child and End reaches the add field when a child control is focused. Enter or Space on a child's completion control toggles it. Left outside text, or Escape, closes the checklist and returns to its parent. Moving through an open checklist preserves its unfinished entry; Left/Right/Home/End inside the entry keep editing text.

`Ctrl+N` opens Quick add from any screen. Choose a destination list and press Enter to save one task, or Escape to cancel. The selector remembers the last successful destination. The header's `add` action opens the same overlay.

Click task text to edit it inline. Enter or leaving the field saves; Escape cancels. The checklist and delete controls on the right become prominent on hover and remain keyboard-focusable. Use the checklist control to add and complete subtasks.

## Complete, reorder, and remove tasks

Use the circle on the left to complete or reactivate a task. It is keyboard-focusable, so Space or Enter also toggles it.

Space on a focused task row toggles completion. `Alt+Up` and `Alt+Down` move that task one position (Option on macOS), keeping it focused. Moving past the first or last position leaves the list file untouched. Forward Delete on a focused task row removes it (`Fn+Delete` on Mac keyboards). Delete, Space, and Enter on task rows act once per press; arrow navigation and reordering repeat while held. These row commands do not apply while editing text.

Drag a task vertically to reorder it. Drag right past the threshold to complete or reactivate it. Drag left to reveal Edit and Delete, then drag right to close those actions. Opening Quick add, Find, or the shortcut reference cancels an unfinished drag.

`Ctrl+Shift+Backspace` (`Shift+Cmd+Delete` on macOS) or **Clear completed** moves completed tasks into the list's restorable History section. With no completed tasks, it leaves the list file untouched. Deleting a task removes it instead of archiving it.

Select **Show history**, then **Restore** to return a task to the bottom of the list as incomplete, with its subtasks intact.

Down from the last task reaches **Show history**, or Down from task entry when no tasks remain. Down, Right, or Enter on the History control opens it and focuses the first Restore action. Up/Down selects archived tasks; Enter or Space restores the focused task and focuses its active row. Up from the first archived task returns to the History control. Left or Escape closes History and returns to that control. Escape closes the focused History region first when a checklist is also open.

## Find and navigate

`Ctrl+F` or the header's `find` action searches task and subtask text across every list, including completed tasks and History. Use Up and Down to select a result, Enter to open it, and Escape to close the overlay. The selected task scrolls into view and is briefly highlighted.

<img src="../assets/find.jpg" alt="Find showing matching tasks from the Projects and Errands lists" width="360">

`Ctrl+Tab` and `Ctrl+Shift+Tab` move between lists, including on macOS, where Command+Tab switches applications.

Tab and Shift+Tab reach individual controls, including list-card settings, task completion, subtasks, delete actions, History restoration, and Settings. Focused controls are outlined and scroll into view. Left and Right retain their usual text-editing and slider-adjustment behavior.

Enter and Space activate controls once per press, including across focus changes and overlay dismissal. Typing spaces and holding editing or navigation keys retain their normal repeat behavior.

Escape dismisses one layer per press: an open dropdown, the shortcut reference, Quick add or Find, Settings, an inline edit, list creation, subtasks, revealed task actions, or History. Holding it does not dismiss further layers. It leaves Lists and the task-list screen open. Closing an overlay restores its opening control; opening a search result focuses the matching task, subtask, or History restore action. Screen navigation saves an active inline task edit, like leaving its field.

## Window and appearance settings

`Ctrl+,` opens Settings from Lists or a list, including layouts that require Shift to type a comma. Escape returns to the previous screen and restores focus. Applied preferences remain saved; leaving a list-name field applies its rename, including when `Ctrl+Shift+N` opens list creation. Closing Settings without editing the name preserves renames reloaded from disk. Typing a workspace path requires Enter or **Use typed path** to switch folders. Enter on an unchanged path preserves the current list and saved workspace selection.

In Settings, use Up/Down to move between rows and Left/Right to move within a row of choices. Enter or Space activates the focused control. Left/Right adjust a focused slider or move the cursor in a text field. Slider steps are 1 for text size and row spacing, and 1% for opacity. Page Up/Page Down jump between Workspace, Window, Appearance, and Current list, scrolling the focused control into view.

The header's `pin` control toggles always-on-top mode. When native window decorations are disabled, drag the title to move the window and use the visible `x` control to close it.

Typography settings control proportional or monospace text, weight, size, and row spacing. Existing typography preferences are retained; reduce row spacing to fit more tasks on screen.

**Long titles** has two modes: **Scroll** (the default) keeps one line, uses an ellipsis at rest, and scrolls horizontally after a short hover delay; **Wrap** shows the full title across as many lines as needed. Scrolling stops when you leave the title, edit, or drag.

**Background opacity** starts at 85% and can be adjusted from 20% to 100%. It affects background surfaces while task text and controls stay readable, including in pinned mode. Desktop transparency depends on the operating system's compositor.

Choose folders and launch-time overrides through [workspace settings](storage.md#workspace-location). Appearance preferences save automatically at the [local settings path](storage.md#default-directories).

## Startup and system tray

The [Windows and Linux installation scripts](../README.md#install-and-run) enable startup for the current user at sign-in. The startup shortcut passes `--start-hidden`; an ordinary launch opens the window. The flag can also be used with `--data-dir`. If a usable tray is unavailable at startup, the window opens so the app remains accessible.

Closing or minimizing the window saves any inline edit and hides it from the taskbar into the system tray. Open the tray icon's menu and choose **Show Minimalist List** to restore it or **Quit** to exit. Activating the icon also restores the window on desktops that support it. Shared-folder polling continues while hidden. Use the tray to reopen a running instance; launching another copy from the app menu starts a separate instance.

Linux uses X11 or XWayland for hiding and restoration when a desktop tray host is available, such as GNOME's AppIndicator extension or KDE's system tray. Without a usable tray, the app keeps the normal window backend and close/minimize behavior without a startup warning. If the tray host disappears while hidden, the window returns. Exiting saves inline edits and pending clear/delete actions.

To disable startup on Windows, remove **Minimalist List** from the Startup folder (`shell:startup` in Run). On Linux, remove `${XDG_CONFIG_HOME:-$HOME/.config}/autostart/minimalist-list.desktop`. Reinstalling enables startup again. macOS startup registration is not provided by these scripts.
