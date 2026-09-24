# DESIGN_SYSTEM.md

## Visual Design System --- Multi-Tenant Workflow & Operations Platform

> This document defines the visual language and reusable UI rules of the
> product. Read together with `UX_UI_SPEC.md`. `UX_UI_SPEC.md` defines
> behavior; this document defines visual consistency.

# 1. Design Direction

The product should feel like a modern professional SaaS application, not
a traditional dense ERP.

Core qualities:

``` text
Clean
Calm
Fast
Professional
Modern
Trustworthy
Operational
Information-dense when needed
Simple by default
```

Avoid: - excessive gradients, - glassmorphism everywhere, - decorative
3D effects, - oversized marketing-style typography inside the
application, - unnecessary animation, - excessive borders, - excessive
shadows, - dashboard clutter, - giant empty spaces that reduce
operational efficiency, - every card having a different visual style, -
"AI-looking" random purple/blue decoration.

The application is a work tool. Visual design must help users understand
state and act quickly.

# 2. Design Foundation

Frontend foundation:

``` text
SvelteKit
Tailwind CSS
shadcn-svelte / Bits UI
Lucide icons
```

Prefer accessible headless primitives and shared design-system
components over page-specific UI implementations.

# 3. Theme Strategy

Support:

``` text
Light
Dark
System
```

Initial default:

``` text
Light
```

User preference should be remembered.

Theme selection is independent from tenant branding.

Dark mode must be designed, not produced by naïvely inverting colors.

# 4. Color Architecture

Use semantic design tokens rather than hard-coded colors.

Required semantic groups:

``` text
background
foreground

surface
surface-muted
surface-elevated

border
border-strong

primary
primary-hover
primary-foreground

secondary
secondary-hover

muted
muted-foreground

success
warning
danger
info

focus-ring
```

Status colors must be separate semantic tokens:

``` text
status-not-started
status-ready
status-active
status-paused
status-blocked
status-awaiting-approval
status-completed
status-cancelled
```

Do not scatter Tailwind color literals throughout feature components.

# 5. Brand Color

The initial product should use a restrained professional primary accent.

Do not permanently couple product semantics to a specific hue.

The primary color is used for: - primary CTA, - active navigation, -
focus/selection emphasis, - selected controls, - important interactive
highlights.

Do not paint entire dashboards in the primary color.

Tenant branding may later override limited brand tokens such as logo and
accent, but must preserve accessibility.

# 6. Neutral Surfaces

Operational UI relies heavily on neutral surfaces.

Light theme hierarchy:

``` text
App background
→ Navigation/sidebar surface
→ Main content surface
→ Cards/panels
→ Elevated overlays
```

Dark theme uses equivalent semantic hierarchy.

Prefer subtle surface difference over heavy shadows.

# 7. Typography

Preferred UI font strategy:

``` text
system UI font stack
```

A custom web font may be adopted only if: - performance is acceptable, -
Turkish glyph support is excellent, - readability is strong, -
licensing/deployment is appropriate.

Typography hierarchy:

``` text
Page title
Section title
Card title
Body
Secondary text
Label
Caption
Metric
```

Suggested scale:

``` text
12px caption
14px secondary/body-small
16px body
18px card/section emphasis
20–24px section/page heading
28–32px major dashboard title when needed
```

Avoid giant 48--72px application headings.

Use font weight for hierarchy before adding more colors.

# 8. Numeric Typography

Timers and important operational numbers should use tabular numerals
where possible.

Example:

``` text
01:42:18
```

Timer digits must not visually jump as values change.

Use consistent number alignment in tables and metrics.

# 9. Spacing

Use a consistent spacing scale based on 4px increments.

Conceptual scale:

``` text
4
8
12
16
20
24
32
40
48
64
```

Typical component spacing:

``` text
button horizontal padding: 12–16
card padding: 16–24
form vertical gap: 16
page section gap: 24–32
```

Do not use arbitrary spacing values unless there is a documented reason.

# 10. Border Radius

Use restrained modern rounding.

Recommended hierarchy:

``` text
small controls: 6–8px
buttons/inputs: 8px
cards: 10–12px
dialogs/drawers: 12–16px
pills/badges: full radius
```

Avoid extremely rounded "bubble UI" for the entire product.

# 11. Shadows

Use shadows sparingly.

Appropriate: - dropdown, - popover, - modal, - floating command
palette, - temporary elevated layer.

Normal cards should primarily use border/surface hierarchy rather than
strong shadows.

# 12. Borders

Default borders are subtle.

Use stronger borders for: - selected items, - drag target, - focus
state, - validation emphasis, - active operational item where necessary.

Do not place visible borders around every nested element.

# 13. Application Shell

Desktop:

``` text
┌──────────────┬─────────────────────────────────────────┐
│ Sidebar      │ Topbar                                  │
│              ├─────────────────────────────────────────┤
│              │                                         │
│ Navigation   │ Main Content                            │
│              │                                         │
│              │                                         │
└──────────────┴─────────────────────────────────────────┘
```

Sidebar: - compact but readable, - collapsible, - icon + label, - clear
active state, - organization/workspace context near top, -
account/settings near bottom where appropriate.

Suggested expanded width:

``` text
240–272px
```

Collapsed:

``` text
64–72px
```

Do not hard-code exact values until implementation validates layout.

# 14. Topbar

Topbar may contain:

``` text
Breadcrumb/context
Global search
Create action when contextually appropriate
Notifications
Help
User menu
```

Do not fill topbar with rarely used actions.

# 15. Page Width

Operational pages may use wide layouts.

Do not force every page into a narrow marketing-site max-width.

Use: - constrained width for forms/settings, - wider width for
project/admin views, - full width for tables, active operations and TV.

# 16. Page Header Pattern

Standard:

``` text
Title
Short optional description

[Secondary actions] [Primary action]
```

Example:

``` text
Projects
Manage ongoing and planned work.

[Import] [Create Project]
```

On mobile, actions may move below the title or into overflow.

# 17. Navigation

Primary navigation should use icon + text.

Recommended icon family:

``` text
Lucide
```

Do not mix multiple icon libraries without necessity.

Icons support labels; they do not replace unclear labels.

# 18. Buttons

Variants:

``` text
Primary
Secondary
Outline
Ghost
Danger
Link
Icon
```

Rules: - one dominant primary action per local context where possible, -
destructive action never visually resembles primary success action, -
loading preserves button width, - icon-only buttons require
tooltip/accessibility label, - disabled buttons must remain readable.

Suggested heights:

``` text
sm: 32px
default: 40px
lg/touch: 44–48px
```

Employee mobile primary actions should favor larger touch targets.

# 19. Inputs

All inputs need: - visible label, - optional description, - error
state, - disabled state, - focus state.

Do not rely only on placeholder as label.

Input height generally aligns with default button height.

# 20. Forms

Group related fields.

Avoid one enormous form.

Use sections:

``` text
General
Assignment
Schedule
Workflow
Completion Rules
Advanced
```

Advanced sections may collapse.

Sticky action footer may be used for long explicit-save forms.

# 21. Autosave

Small inline edits may autosave.

States:

``` text
Saving…
Saved
Could not save
```

Do not permanently show noisy "Saved" indicators everywhere; use subtle
transient feedback.

# 22. Cards

Cards are information containers, not decoration.

Standard card:

``` text
Header
Content
Optional footer/actions
```

A card should not contain another visually heavy card unless hierarchy
requires it.

Nested sections should rely on spacing/surface differences to avoid
"card inside card inside card" visual noise.

# 23. Project Cards

Suggested:

``` text
Project Name                       Active
Short metadata

Progress ███████░░ 72%

Due 28 Sep
12 active • 3 blocked
```

Clicking the main card opens the project.

Secondary actions live in overflow menu.

# 24. Progress Rings

Use for compact section/project progress.

Rules: - percentage text remains visible, - ring is not the only status
signal, - avoid tiny unreadable rings, - use consistent size variants.

Example:

``` text
 72%
```

# 25. Tables

Tables are important for admin power users.

Requirements: - sticky header for long lists, - sortable columns, -
optional resizable columns later, - row selection, - bulk action bar, -
clear hover, - keyboard accessibility, - density appropriate for
operations.

Do not center-align text columns by default.

Numbers/durations may right-align.

# 26. Table Density

Support a balanced default.

Potential future preference:

``` text
Comfortable
Compact
```

Employee screens should not resemble admin data tables.

# 27. Status Badges

Status badge includes text.

Examples:

``` text
Ready
Active
Paused
Blocked
Completed
```

Use: - semantic background, - semantic foreground, - optional icon.

Never communicate status by a colored dot alone.

# 28. Priority

Canonical priority:

``` text
Low
Normal
High
Urgent
```

Use restrained emphasis.

"Urgent" should stand out, but the whole application should not become
red.

# 29. Live Task Card

Active work deserves stronger visual hierarchy.

Example:

``` text
┌─────────────────────────────────────┐
│ ACTIVE                              │
│ Quality Control                     │
│ Project A / Card 1842               │
│                                     │
│              01:42:18               │
│                                     │
│ [Pause]                  [Complete] │
└─────────────────────────────────────┘
```

Timer is the primary visual element, not decorative animation.

# 30. Employee Mobile Bottom Area

For task execution, critical actions may remain reachable near the
bottom.

Consider sticky action area:

``` text
[ PAUSE ] [ COMPLETE ]
```

Respect safe-area insets on mobile/PWA.

Do not cover content.

# 31. Dialogs

Use dialogs for: - focused decisions, - high-impact confirmation, -
short forms.

Do not use dialogs for entire complex admin pages.

Dialog must: - trap focus, - support Escape when safe, - clearly label
primary/secondary action, - explain destructive consequences.

# 32. Drawers / Sheets

Useful for: - mobile detail, - quick edit, - filters, - property
configuration, - assignment selection.

On desktop, side drawer may preserve list context.

# 33. Dropdown Menus

Use overflow menu for secondary actions:

``` text
Duplicate
Move
Archive
Delete
```

Primary actions should not be hidden in overflow.

# 34. Toasts

Use for transient feedback:

``` text
Saved
Task started
Card deleted — Undo
Connection restored
```

Do not use toast as the only place to show critical persistent failure.

# 35. Empty States

Use restrained illustration/icon only when useful.

Structure:

``` text
Icon
Title
One-sentence explanation
Primary action
Optional secondary action
```

Operational screens should favor clarity over decorative artwork.

# 36. Skeletons

Skeletons should approximate final layout.

Avoid spinner-only full-page loading where structure is known.

Do not show skeleton for actions that complete nearly instantly.

# 37. Error Design

Inline errors near the affected area.

Page error:

``` text
Couldn’t load projects.
Your connection may have been interrupted.

[Try Again]
```

Preserve user input after save failure.

# 38. Realtime Indicator

Normal connected state should be visually quiet.

Only emphasize:

``` text
Reconnecting…
Offline
Sync issue
```

TV screen should show a subtle connection state so stale data is not
mistaken for live data.

# 39. Breadcrumbs

Use for deep hierarchy:

``` text
Project / Floor 1 / Apartment 3 / Card
```

On mobile: - collapse older levels, - preserve current parent context.

# 40. Tree View

Tree indentation must remain readable.

Each node may contain: - expand control, - icon, - title, -
progress/status, - contextual actions.

Do not overload every row with many permanent buttons; reveal secondary
actions on hover/focus/menu.

# 41. Nested Card View

Nested card layout must communicate hierarchy using: - breadcrumb, -
heading, - spacing, - progress, - parent context.

Avoid rendering ten levels simultaneously as nested bordered boxes.

Enter a section to inspect deeper levels.

Section hierarchy uses this drill-down model: recursive hierarchy is
represented through URL-based drill-down navigation. A page renders one
hierarchy level and its direct children; the UI does not recursively expand
the entire descendant tree (§40 tree view is reserved for possible future
tools, not for project sections).

# 42. Process Step Visuals

Suggested states:

``` text
✓ Completed
● Active
Ⅱ Paused
○ Ready
🔒 Blocked
◇ Awaiting approval
```

Use actual icon components, not emoji in production.

Connectors/sequence indicators may show order, but dependency logic must
not be visually implied incorrectly.

# 43. Dependency Display

Blocked task:

``` text
Blocked

Waiting for
○ Material Approval
○ Measurement
```

When possible show the specific unmet prerequisite.

# 44. Assignee Display

Use:

``` text
Avatar + Name
```

Teams:

``` text
Team icon/avatar + Team Name
```

For multiple assignees: - show first few, - `+3`, - tooltip/popover for
full list.

# 45. Avatar

Fallback: - initials, - deterministic neutral/accent background.

Do not require profile photos.

# 46. Notifications

Unread: - subtle stronger surface/text, - unread marker, - not an
aggressive colored block.

Notification groups: - Today, - Yesterday, - Earlier.

# 47. Command Palette

Shortcut:

``` text
Ctrl/Cmd + K
```

Design: - centered/floating dialog, - immediate search focus, - grouped
results, - keyboard navigation, - recent items/actions.

Potential actions:

``` text
Go to Project
Open Card
Create Project
Create Card
Search User
Switch Workspace
```

Only expose actions user has permission to execute.

# 48. Filters

Simple filters appear as chips/selects.

Advanced filters may use side panel.

Active filters should be visible and removable individually.

Provide:

``` text
Clear all
```

when useful.

# 49. Bulk Selection

When rows/cards selected, replace or augment normal toolbar with a bulk
action bar.

Example:

``` text
143 selected
[Assign] [Move] [Due Date] [Archive] [More]
```

Do not hide selection count.

# 50. Date & Time

Render according to locale.

For operational recency, combinations may be useful:

``` text
Today, 14:42
21 Sep 2026
3 min ago
```

Relative time should have exact time accessible via tooltip/detail where
useful.

# 51. Localization Layout

Turkish is the default UI.

English is supported from initial architecture.

Design for text expansion.

Do not fix button widths to Turkish labels.

Avoid layouts that break when:

``` text
Tamamla
Complete
Awaiting approval
Onay Bekliyor
```

have different lengths.

# 52. Terminology Overrides

Tenant terminology is resolved at presentation level.

Example:

``` text
Card
→ Kart
→ İş Emri
→ Sipariş
```

Components should receive resolved labels from a terminology
service/configuration layer rather than hard-coded domain display text.

# 53. RTL Readiness

Full RTL is not required for V1.

However: - avoid assumptions that permanently block RTL, - prefer
logical CSS properties where practical, - keep icon/text composition
adaptable.

# 54. Accessibility

Target WCAG-aware implementation.

Mandatory: - sufficient contrast, - keyboard access, - visible focus, -
semantic structure, - form labels, - accessible names, - reduced
motion, - no color-only meaning, - no timer announcements every second
to screen readers.

# 55. Motion

Motion should explain state change, not entertain.

Recommended duration:

``` text
fast: 100–150ms
normal: 150–250ms
```

Use for: - drawer, - dropdown, - reorder, - completion feedback, -
expanding sections.

Avoid: - bouncing buttons, - looping decorative animations, - large
parallax, - excessive page transitions.

Respect `prefers-reduced-motion`.

# 56. Drag and Drop

Use for: - section reorder, - step reorder, - optional card movement.

Must have a non-drag alternative.

During drag: - clear source, - clear drop target, - invalid target
indication, - auto-scroll when appropriate.

# 57. TV Visual System

TV is a dedicated visual mode.

Requirements: - full-screen, - no normal sidebar, - large timer, - high
contrast, - readable from several meters, - adaptive grid, - minimal
metadata, - no tiny controls.

Grid concept:

``` text
1 active → 1
2 active → 2 columns
3–4 active → 2×2
5–6 active → 3×2
7+ → adaptive
```

Do not shrink text indefinitely. Consider pagination/rotation if active
count becomes too high.

# 58. TV Card Priority

Visual order:

``` text
Employee/team
Task/process step
Project/card context
Live timer
Status
Selected custom properties
```

Timer should remain instantly recognizable.

# 59. Dashboard Metrics

Metric card:

``` text
Active Work
12
+2 since morning
```

Do not use meaningless trend percentages unless the comparison is
useful.

Avoid dashboards composed only of KPI cards.

# 60. Charts

Charts must answer a specific operational question.

Use sparingly.

Examples: - active vs waiting time, - cycle time trend, - work completed
by date, - bottleneck duration.

Always provide labels/tooltips and accessible data alternative where
practical.

Do not use 3D charts.

# 61. Reports

Reports prioritize: - filters, - clear metrics, - table/detail, -
export.

Do not turn reports into visually impressive but operationally useless
dashboards.

# 62. Settings

Settings navigation:

``` text
Organization
Workspace
Members
Teams
Roles & Permissions
Terminology
Language & Locale
Notifications
Billing
Integrations
API & Webhooks
```

Only show authorized sections.

# 63. Billing Visuals

Billing is calm and factual.

Highlight: - current plan, - renewal, - usage, - limits, - invoices.

Avoid aggressive consumer-style upsell banners throughout the
application.

# 64. Platform Admin

Platform admin must be visually distinguishable from tenant admin.

Use a clear context indicator:

``` text
PLATFORM ADMIN
```

Do not rely only on color.

This reduces risk of performing platform actions while believing the
user is inside a tenant.

# 65. Destructive Color Usage

Danger color is reserved for: - delete, - destructive confirmation, -
serious failure, - critical blocking issue.

Do not use danger red merely because something is overdue if a less
alarming semantic treatment works.

# 66. Success Color Usage

Success indicates: - completed, - saved success when persistent
indication is needed, - healthy state.

Do not make all positive metrics green without meaning.

# 67. Information Density

Admin:

``` text
medium/high density
```

Employee:

``` text
low/medium density
```

TV:

``` text
low density / high legibility
```

The same component does not need identical density across contexts.

# 68. Responsive Admin Navigation

Desktop: - persistent sidebar.

Tablet: - collapsible sidebar.

Mobile: - drawer navigation or carefully selected bottom navigation.

Do not put every admin module in a mobile bottom bar.

# 69. Employee Mobile Navigation

Potential bottom navigation:

``` text
Görevlerim
Aktif
Bildirimler
Profil
```

Only use if user testing/implementation confirms it improves navigation.

The current task must remain easy to reach.

# 70. Touch Targets

Interactive touch targets should generally be at least around:

``` text
44 × 44px
```

especially for employee mobile execution controls.

# 71. Focus Mode

When an employee starts a task, UI may emphasize the active task and
reduce surrounding noise.

Do not trap the user; they can still inspect necessary context.

# 72. Permission-Aware UI

Unauthorized actions: - usually hidden, - disabled with explanation only
when seeing the capability is useful.

Backend remains authoritative.

Do not flash unauthorized controls before permissions load.

# 73. Feature Entitlement UI

If a plan does not include a feature: - explain calmly, - show plan
requirement to authorized admin, - do not show upgrade prompts to normal
workers unless product strategy explicitly requires it.

# 74. Design Tokens

Implement tokens centrally.

Conceptual example:

``` text
--background
--foreground
--surface
--surface-muted
--border
--primary
--primary-foreground
--success
--warning
--danger
--info
--focus-ring

--radius-sm
--radius-md
--radius-lg

--space-1
--space-2
...
```

Tailwind/theme configuration should map to these semantic concepts.

# 75. Component Ownership

Reusable visual primitives belong in:

``` text
packages/ui
```

Feature-specific composed components may live near the feature.

Examples:

``` text
packages/ui/Button
packages/ui/Dialog
packages/ui/Badge
packages/ui/LiveTimer

apps/web/features/tasks/ActiveTaskCard
apps/web/features/projects/ProjectProgressCard
```

Do not move every feature component into the global UI package.

# 76. Component States

Every reusable interactive component must consider:

``` text
default
hover
focus
active
disabled
loading
error
selected
```

where applicable.

# 77. Story / Preview Strategy

If a component preview/story tool is later introduced, prioritize: -
design-system primitives, - operational status components, -
LiveTimer, - task cards, - progress components, - empty/error states.

Do not introduce heavy tooling solely because this document mentions
previews.

# 78. Visual Regression

When practical, add visual regression coverage for critical stable
screens:

``` text
Employee current task
Admin project page
Active work
TV display
```

This is secondary to functional correctness but valuable as the UI
stabilizes.

# 79. UI Performance

Visual polish must not cause sluggishness.

Avoid: - rendering hundreds of live timers with expensive global
rerenders, - unnecessary blur/backdrop effects, - huge icon bundles, -
unvirtualized massive tables/trees, - layout shifts during realtime
updates.

# 80. Loading and Layout Stability

Reserve predictable space for: - avatars, - badges, - progress, -
timer, - async content.

Realtime changes should not cause major unexpected layout jumps.

# 81. Design Review Checklist

Before a screen is complete:

``` text
[ ] Clear primary purpose
[ ] Clear primary action
[ ] Correct information hierarchy
[ ] Uses design tokens
[ ] Uses shared components
[ ] Loading state
[ ] Empty state
[ ] Error state
[ ] Disabled state
[ ] Permission state
[ ] Responsive
[ ] Keyboard usable
[ ] Focus visible
[ ] Turkish works
[ ] English text expansion works
[ ] Dark mode works
[ ] Realtime updates do not break layout
[ ] No unnecessary decoration
```

# 82. Initial Screen Priority

Build visual consistency in this order:

``` text
1. App shell
2. Authentication
3. Organization/workspace switcher
4. Projects list
5. Project detail
6. Section tree/card views
7. Card detail
8. Process/step UI
9. Employee My Tasks
10. Active task
11. Active Work
12. TV
13. Templates
14. Notifications
15. Settings
16. Reports
17. Billing
```

Do not design every future screen before validating core patterns.

# 83. Visual Acceptance Principle

A user should be able to identify within a few seconds:

``` text
Where am I?
What is happening?
What needs attention?
What can I do next?
```

If the visual design makes these answers harder, simplify it.

------------------------------------------------------------------------

# 84. Implemented Product Page Contract (STEP 19.5/19.5C)

- AppShell owns global context/account/navigation; pages own their breadcrumb
  and content. Render exactly one main landmark and skip link.
- Desktop uses a persistent sidebar (brand, org/workspace context selectors,
  primary navigation, account block + logout); below the mobile breakpoint the
  same sidebar becomes an off-canvas drawer with backdrop, Escape, scroll lock
  and focus return to the menu trigger. Topbar carries the menu trigger,
  navigation status, locale and theme selectors.
- PageHeader orders context, title/description, status metadata and actions.
  Use shared PageHeader, StatusBadge, EmptyState, ConfirmDialog, FormDrawer and
  ActionMenu primitives.
- Shared form/card/action styles live in app.css; feature components compose
  these rather than copying separate visual systems.
- Use `--page-gutter`, `--content-width`, `--form-width`, `--control-height`,
  `--sidebar-width`, `--topbar-height`, `--selection-surface` and
  `--danger-surface` for their semantic purposes. Interactive
  form/buttons/summary controls have a 44px minimum height.
- Creation surfaces are transient FormDrawer sheets (right-side dialog on
  desktop, near-full-width on mobile) opened by an explicit primary action.
  No permanent create form lives inside a list. Failed submission keeps the
  drawer open with the draft; success closes it and refreshes/navigates.
- Secondary entity actions use the shared ActionMenu overflow (menu role,
  arrow/Home/End/Escape keys, outside click, focus return, danger styling only
  on destructive items). Archive stays a deliberate ConfirmDialog flow.
- Primary actions create/save; secondary actions edit/cancel; archive uses
  danger styling and deliberate confirmation. Native dialogs focus cancel
  first, support Escape and return focus. Failed requests keep the draft.
- Status always includes localized text and a non-color cue. Manual completed
  status does not imply process completion or a calculated percentage.
- Recursive hierarchy is drill-down only: the project page lists root section
  cards, a section page lists direct-child section cards and that section's
  work items. Depth is expressed by URL and breadcrumbs, never by indentation
  or disclosure trees. Section cards are navigation surfaces (stretched link +
  chevron); management actions live on the section's own page.
- WorkItemCard may receive a future child summary snippet only when backed by
  real authorized data. Do not render speculative process/timer placeholders.
- Validate long names, TR/EN, light/dark, 360/375/768px and desktop layouts.

Architecture and implemented boundaries are recorded in
[ADR 0014](decisions/0014-frontend-product-experience.md).

# Final Design Rule

> Build a serious operational SaaS product with the clarity of a modern
> consumer application.

> The interface should feel consistent enough that a user learns the
> design language once and can understand every new module without
> retraining.
