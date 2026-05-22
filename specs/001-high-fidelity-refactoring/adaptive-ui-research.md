# Adaptive UI Research: GTK4/Libadwaita Responsive Design

**Date**: 2026-05-22
**Sources**: GNOME Libadwaita docs, gtk4-rs book, Context7 MCP, GNOME HIG

## Key Finding: AdwLeaflet Is Deprecated

**AdwLeaflet has been deprecated** as of Libadwaita 1.4. It is replaced by a more powerful, declarative breakpoint-based system.

### Replacements

| Deprecated Widget | Modern Replacement | Purpose |
|---|---|---|
| `AdwLeaflet` | `AdwNavigationSplitView` + `AdwNavigationView` | Sidebar/collapsible panes |
| `AdwFlap` | `AdwOverlaySplitView` | Overlay sidebars |
| `AdwSqueezer` | Plain `GtkBox` + `AdwBreakpoint` orientation switch | Adaptive box orientation |

---

## Architecture: The Modern Adaptive Stack

The recommended widget hierarchy for a responsive Libadwaita application:

```
AdwApplicationWindow
  └── AdwToolbarView           # Main layout manager for bars + content
      ├── child type="top"
      │   └── AdwHeaderBar     # Title, controls, view switcher
      │       └── title-widget → AdwViewSwitcher  (wide layout)
      ├── content              # Main content area
      │   └── AdwNavigationSplitView  (or AdwNavigationView for page stacks)
      │       ├── sidebar → AdwNavigationPage
      │       └── content → AdwNavigationPage
      └── child type="bottom"
          └── AdwViewSwitcherBar  (narrow layout, hidden by default)
```

Plus breakpoints to toggle between wide/narrow states.

---

## Core Widgets: Detailed Reference

### 1. AdwToolbarView

**Purpose**: Replaces manual `GtkBox` for managing header bars and bottom bars. Manages flat/opaque styling, animated bar reveal/hide, and content extension behind bars.

**Rust API** (from gtk4-rs / libadwaita-rs):

```rust
use adw::prelude::*;
use adw::{ToolbarView, HeaderBar};

let toolbar = ToolbarView::new();
let header = HeaderBar::new();
let content = /* your main content widget */;

toolbar.add_top_bar(&header);
toolbar.set_content(Some(&content));
```

**Key Properties**:
- `top-bar-style` / `bottom-bar-style`: `Flat` (default), `Raised`
- `extend-content-to-top-edge` / `extend-content-to-bottom-edge`: bool
- Content extends behind bars when `true`

### 2. AdwHeaderBar

**Purpose**: Standard window header with start/center/end slots. Doubles as window drag handle.

**Rust API**:

```rust
use adw::HeaderBar;
use gtk::Button;

let header = HeaderBar::new();

// Add buttons to start (left in LTR)
let back_btn = Button::with_label("Back");
header.pack_start(&back_btn);

// Title widget (center) - set to AdwViewSwitcher for tabs
header.set_title_widget(Some(&view_switcher));

// Add buttons to end (right in LTR)
let menu_btn = Button::new();
header.pack_end(&menu_btn);
```

**HIG Rules**:
- **Start**: Primary actions (back, new, add)
- **Center**: Window title or `AdwViewSwitcher`
- **End**: Primary hamburger menu
- Always leave blank space for window dragging

### 3. AdwViewSwitcher + AdwViewSwitcherBar

**Purpose**: Flat tab navigation. `AdwViewSwitcher` goes in the header bar (wide layout). `AdwViewSwitcherBar` goes at the bottom (narrow layout, revealed via breakpoint).

**Rust API**:

```rust
use adw::{ViewStack, ViewSwitcher, ViewSwitcherBar};

let stack = ViewStack::new();
let view_switcher = ViewSwitcher::new();
view_switcher.set_stack(&stack);
view_switcher.set_policy(adw::ViewSwitcherPolicy::Wide);

let switcher_bar = ViewSwitcherBar::new();
switcher_bar.set_stack(&stack);

// Add pages
stack.add_titled_with_icon(&album_page, Some("albums"), "Albums", Some("folder-music-symbolic"));
stack.add_titled_with_icon(&artist_page, Some("artists"), "Artists", Some("avatar-default-symbolic"));
```

**Key Properties**:
- `AdwViewSwitcher.policy`: `Wide` (shows full text) or `Narrow` (icons only)
- `AdwViewSwitcherBar.reveal`: bool — animated show/hide, toggled by breakpoint

### 4. AdwBreakpoint

**Purpose**: Declaratively change widget properties when the window crosses a size threshold. Core of modern adaptive design.

**Breakpoint Conditions**:
- `max-width: WIDTH` — triggers when width ≤ WIDTH
- `min-width: WIDTH` — triggers when width ≥ WIDTH
- Units: `px` (pixels), `pt` (points), `sp` (scale-independent pixels — PREFERRED)

**Rust API** — building breakpoints programmatically:

```rust
use adw::{Breakpoint, BreakpointCondition};

// Parse a condition from string
let condition = BreakpointCondition::parse("max-width: 550sp")
    .expect("Invalid breakpoint condition");

let breakpoint = Breakpoint::new(condition);

// Add setters — change properties when breakpoint activates
breakpoint.add_setter(
    &switcher_bar,
    "reveal",
    &true_value,       // GValue bool = true
);
breakpoint.add_setter(
    &header_bar,
    "title-widget",
    &null_value,       // GValue null — removes view switcher from header
);

// Add breakpoint to the window
window.add_breakpoint(&breakpoint);

// Set minimum size manually (required when using breakpoints)
window.set_size_request(360, 200);
```

**Critical**: When using breakpoints, you **must** set minimum window size via `set_size_request()` — the window no longer auto-enforces a minimum.

### 5. AdwNavigationSplitView

**Purpose**: Two-pane layout (sidebar + content) that collapses to a stack on narrow screens. Replaces `AdwLeaflet` for sidebar patterns.

**Rust API**:

```rust
use adw::{NavigationSplitView, NavigationPage, ToolbarView, HeaderBar};

let split_view = NavigationSplitView::new();
split_view.set_min_sidebar_width(200);
split_view.set_max_sidebar_width(300);

// Sidebar
let sidebar_toolbar = ToolbarView::new();
sidebar_toolbar.add_top_bar(&HeaderBar::new());
sidebar_toolbar.set_content(Some(&sidebar_content));
let sidebar_page = NavigationPage::new(&sidebar_toolbar, "Library");
split_view.set_sidebar(Some(&sidebar_page));

// Content
let content_toolbar = ToolbarView::new();
content_toolbar.add_top_bar(&HeaderBar::new());
content_toolbar.set_content(Some(&main_content));
let content_page = NavigationPage::new(&content_toolbar, "Content");
split_view.set_content(Some(&content_page));
```

**Adaptive via Breakpoint**:

```xml
<AdwBreakpoint>
  <condition>max-width: 400sp</condition>
  <setter object="split_view" property="collapsed">True</setter>
</AdwBreakpoint>
```

When collapsed, the sidebar becomes a navigation page that can be pushed/popped.

### 6. AdwNavigationView

**Purpose**: Stack-based page navigation (push/pop). Replaces `AdwLeaflet` for page stacks.

**Rust API**:

```rust
use adw::{NavigationView, NavigationPage};

let nav_view = NavigationView::new();
nav_view.set_animate_transitions(true);
nav_view.set_pop_on_escape(true);

// Root page
let root_page = NavigationPage::new(&root_widget, "Home");
nav_view.push(&root_page);

// Push detail page
let detail_page = NavigationPage::new(&detail_widget, "Album Detail");
nav_view.push(&detail_page);

// Pop back
nav_view.pop();
```

### 7. AdwOverlaySplitView

**Purpose**: Overlay sidebar (slides over content). Replaces `AdwFlap`.

**Rust API**:

```rust
use adw::OverlaySplitView;

let split_view = OverlaySplitView::new();
split_view.set_sidebar(&sidebar_page);
split_view.set_content(&content_page);
split_view.set_show_sidebar(true); // toggle visibility
```

---

## Example: Full Adaptive Application Layout

```rust
use adw::prelude::*;
use adw::{self, Application, ApplicationWindow, Breakpoint, BreakpointCondition,
          HeaderBar, ToolbarView, ViewStack, ViewSwitcher, ViewSwitcherBar};
use gtk::{glib, glib::GValue, prelude::*};

fn build_ui(app: &Application) -> ApplicationWindow {
    let window = ApplicationWindow::builder()
        .application(app)
        .default_width(1200)
        .default_height(800)
        .build();

    // 1. ToolbarView — main layout
    let toolbar = ToolbarView::new();
    let header_bar = HeaderBar::new();

    // 2. ViewSwitcher in header
    let stack = ViewStack::new();
    let view_switcher = ViewSwitcher::new();
    view_switcher.set_stack(&stack);
    view_switcher.set_policy(adw::ViewSwitcherPolicy::Wide);
    header_bar.set_title_widget(Some(&view_switcher));

    // 3. Bottom switcher bar (narrow layout)
    let switcher_bar = ViewSwitcherBar::new();
    switcher_bar.set_stack(&stack);
    switcher_bar.set_reveal(false);

    toolbar.add_top_bar(&header_bar);
    toolbar.set_content(Some(&stack));
    toolbar.add_bottom_bar(&switcher_bar);

    // 4. Breakpoint: narrow layout
    let condition = BreakpointCondition::parse("max-width: 550sp").unwrap();
    let breakpoint = Breakpoint::new(&condition);

    let true_val = GValue::from(true);
    breakpoint.add_setter(&switcher_bar, "reveal", &true_val);
    // Remove view switcher from header (replace with null)
    let null_val = GValue::from(gtk::Widget::NONE);
    breakpoint.add_setter(&header_bar, "title-widget", &null_val);

    window.add_breakpoint(&breakpoint);
    window.set_size_request(360, 200);

    // 5. Content — NavigationSplitView for library browsing
    // ... (add pages to stack, split views inside each page)

    window.set_content(Some(&toolbar));
    window
}
```

---

## HIG Navigation Patterns

### Flat Tab Navigation (Albums / Artists)

```
Window (wide)
  ┌─────────────────────────────────────┐
  │ [≡]  [Albums | Artists]  [☰]        │ ← HeaderBar + ViewSwitcher
  ├─────────────────────────────────────┤
  │                                     │
  │  Grid/Column toggleable views       │ ← ViewStack content
  │                                     │
  └─────────────────────────────────────┘

Window (narrow)
  ┌─────────────────────┐
  │ [≡]           [☰]  │ ← HeaderBar (no switcher)
  ├─────────────────────┤
  │                     │
  │ Content             │
  │                     │
  ├─────────────────────┤
  │ [Albums] [Artists]  │ ← ViewSwitcherBar (revealed)
  └─────────────────────┘
```

### Detail Page Navigation

```
Library (wide)
  ┌──────────────────────────────────────────┐
  │ Library  │  Album Detail                 │
  │ ──────── │  ──────────────────────────── │
  │ Album 1  │  Artwork   Title              │
  │ Album 2  │  Track 1  ████████████░░░ 3:45│
  │ Album 3  │  Track 2  ██████████████░ 4:12│
  │ ──────── │  Track 3  ██████████░░░░ 3:01│
  │ Albums ▲ │                               │
  └──────────────────────────────────────────┘
                       ↓ narrow/collapsed
  ┌────────────────────────────┐
  │ [←] Album Detail    [☰]   │ ← NavigationView push
  ├────────────────────────────┤
  │ Artwork   Title            │
  │ Track 1  ████████████░░ 3:45│
  │ Track 2  ██████████████░ 4:12│
  │ Track 3  ██████████░░░░ 3:01│
  └────────────────────────────┘
```

In wide layout: `AdwNavigationSplitView` shows sidebar + content simultaneously.
In narrow layout: sidebar is collapsed; tapping an album pushes detail into `AdwNavigationView`.

---

## Widget Inventory for the Music Player

| UI Component | Widget | Adaptive Behavior |
|---|---|---|
| Main window | `AdwApplicationWindow` | — |
| Layout root | `AdwToolbarView` | Manages top/bottom bars |
| Title bar | `AdwHeaderBar` | Loses view switcher on narrow |
| Tab switcher (wide) | `AdwViewSwitcher` | In header bar's title-widget |
| Tab switcher (narrow) | `AdwViewSwitcherBar` | Revealed at bottom via breakpoint |
| Tab pages | `AdwViewStack` | Albums / Artists pages |
| Album grid | `GtkFlowBox` or custom `GridView` | Responsive column count |
| Artist grid | `GtkFlowBox` or custom `GridView` | Responsive column count |
| Detail page nav | `AdwNavigationView` | Push/pop detail pages |
| Player side panel | `GtkRevealer` or `AdwOverlaySplitView` | Slides in from left on playback |
| List view (column) | `GtkListView` / `GtkColumnView` | Standard list layout |
| Empty state | Custom widget (icon + label) | Centered in content area |
| Status bar | `GtkLabel` in `AdwToolbarView` bottom bar | Scanning indicator |
| Preferences | `AdwPreferencesDialog` | Built-in adaptive behavior |

---

## Key Rules from Research

1. **Do NOT use `AdwLeaflet`** — it is deprecated. Use `AdwNavigationSplitView` + breakpoints instead.
2. **Prefer `sp` units** for breakpoint conditions (scale-independent pixels respect user font size).
3. **Always set minimum window size** (`set_size_request()`) when using breakpoints.
4. **Use `AdwToolbarView`** as the root layout container — never manual `GtkBox` for navigation.
5. **200ms transition duration** for animations (HIG standard).
6. **6px spacing scale**: 6, 12, 18, 24, 30px (use `GtkBox::spacing`, margins).
7. **Never hardcode border radii** (use CSS classes or style context).
8. **Accessibility**: `set_accessible_label()`, `set_can_focus(true)`, `set_tooltip_text()` on all interactive widgets.
9. **Toast for feedback**: Use `AdwToastOverlay` + `AdwToast` for transient messages.
10. **Programmatic widgets only**: No `.ui`, `.blp`, or `.xml` files.
