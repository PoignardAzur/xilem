# Text and tab focus: current design, problems, and future directions

<!-- Copyright 2025 the Xilem Authors -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

<div class="rustdoc-hidden">

> This file is intended to be read in rustdoc.
> Use `cargo doc --open --package masonry --no-deps` and open the `doc` module.

</div>

This document describes how text and tab focus currently work in Masonry, identifies problems and limitations in the current design, and sketches possible directions for improvement.


## Current design

### Core state

Focus state lives in `RenderRootState` (in `render_root.rs`) as a handful of fields:

- **`focused_widget: Option<WidgetId>`** -- The widget that currently receives text events (keyboard, IME, clipboard).
- **`next_focused_widget: Option<WidgetId>`** -- A staging field. Focus changes are requested during event passes and applied later during `run_update_focus_pass`.
- **`focused_path: Vec<WidgetId>`** -- The ancestor chain from the root to the focused widget. Used to propagate `ChildFocusChanged` and to set `has_focus_target` on every ancestor.
- **`focus_anchor: Option<WidgetId>`** -- The most recently clicked or focused widget. This is the starting point for Tab navigation: pressing Tab focuses the next focusable widget *after* the anchor in tree order.
- **`focus_fallback: Option<WidgetId>`** -- An optional widget that receives text events when nothing is focused. Set by the Masonry driver, not by widgets. The fallback is *not* considered focused and does not get `FocusChanged` events.
- **`window_focused: bool`** -- Whether the OS window has focus. Determines "active" vs "inactive" focus styling.
- **`is_ime_active: bool`** -- Whether an IME session is currently running (because the focused widget returned `true` from `accepts_text_input`).

### Widget-level declarations

The `Widget` trait has two focus-related methods, both returning `bool` and defaulting to `false`:

- **`accepts_focus()`** -- If true, Tab navigation can land on this widget. Widgets that return true include `Button`, `Checkbox`, `Slider`, `TextArea` (when editable), `Split`, `ScrollBar`, `Portal` (when not fully constrained), and `VirtualScroll`.
- **`accepts_text_input()`** -- If true, focusing the widget starts an IME session. Only `TextArea<true>` returns true.

Both values are **cached at widget creation** in `WidgetState` and treated as immutable for the widget's lifetime. The cached values feed into the `descendant_is_focusable` flag, which is propagated up the tree so that Tab navigation can skip entire subtrees with no focusable descendants.

### How focus changes

Focus changes in response to three triggers:

1. **Tab / Shift+Tab.** During `run_on_text_event_pass`, if a `Tab` key-down event is not already handled, Masonry calls `find_next_focusable(root, forward)`. This walks the widget tree in depth-first preorder (or reverse post-order for Shift+Tab), starting from the focus anchor, skipping subtrees where `descendant_is_focusable` is false, and returns the first widget with `accepts_focus == true`. The result is written to `next_focused_widget`.

2. **Pointer click.** In `run_on_pointer_event_pass`, a pointer-down event updates `focus_anchor` to the clicked widget. If the click target is *not* a descendant of the currently focused widget, `next_focused_widget` is set to `None`, which clears focus. Widgets that want to gain focus when clicked (e.g. `Button`, `TextArea`, `Slider`) call `ctx.request_focus()` inside their `on_pointer_event` handler, which sets `next_focused_widget` to themselves.

3. **Programmatic requests.** Any widget with an `EventCtx` or `MutateCtx` can call `request_focus()`, `set_focus(target)`, or `resign_focus()`. Accessibility events (`accesskit::Action::Focus` and `accesskit::Action::Blur`) also set or clear `next_focused_widget`.

### The update focus pass

All focus changes are staged into `next_focused_widget` and then *applied* during `run_update_focus_pass`, which runs as part of the rewrite passes after every event pass. This pass:

1. Validates that `next_focused_widget` still refers to an interactive (not disabled, not stashed, not removed) widget. If not, it clears it.
2. Handles IME transitions: if focus is moving away from a widget that had an active IME session, synthesizes an `Ime::Disabled` event to the old widget and sends `EndIme` to the platform. If the new widget also wants IME, sends `StartIme`.
3. Computes the new `focused_path` and diffs it against the old one. For every widget whose `has_focus_target` status changed, sends `Update::ChildFocusChanged`.
4. Sends `Update::FocusChanged(false)` to the old focused widget and `Update::FocusChanged(true)` to the new one.
5. Updates `focus_anchor` to the newly focused widget (if any).

### Tab navigation algorithm

The `find_next_focusable` function (`update.rs`) works as follows:

1. Look up the current `focus_anchor`. If it no longer exists, treat it as `None`.
2. If an anchor exists, temporarily "yank" it out of the search (set `accepts_focus = false` for forward navigation, or `descendant_is_focusable = false` for backward) so the search skips it.
3. Call `find_first_focusable` starting from the root, passing the path from the anchor up to the root as an "anchor path". The search descends into the anchor's subtree first, then searches siblings after the anchor, then their children, etc.
4. If nothing is found after the anchor, search the entire tree from the beginning (wrapping around).
5. Restore the anchor's original flags.

`find_first_focusable` recurses depth-first. For forward navigation, it checks the current node before its children (preorder); for backward, it checks children before the current node (reverse post-order). It exits early from any subtree where `descendant_is_focusable` is false.

### Focus in specific widgets

- **Button:** Requests focus on pointer down. When focused, Space and Enter trigger a press.
- **Checkbox:** Does *not* request focus on click (unlike Button). When focused, Space toggles.
- **TextArea:** Requests focus on pointer down. Handles keyboard navigation, text editing, IME composition, and clipboard. Blinks its cursor only while focused.
- **Slider:** Requests focus on pointer down. Arrow keys adjust the value; Home/End go to extremes.
- **Split:** Requests focus on pointer down on the draggable bar. Arrow keys adjust the split ratio.
- **ScrollBar, Portal:** Accept focus for keyboard scrolling (arrow keys, Page Up/Down, Home/End). Do not request focus on click.
- **VirtualScroll:** Accepts focus (self-described as "not carefully designed") to allow PageDown handling.


## Problems with the current design

### 1. Focus capabilities are immutable

`accepts_focus()` and `accepts_text_input()` are cached once at widget creation and never updated. This means a widget cannot dynamically become focusable or stop being focusable based on its state.

For example, a `TextArea` could reasonably toggle between read-only and editable modes at runtime, but the current design requires two separate generic instantiations (`TextArea<true>` and `TextArea<false>`) because the focus and IME declarations are baked in at creation time. If a text field needs to switch between read-only and editable, it would need to be removed and re-inserted into the tree.

This is a deliberate simplification (it avoids a class of invalidation bugs), but it will become increasingly limiting as more widgets need dynamic focusability.


### 2. "Text focus" conflates two concerns

The single `accepts_focus` flag is used for two different purposes:

- **Tab navigation target.** Buttons and checkboxes accept focus so users can Tab to them and activate them with the keyboard.
- **Text input recipient.** Text areas accept focus to receive keyboard and IME events.

These are meaningfully different. A button that is tab-focusable does not need IME integration, and a read-only text view might want to receive keyboard shortcuts (Ctrl+C to copy) without being an IME target. The current split between `accepts_focus` (Tab reachable) and `accepts_text_input` (IME enabled) partially addresses this, but the naming and conceptual model treat focus as primarily about text, which does not match how widgets actually use it.

Web browsers distinguish between "focusable" (the element can be focused) and "contenteditable" / `<input>` (the element accepts text input). The current Masonry model is closer to this than it might seem (with the two separate methods), but the naming ("text focus") and documentation frame it as primarily a text-input concern.


### 3. Focus anchor behavior is underspecified

The focus anchor (the starting point for Tab navigation) is updated when the user clicks a widget or when a widget gains focus. However, the behavior when the focus anchor is removed from the tree, stashed, or disabled is explicitly marked as "currently unspecified" in the documentation. This can lead to surprising Tab behavior: pressing Tab after a widget has been removed might start the search from an unexpected location, or fall back to searching the entire tree from the root.

The documentation suggests the closest interactive ancestor should become the new anchor, but this is not implemented.


### 4. Tab order is locked to tree order

Tab navigation follows depth-first preorder of the widget tree. There is no mechanism for:

- **Explicit tab indices** (like HTML's `tabindex`), to control the order in which widgets are visited.
- **Focus groups** or **focus scopes**, where Tab cycles within a group before escaping to the next group (e.g. a toolbar, a dialog, or a form).
- **Skip links** or **landmarks**, where certain widgets are reachable via keyboard shortcuts but excluded from the normal Tab cycle.

For simple layouts the tree order is fine, but for complex UIs (e.g. a toolbar with a text field, a dialog with multiple form controls, a sidebar with a list and a detail panel) the tree order may not match the expected visual or logical order.

This is arguably the biggest gap for accessibility compliance.


### 5. Focus trapping and restoration are not supported

When a modal dialog opens, focus should be trapped within it: Tab should cycle through the dialog's controls and not escape to the background. When the dialog closes, focus should return to whatever was focused before the dialog opened.

Neither of these behaviors is supported. The layer system (`create_layer`) could be a natural place to implement focus trapping, but currently layers have no special focus behavior.


### 6. Click-to-unfocus is coarse

When the user clicks outside the currently focused widget, focus is cleared if the click target is not a descendant of the focused widget. This check is purely structural (ancestor relationship in the widget tree), not semantic.

This means clicking on a *sibling* of a text field (say, a label next to it) clears focus from the text field, even if the label is logically associated with it. There is no way for a widget to declare "clicks on me should not steal focus from my sibling."


### 7. IME transitions have edge-case hacks

The `run_update_focus_pass` code includes a documented HACK for IME transitions. When focus moves away from a widget with an active IME session:

1. An `Ime::Disabled` event is synthesized and sent to the old widget.
2. This synthetic event may itself trigger a `request_focus` back to the old widget.
3. If focus ends up back on the same widget, IME must be re-enabled to avoid leaving it in an inconsistent state.

The code handles this, but it is fragile and relies on focus changes during the IME disable event not causing further cascading focus changes. The comment in the code notes that "IME events bubbling is not the correct behavior" and that a refactor is planned for when the project updates to use Android View's IME model.


### 8. VirtualScroll focus is known-broken

The `VirtualScroll` widget explicitly documents that its focus handling is "not carefully designed." It accepts focus to receive PageDown events, but:

- Focused items that scroll off-screen lose their widget (and thus their focus state), since `VirtualScroll` destroys off-screen widgets.
- Tab navigation across virtual items does not work because off-screen items do not exist in the tree.
- There is a commented-out `focused_item` field that hints at an unrealized design for preserving focus across scrolling.

This is a hard problem (it requires focus to be decoupled from widget existence), and it is one that the current architecture is not set up to solve.


### 9. No "arrow key" or "group" navigation

Tab moves focus through the entire tree. But in many UI patterns, arrow keys should move focus *within* a group of related widgets:

- Arrow keys in a radio button group move between options.
- Arrow keys in a menu move between items.
- Arrow keys in a toolbar move between buttons.
- Arrow keys in a grid move between cells.

There is currently no framework support for this. Each widget must implement its own arrow-key logic (as `Slider` does for value adjustment), but there is no way for a container to declare "my children form a group; arrow keys should move focus between them."

The WAI-ARIA "roving tabindex" and "activedescendant" patterns have no analog in Masonry.


### 10. Accessibility focus vs visual focus

Masonry builds an AccessKit accessibility tree in the accessibility pass, and AccessKit `Focus`/`Blur` actions can change focus. But the accessibility tree's notion of focus and Masonry's visual focus are the same thing: there is only one focused widget, and it is the same for both visual and accessibility purposes.

This is correct for most cases, but some accessibility patterns (like `aria-activedescendant`, where a composite widget reports a focused descendant to the accessibility tree without moving actual keyboard focus) cannot be expressed.


## Possible future directions

### Dynamic focusability

Allow `accepts_focus()` and `accepts_text_input()` to be changed after widget creation. This would require:

- Removing the "cached at creation" invariant.
- Adding an invalidation flag so that `update_focusable_pass` re-runs when a widget's focusability changes.
- Carefully handling the case where the currently focused widget stops accepting focus.

A lighter alternative: keep the cached values but allow widgets to set them via a context method (like `set_disabled` today), which would trigger the appropriate invalidation.


### Focus scopes

Introduce a concept of "focus scope" (similar to Qt's `QFocusProxy` or Compose's `FocusRequester` groups). A focus scope would:

- Contain a set of focusable widgets.
- Tab would cycle within the scope. When the end of the scope is reached, Tab would move to the next scope.
- A scope could be marked as "trapping", preventing Tab from leaving (for modal dialogs).
- A scope could have a "remembered focus" so that re-entering the scope (by Tabbing back in) restores the previously focused widget within it.

Focus scopes could be implemented as a widget property or as a special container widget.


### Tab index / focus order override

Allow widgets or containers to specify an explicit focus order, overriding tree order. This could be:

- A numeric `tab_index` property (like HTML, but hopefully with a less error-prone API).
- A declarative list of widget IDs specifying the Tab order within a container.
- A focus policy enum (`TabStop`, `SkipTab`, `TabFirst`, `TabLast`).


### Arrow-key group navigation

Provide a built-in mechanism for group navigation:

- A container marks itself as a "focus group" with a navigation mode (linear horizontal, linear vertical, grid, etc.).
- Arrow keys within a focus group move focus between the group's children.
- Tab exits the group and moves to the next focusable widget outside it.
- This is the "roving tabindex" pattern from WAI-ARIA.


### Focus restoration

When a transient UI element (dialog, popover, menu) closes, focus should return to the widget that was focused before the element opened. This could be built into the layer system:

- When a layer is created, save the current `focused_widget`.
- When the layer is destroyed, restore focus to the saved widget (if it is still interactive).


### Separate "input focus" from "selection focus"

Some widgets (like lists, trees, and grids) have a concept of a "selected item" that is separate from keyboard focus. The selected item might change with arrow keys, but the list widget itself retains focus. This is the `aria-activedescendant` pattern.

Supporting this would mean:

- A widget can report a "focused descendant" to the accessibility tree without moving Masonry's keyboard focus.
- The accessibility pass would need to be able to mark a different node as focused than the one Masonry considers focused.


### Rethinking the focus anchor

The focus anchor could be made more robust by:

- Falling back to the closest interactive ancestor when the anchor is removed/stashed/disabled (as the existing TODO suggests).
- Allowing containers to influence the anchor (e.g. a list widget could set the anchor to the selected item when it gains focus).
- Separating the "Tab starting point" concept from the "most recently clicked widget" concept, since they serve different purposes.
