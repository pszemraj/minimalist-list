# Using Minimalist List

Shortcuts work while the application has keyboard focus. On macOS, use Command in place of Ctrl, except list switching uses Control on every platform. Home and End are Fn+Left and Fn+Right on MacBook keyboards; Page Up and Page Down are Fn+Up and Fn+Down.

Press `Ctrl+/` (`Cmd+/` on macOS) to show the in-app keyboard shortcut reference. Keyboard layouts that require Shift to type `/` are supported. Use Up/Down, Page Up/Page Down, or Home/End to scroll it. Escape or **Close** returns focus to the control you left. Quick add drafts and Find queries remain available underneath the reference.

<img src="../assets/shortcuts.png" alt="The keyboard shortcut reference on Windows, showing Backspace and task navigation" width="360">

## Manage lists

Press Backspace (`Delete` on macOS) from an empty task entry or a focused task row to open the overview, with the card you left focused. Backspace keeps deleting text while an entry or editor contains text, and holding it until the entry is empty does not navigate away. Close subtasks, History, or revealed row actions with Escape first. `Alt+Left` (`Cmd+[` on macOS), the back control, and double-clicking a list title also open the overview. Arrow keys move between list cards in the displayed grid; Enter or Space opens the focused list and puts the cursor in task entry.

`Ctrl+Shift+N` opens list creation from Lists, a list, or Settings. You can also select **+ New list**. New lists default to Charcoal, a muted grey palette. Enter a name, optionally choose an accent, and press Enter or select **Create list**. **Cancel** or Escape closes the form. Down from the name focuses the selected accent; Left/Right moves between colors. Enter or Space chooses the color and returns to the name; the next Enter creates the list. Up returns from colors to the name without selecting.

Open **List options** from a list header or card to rename, recolor, or delete that list. **Delete list** shows its name and affected task and History counts, then requires a separate **Confirm delete list** activation. **Cancel deletion** backs out; holding Enter or Space cannot confirm it. At least one list must remain.

<img src="../assets/lists.png" alt="Four example lists with accent colors, task previews, and completion counts" width="360">

## Capture and edit tasks

Add tasks through the [list entry field](../README.md#start-using-it). Task and subtask entry keep focus after Enter, including an empty submission. Unfinished task and checklist entries stay with their originating workspace and list or task during the session, including after navigation or closing a checklist.

Down from task entry focuses the first task. Up and Down move between task rows, including from their secondary controls; Home and End focus the first and last task. Up from the first task returns to entry. Enter on a task row edits its title; Enter saves and returns focus to that row, while Escape cancels the edit.

Right opens a task's checklist and focuses its first child, or its add field when empty. Up/Down moves through children and the add field; Down from an expanded parent enters its checklist. Up from the first child returns to its parent. Home selects the first child and End reaches the add field when a child control is focused. Enter or Space on a child's completion control toggles it. Left outside text, or Escape, closes the checklist and returns to its parent. Moving through an open checklist preserves its unfinished entry; Left/Right/Home/End inside the entry keep editing text.

`Ctrl+N` or the header's `add` action opens Quick add. A new capture from an open list defaults to that list; Lists and Settings use the last successful destination. Choose a list and press Enter or **Add**; a successful capture names its destination. Switching between Quick add and Find preserves their drafts and destination. An existing capture draft keeps its chosen destination; Escape cancels that capture.

Click task or child text to edit it inline; Tab also reaches child text, where Enter or Space starts editing. Enter or leaving the editor saves; Escape cancels. Editing a child preserves its position and completion. Task editors save before navigating, archiving, hiding, or quitting. The checklist and delete controls on the right become prominent on hover and remain keyboard-focusable.

## Complete, reorder, and remove tasks

Use the circle on the left to complete or reactivate a task. It is keyboard-focusable, so Space or Enter also toggles it.

Space on a focused task row toggles completion. `Alt+Up` and `Alt+Down` move that task one position (Option on macOS), keeping it focused. Moving past the first or last position leaves the list file untouched. Forward Delete on a focused task row removes it (`Fn+Delete` on Mac keyboards). Delete, Space, and Enter on task rows act once per press; arrow navigation and reordering repeat while held. These row commands do not apply while editing text.

Drag a task vertically to reorder it. Drag right past the threshold to complete or reactivate it. Drag left to reveal Edit and Delete, then drag right to close those actions. Opening Quick add, Find, or the shortcut reference cancels an unfinished drag.

`Ctrl+Shift+Backspace` (`Shift+Cmd+Delete` on macOS) or **Archive completed** moves completed tasks into restorable History. With no completed tasks, it leaves the list file untouched.

Deleting a task or child shows **Undo**. Select it, or press `Ctrl+Z` (`Cmd+Z` on macOS) outside a text editor, to restore the latest deletion with its contents, completion, and original position. This recovery is available for the current workspace until another deletion, dismissal, or workspace switch; it is not saved across restarts. Whole-list deletion uses its separate confirmation.

Right-click a task to choose **Move to list**. Select a destination and **Move**; its identity, completion, checklist, and unfinished checklist draft follow it. A successful move names the destination.

Select **Show history** and activate an archived title to inspect its children without changing their state. **Restore** is separate: it returns the parent to the bottom of the list as incomplete, with child completion unchanged.

Down from the last task reaches **Show history**, or Down from task entry when no tasks remain. Down, Right, or Enter on the History control opens it and focuses the first Restore action. Up/Down selects archived tasks; Enter or Space restores the focused task and focuses its active row. Up from the first archived task returns to the History control. Left or Escape closes History and returns to that control. Escape closes the focused History region first when a checklist is also open.

## Find and navigate

`Ctrl+F` or the header's `find` action searches task and subtask text across every list, including completed tasks and History. Use Up and Down to select a result, Enter to open it, and Escape to close the overlay. The selected task scrolls into view and is briefly highlighted. Opening an archived child reveals its read-only History contents. Results show each child's own completion state, with History labeled separately. Reopening Find retains the query and selection; ordinary wheel or trackpad scrolling keeps its position.

<img src="../assets/find.png" alt="Find showing matching tasks from the Projects and Errands lists" width="360">

`Ctrl+Tab` and `Ctrl+Shift+Tab` move between lists, including on macOS, where Command+Tab switches applications. Each list remembers its own scroll position during the session.

Tab and Shift+Tab reach individual controls, including list-card settings, task completion, subtasks, delete actions, History restoration, and Settings. Focused controls are outlined and scroll into view. Left and Right retain their usual text-editing and slider-adjustment behavior.

Enter and Space activate controls once per press, including across focus changes and overlay dismissal. Typing spaces and holding editing or navigation keys retain their normal repeat behavior.

Escape dismisses one layer per press: an open dropdown, the shortcut reference, Quick add or Find, Settings, an inline edit, list creation, subtasks, revealed task actions, or History. Holding it does not dismiss further layers. It leaves Lists and the task-list screen open. Closing an overlay restores its opening control; opening a search result focuses the matching task, subtask, or History restore action. Screen navigation saves an active inline task edit, like leaving its field.

## Window and appearance settings

`Ctrl+,` opens global **Settings**, including layouts that require Shift to type a comma. Workspace, window, and appearance preferences apply to every list. **Current list options** opens the selected list's rename, accent, and deletion controls; **App settings** returns to global preferences. The list header and card open **List options** directly.

Escape returns to the previous screen and restores focus. Applied preferences remain saved. Leaving a list-name field, quitting, or switching workspaces applies its rename; an unchanged field preserves external renames. A typed workspace path requires Enter or **Use typed path**; a blank path does nothing, and an unchanged path preserves the current list.

In Settings, use Up/Down to move between rows and Left/Right to move within a row of choices. Enter or Space activates the focused control. Left/Right adjust a focused slider or move the cursor in a text field. Slider steps are 1 for text size and row spacing, and 1% for opacity. Page Up/Page Down jump between the current screen's sections, scrolling the focused control into view.

The header's `pin` control toggles always-on-top mode. Turn off **Show window title bar** to use the app's draggable title and visible `x` close control. Window size and position are saved for the next launch.

Typography settings control proportional or monospace text, real regular or bold faces, content size, and row spacing. Task titles, children, editors, History, and Find scale together. Reduce row spacing to fit more tasks on screen.

**Long titles** has two modes: **Scroll** (the default) keeps one line, uses an ellipsis at rest, and scrolls horizontally after a short hover delay; **Wrap** shows the full title across as many lines as needed. Scrolling stops when you leave the title, edit, or drag.

**Background opacity** starts at 85% and can be adjusted from 20% to 100%. It affects background surfaces while task text and controls stay readable, including in pinned mode. Desktop transparency depends on the operating system's compositor.

Choose folders and launch-time overrides through [workspace settings](storage.md#workspace-location). Appearance preferences save automatically at the [local settings path](storage.md#default-directories).

A visible save error keeps unsaved task changes in memory and offers **Retry save**. Restore access to the workspace and retry; the same changes are saved without resubmitting them. A failed Quick add keeps its text and destination with its own **Retry save** action.

## Startup and system tray

The [installation scripts](../README.md#install-and-run) enable startup for the current user at sign-in. The startup entry passes `--start-hidden`; an ordinary launch opens the window. The flag can also be used with `--data-dir`. If a usable tray is unavailable at startup, the window opens so the app remains accessible.

Closing or minimizing the window saves any inline edit and hides it in the system tray (the menu bar on macOS), removing its taskbar entry where applicable. Open the tray icon's menu and choose **Show Minimalist List** to restore it or **Quit** to exit. Activating the icon also restores the window on desktops that support it. Shared-folder polling continues while hidden. Launching the executable again for the same workspace and local settings asks the existing instance to show its window; duplicate `--start-hidden` launches stay quiet. Different workspaces can run separately.

Linux uses X11 or XWayland for hiding and restoration when a desktop tray host is available, such as GNOME's AppIndicator extension or KDE's system tray. Without a usable tray, the app keeps the normal window backend and close/minimize behavior without a startup warning. If the tray host disappears while hidden, the window returns. Exiting saves inline edits and accepted archive/delete actions. If task changes cannot be saved, the window stays available with **Retry save**; recover the folder and retry before quitting.

On macOS, reopening the running app bundle through Finder or `open` restores its existing window. The app uses the menu bar without a Dock icon, including while its window is open. The minimize button is disabled because a hidden minimized window would remain in the Dock; close hides the window instead. The installer places the app in `~/Applications` and a launch agent in `~/Library/LaunchAgents`. Hidden startup avoids requesting activation over another app. **Quit** exits for the remainder of the session; the launch agent does not keep restarting it.

To disable startup on Windows, remove **Minimalist List** from the Startup folder (`shell:startup` in Run). On Linux, remove `${XDG_CONFIG_HOME:-$HOME/.config}/autostart/minimalist-list.desktop`. On macOS, quit the app and remove `~/Library/LaunchAgents/io.github.pszemraj.minimalist-list.plist`. Reinstalling enables startup again.
