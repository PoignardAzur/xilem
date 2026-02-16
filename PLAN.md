# Plan: RadioButton and RadioGroup Widgets

## Summary

Add `RadioButton` (masonry widget), `RadioGroup` (masonry widget), and corresponding xilem views
(`radio_button`, `radio_group`). Mutual exclusion (only one radio selected at a time) is handled at
the xilem view layer through standard state flow, consistent with masonry's "be dumb" philosophy.

---

## Design Overview

### Why the coordination belongs in xilem, not masonry

Masonry's architecture is deliberately "dumb" (see ARCHITECTURE.md): no reconciliation logic, no
behind-the-scenes dataflow. High-level logic belongs in downstream crates. This is exactly how
`Checkbox` already works — the widget emits a `CheckboxToggled` action but does **not** self-toggle;
the xilem view callback updates the app state, which triggers a rebuild that passes the new `checked`
value back down.

RadioButton follows the same pattern. The masonry widget emits `RadioButtonActivated` when clicked
but does not manage group coordination. The xilem `radio_button` view compares its own value to the
currently-selected value to derive its `checked` state. When the user selects a different button, the
callback updates app state, xilem rebuilds, and each `radio_button` gets its new `checked` flag.

Other approaches considered and rejected:

- **Event bubbling interception**: RadioGroup could intercept pointer events that bubbled from
  descendant RadioButtons. However, masonry actions (`submit_action`) bypass the widget tree entirely
  — they go directly to the signal sink. The container would only see raw pointer events, not
  semantic "this radio was selected" information. Reconstructing that logic in the container would
  duplicate RadioButton's click-handling code.

- **`mutate_later` from container to descendants**: `mutate_later` requires a `&mut WidgetPod<W>`
  reference to a *direct child*. A RadioGroup can't reach grand-children or deeper descendants.

- **Property-based cascading**: Masonry properties are per-widget with global defaults — there is no
  inherited/cascading property mechanism from ancestors to descendants.

- **Shared mutable state**: Goes against masonry's "no mutability tricks" principle.

The xilem-level approach is cleanest, requires no new masonry_core primitives, and follows the
established Checkbox pattern exactly.

### Architecture at a glance

```
┌─────────────────────── xilem view layer ───────────────────────┐
│                                                                │
│  radio_group(selected, on_select, children)                    │
│    ├─ radio_button("Option A", ValueA, selected, on_select)   │
│    ├─ radio_button("Option B", ValueB, selected, on_select)   │
│    └─ radio_button("Option C", ValueC, selected, on_select)   │
│                                                                │
│  Data flow:                                                    │
│    1. User clicks "Option B"                                   │
│    2. Masonry RadioButton emits RadioButtonActivated action    │
│    3. xilem radio_button view invokes callback(ValueB)         │
│    4. App state updates: selected = ValueB                     │
│    5. Rebuild: all radio_button views compare their value to   │
│       selected, call RadioButton::set_checked accordingly      │
│                                                                │
└────────────────────────────────────────────────────────────────┘

┌─────────────────────── masonry widget layer ───────────────────┐
│                                                                │
│  RadioGroup (Role::RadioGroup, wraps one child)                │
│    └─ Flex / Grid / any layout container                       │
│         ├─ RadioButton (Role::RadioButton, checked: bool)      │
│         ├─ RadioButton (Role::RadioButton, checked: bool)      │
│         └─ RadioButton (Role::RadioButton, checked: bool)      │
│                                                                │
│  RadioGroup is layout-transparent (like Passthrough).          │
│  RadioButton is a leaf interactive widget (like Checkbox).     │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

---

## Part 1: Masonry `RadioButton` Widget

**File:** `masonry/src/widgets/radio_button.rs`

### Structure

```rust
pub struct RadioButton {
    checked: bool,
    label: WidgetPod<Label>,
}

/// Action emitted when the radio button is activated.
///
/// Unlike checkboxes, radio buttons can only be activated (not deactivated) by clicking.
/// The bool is always `true`. Deselection happens when a different radio in the group
/// is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioButtonActivated;
```

### Key design decisions

- **Action type is unit-like (`RadioButtonActivated`)** rather than carrying a bool like
  `CheckboxToggled(bool)`. Radio buttons only emit when clicked — they don't toggle off. The xilem
  view layer knows which value was activated because each `radio_button` view has its own callback.

- **Visual rendering**: Circle outline (not rounded rectangle like Checkbox). When `checked`, a
  filled inner circle. Uses the same property set as Checkbox (Background, BorderColor, BorderWidth,
  etc.) plus new `RadioDotColor` and `DisabledRadioDotColor` properties (analogous to
  `CheckmarkColor` and `DisabledCheckmarkColor`).

- **`accepts_focus() -> true`**: Radio buttons are tab-focusable. Arrow keys for moving between
  radio buttons in a group can be a future enhancement.

### Widget trait implementation

Follows the Checkbox pattern closely:

| Method | Behavior |
|--------|----------|
| `on_pointer_event` | `Down` → capture pointer; `Up` while active+hovered → `submit_action(RadioButtonActivated)` |
| `on_text_event` | Space key up → `submit_action(RadioButtonActivated)` |
| `on_access_event` | `Action::Click` → `submit_action(RadioButtonActivated)` |
| `update` | Repaint on HoveredChanged, ActiveChanged, FocusChanged, DisabledChanged |
| `register_children` | Register label child |
| `measure` | Like Checkbox: circle indicator + gap + label |
| `layout` | Like Checkbox: position circle on left, label on right |
| `paint` | Draw circle outline; if checked, draw filled inner dot; focus/hover indicator |
| `accessibility_role` | `Role::RadioButton` |
| `accessibility` | Set `Toggled::True/False`, add `Action::Click` |
| `children_ids` | `[self.label.id()]` |

### WidgetMut API

```rust
impl RadioButton {
    pub fn set_checked(this: &mut WidgetMut<'_, Self>, checked: bool);
    pub fn set_text(this: &mut WidgetMut<'_, Self>, new_text: ArcStr);
    pub fn label_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, Label>;
}
```

### Properties

New properties (in `masonry/src/properties/`):

- `RadioDotColor` — color of the inner dot when checked (analogous to `CheckmarkColor`)
- `DisabledRadioDotColor` — dot color when disabled (analogous to `DisabledCheckmarkColor`)

These follow the same pattern as the existing `CheckmarkColor` property. The rest reuses existing
properties: `Background`, `ActiveBackground`, `DisabledBackground`, `BorderColor`,
`HoveredBorderColor`, `FocusedBorderColor`, `BorderWidth`, `CornerRadius`, `Padding`.

### Theme defaults

Add defaults to the theme module, matching the existing checkbox style but with circular geometry.

---

## Part 2: Masonry `RadioGroup` Widget

**File:** `masonry/src/widgets/radio_group.rs`

### Structure

```rust
pub struct RadioGroup {
    child: WidgetPod<dyn Widget>,
}
```

### Design

RadioGroup is a **transparent container** modeled after `Passthrough`:

- Wraps exactly one child widget (Flex, Grid, or any layout container)
- Adds no visual chrome, padding, or borders of its own
- Transparent to layout: delegates `measure` and `layout` directly to child
- Transparent to events: `type Action = NoAction`, `accepts_pointer_interaction() = false`
- Its sole purpose is **accessibility grouping**: `accessibility_role() = Role::RadioGroup`

This ensures screen readers announce "radio group" when the user navigates into the container, which
is important for accessibility compliance.

### Widget trait implementation

| Method | Behavior |
|--------|----------|
| `register_children` | Register single child |
| `measure` | `ctx.redirect_measurement(&mut self.child, ...)` (like Passthrough) |
| `layout` | `ctx.run_layout` + `ctx.place_child` at origin (like Passthrough) |
| `paint` | No-op (transparent) |
| `accessibility_role` | `Role::RadioGroup` |
| `accessibility` | No-op (the role is sufficient) |
| `children_ids` | `[self.child.id()]` |

### WidgetMut API

```rust
impl RadioGroup {
    pub fn set_child(this: &mut WidgetMut<'_, Self>, child: NewWidget<impl Widget + ?Sized>);
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget>;
}
```

---

## Part 3: Xilem `radio_button` View

**File:** `xilem_masonry/src/view/radio_button.rs`

### API

```rust
pub fn radio_button<T, F, State, Action>(
    label: impl Into<ArcStr>,
    value: T,
    selected: T,
    callback: F,
) -> RadioButton<T, State, Action, F>
where
    T: PartialEq + Send + Sync + 'static,
    F: Fn(Arg<'_, State>, T) -> Action + Send + 'static,
```

Usage:

```rust
fn view(state: &mut AppState) -> impl WidgetView<Edit<AppState>> {
    radio_group(
        flex((
            radio_button("Small", Size::Small, state.size, |s: &mut AppState, v| s.size = v),
            radio_button("Medium", Size::Medium, state.size, |s: &mut AppState, v| s.size = v),
            radio_button("Large", Size::Large, state.size, |s: &mut AppState, v| s.size = v),
        ))
    )
}
```

### View implementation

Follows the Checkbox view pattern:

- **`build`**: Creates `widgets::RadioButton::new(value == selected, label)` via
  `ctx.with_action_widget()`.
- **`rebuild`**: Compares `prev.value == prev.selected` to `self.value == self.selected`. If
  changed, calls `RadioButton::set_checked`. Also handles label/font/disabled changes.
- **`message`**: Downcasts to `RadioButtonActivated`, invokes
  `(self.callback)(app_state, self.value.clone())`.
- **`teardown`**: `ctx.teardown_action_source(element)`.

### Key design note

The `radio_button` view does **not** need to know about the RadioGroup container. Each radio_button
independently derives its `checked` state from `value == selected`. The group coordination is
implicit: all radio_buttons in a group share the same `selected` state reference, so when one
updates it, all others rebuild with the new value.

---

## Part 4: Xilem `radio_group` View

**File:** `xilem_masonry/src/view/radio_group.rs`

### API

```rust
pub fn radio_group<V, State, Action>(
    child: V,
) -> RadioGroupView<V, State, Action>
where
    V: WidgetView<State, Action>,
```

### View implementation

Simple wrapper analogous to how `button` wraps a child view:

- **`build`**: Creates `widgets::RadioGroup::new(child_widget)` via `ctx.with_id()`.
- **`rebuild`**: Rebuilds child view via `RadioGroup::child_mut`.
- **`message`**: Routes messages to child view.
- **`teardown`**: Tears down child.

This view is optional — users can use radio_buttons without a RadioGroup container if accessibility
grouping isn't needed. But it's recommended.

---

## Part 5: Module Registration & Exports

### masonry/src/widgets/mod.rs

Add:
```rust
mod radio_button;
mod radio_group;

pub use radio_button::{RadioButton, RadioButtonActivated};
pub use radio_group::RadioGroup;
```

### xilem_masonry/src/view/mod.rs

Add:
```rust
mod radio_button;
mod radio_group;

pub use radio_button::radio_button;
pub use radio_group::radio_group;
```

### masonry/src/properties/mod.rs

Add:
```rust
mod radio_dot_color;

pub use radio_dot_color::{RadioDotColor, DisabledRadioDotColor};
```

### Theme defaults

In `masonry/src/theme.rs` (or wherever theme defaults are registered), add default values for
`RadioDotColor` and `DisabledRadioDotColor` following the same pattern as
`CheckmarkColor`/`DisabledCheckmarkColor`.

---

## Part 6: Tests

### Unit tests (masonry)

**In `masonry/src/widgets/radio_button.rs`:**

1. **`simple_radio_button`** — Create unchecked radio, render snapshot, click, verify
   `RadioButtonActivated` action emitted, set checked, render snapshot of checked state.

2. **`radio_button_keyboard`** — Tab-focus to radio, press space, verify action emitted.

3. **`radio_button_focus_indicator`** — Verify focus ring renders correctly (screenshot test).

4. **`edit_radio_button`** — Verify `set_checked`, `set_text`, `label_mut` work correctly.
   Two-path rendering equivalence test (like `edit_checkbox`).

**In `masonry/src/widgets/radio_group.rs`:**

5. **`radio_group_layout`** — Verify RadioGroup is layout-transparent: child gets full size.

6. **`radio_group_accessibility`** — Verify RadioGroup has `Role::RadioGroup`.

7. **`radio_group_mutual_exclusion`** — Integration test: RadioGroup with Flex containing multiple
   RadioButtons. Click one, verify action. Use `edit_widget` to update checked states. Verify only
   one is checked at a time (checked through accessibility nodes).

### Xilem tests

Standard view tests following the checkbox view test patterns, if they exist. At minimum, ensure the
view builds, rebuilds on state change, and routes messages correctly.

---

## Implementation Order

1. `RadioDotColor` + `DisabledRadioDotColor` properties
2. Theme defaults for the new properties
3. `RadioButton` masonry widget + tests
4. `RadioGroup` masonry widget + tests
5. `radio_button` xilem view
6. `radio_group` xilem view
7. Integration example (optional, in `xilem/examples/`)

---

## Open Questions / Future Work

- **Arrow key navigation**: In native UIs, arrow keys move selection between radio buttons in a
  group. This requires the RadioGroup to intercept keyboard events during bubbling and manage focus.
  This is out of scope for the initial implementation but worth noting as future work.

- **RadioButton without label**: The Checkbox widget has a FIXME comment about removing its label
  child and being just a box with a checkmark. RadioButton could follow the same eventual direction
  — the label could be a separate widget alongside the radio indicator, with a click handler that
  delegates to the radio button.

- **`radio_button` view with generic child**: Like the `button` view which accepts a child view
  instead of just a string label, `radio_button` could accept an arbitrary child view. Initial
  implementation uses a string label for simplicity.
