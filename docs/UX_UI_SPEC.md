# UX_UI_SPEC.md

## Multi-Tenant Workflow & Operations Platform

> This document is the product UX/UI contract for the coding agent. Read
> together with `AI_CODING_AGENT_MASTER_PLAN.md`, `DATABASE_SCHEMA.md`,
> and `API_CONTRACT.md`.

# 1. Product UX Principle

> **Simple when you start; powerful when you need it.**

The backend may be complex. The UI must reveal only the complexity
required for the current user, role, screen, and task.

Primary experiences:

``` text
EMPLOYEE
My Tasks → Start → Pause/Resume → Complete

ADMIN / MANAGER
Create → Organize → Assign → Monitor → Correct → Report

TV
Who is working on what right now, and for how long?
```

# 2. Global UX Rules

1.  Common employee actions should normally require no more than 3
    taps/clicks.
2.  Never require users to understand internal concepts such as tenant
    IDs, revisions, outbox events, dependency graphs, or database
    relationships.
3.  Use progressive disclosure for advanced features.
4.  Prefer autosave for small safe edits.
5.  Prefer action + Undo over confirmation for reversible actions.
6.  Use confirmation for destructive/high-impact actions.
7.  Every blocked action must explain why it is blocked.
8.  Every async action must clearly show pending/success/failure state.
9.  Realtime updates must not require refresh.
10. Mobile employee UX is first-class.
11. Desktop admin UX is information-rich but uncluttered.
12. TV UX is readable from distance.
13. Keyboard navigation is first-class for admin.
14. Accessibility is mandatory.
15. Never use color alone to communicate state.
16. Never expose industry-specific terminology in the core product.
17. Tenant terminology aliases may rename generic UI concepts without
    changing internal domain names.
18. Preserve user context after realtime updates; do not unexpectedly
    close panels or reset scroll.
19. Do not show raw technical errors.
20. Empty states should tell the user what to do next.

# 3. Information Architecture

Primary admin navigation:

``` text
Home
Projects
Active Work
My Tasks
People
Templates
Notifications
Reports
Search
Settings
```

Advanced items may appear based on permissions:

``` text
Automations
API & Webhooks
Billing
Audit
Platform Admin
```

Do not show inaccessible navigation items unless there is a deliberate
upsell/education reason.

# 4. Organization and Workspace Switcher

Top-left application context:

``` text
[Organization ▼]
[Workspace ▼]
```

Requirements: - Fast switching. - Remember last used
organization/workspace. - Role/permission context updates immediately. -
Never leak previous tenant data while switching. - Show loading skeleton
while context changes. - If user has one organization/workspace, keep
switcher visually quiet. - Support many organizations with search.

# 5. Admin Home

Default manager dashboard should answer:

``` text
What is active?
What is late?
What is blocked?
What needs approval?
What changed recently?
```

Suggested layout:

``` text
┌─────────────────────────────────────────────┐
│ Today                                       │
│ Active 12 | Paused 3 | Blocked 5 | Late 8 │
└─────────────────────────────────────────────┘

┌──────────────────────┐ ┌────────────────────┐
│ Active Work          │ │ Needs Attention    │
│ live rows/timers     │ │ blocked/late/etc.  │
└──────────────────────┘ └────────────────────┘

┌─────────────────────────────────────────────┐
│ Project Progress                            │
└─────────────────────────────────────────────┘
```

Do not overload V1 with configurable widgets.

# 6. Project List

Provide: - search, - status filter, - owner/manager filter if
relevant, - due-date filter, - card/list view, - sort, - clear create
button.

Project card may show:

``` text
Project Name
Status
Progress %
Due date
Active work count
Blocked count
```

Avoid showing every available property.

# 7. Project Detail

Header:

``` text
Project Name
Status
Progress
Due Date
Actions
```

Main views:

``` text
Sections
Cards
Activity
Files
Settings
```

Primary section experience supports both:

``` text
Tree View
Card View
```

Remember the user's last selected view.

# 8. Recursive Section UX

Tree example:

``` text
▾ Floor 1
   ▾ Area A
      Room 1
      Room 2
   Area B
▸ Floor 2
```

Actions: - add child, - rename, - move, - duplicate, - bulk-create
siblings, - archive/delete.

Drag-and-drop is optional convenience, not the only way to move items.

For large trees: - virtualize if necessary, - expand/collapse all only
with safeguards, - search/filter, - preserve expansion state.

# 9. Section Card View

Each section card:

``` text
Section Name
Progress ring
Active / Total
Blocked count
Due status
```

Nested navigation should feel like entering folders.

Breadcrumb always visible:

``` text
Project / Section / Subsection
```

# 10. Card Detail

Recommended desktop layout:

``` text
┌──────────────────────────────────────────────────────┐
│ CARD-01842  Card title                 Status        │
│ Project / Section / Subsection                       │
├──────────────────────────────┬───────────────────────┤
│ Main Content                 │ Properties            │
│                              │ Assignees             │
│ Processes                    │ Priority              │
│ Activity                     │ Due date              │
│ Comments                     │ Related cards         │
│ Files                        │                       │
└──────────────────────────────┴───────────────────────┘
```

Mobile becomes a single-column flow.

# 11. Dynamic Property UX

Admin can add property blocks:

``` text
Text
Number
Money
Date
Date & Time
Duration
Yes/No
Select
Multi-select
Person
Team
Relation
File
Image
URL
Phone
Email
```

Requirements: - inline editing, - autosave, - clear saved state, -
drag/reorder in edit mode, - required indicator, - visibility
settings, - sensible empty display.

Do not make every card look like a database configuration screen.

# 12. Process UI

Card process section:

``` text
Process A                           60%
──────────────────────────────────────
✓ Step 1
● Step 2       Active      00:42:17
○ Step 3       Locked
○ Step 4
```

Clicking a locked step shows:

``` text
Cannot start yet.

Waiting for:
✓ Step A
○ Step B
```

Never use only a disabled button with no explanation.

# 13. Process Designer

Admin-only advanced screen.

Suggested canvas/list hybrid:

``` text
Step 1
  ↓
Step 2 ─────┐
  ↓         │
Step 3      Step 4
  └────┬────┘
       ↓
     Step 5
```

V1 may use an ordered list + dependency selector rather than a complex
visual graph.

Each step editor: - name, - description, - default assignee, - estimated
duration, - prerequisites, - completion requirements, - checklist, -
form, - approval requirement.

Provide `Test Workflow` / simulation later.

# 14. Employee Home --- Highest UX Priority

Mobile-first.

``` text
Hello, [Name]

CURRENT TASK
┌──────────────────────────────┐
│ Card / Context               │
│ Process Step                 │
│                              │
│       01:42:18               │
│                              │
│ [ PAUSE ]       [ COMPLETE ] │
└──────────────────────────────┘

NEXT
┌──────────────────────────────┐
│ Task                         │
│ Due today • High priority    │
│ [ START ]                    │
└──────────────────────────────┘

MY TASKS
...
```

When there is no active task, emphasize next ready task.

# 15. Employee Task Detail

Show only what helps execution:

``` text
Task name
Card/context
Instructions
Required properties
Checklist
Files/photos
Comments
Start/Pause/Resume/Complete
```

Hide admin configuration.

# 16. Start Flow

If no active task:

``` text
[START]
→ immediate start
→ button becomes active timer
→ realtime event
```

Avoid unnecessary confirmation.

If another task is active:

``` text
You are currently working on:
Task A — 00:31:12

Pause it and start Task B?

[Cancel] [Pause & Start]
```

# 17. Pause Flow

If pause reason is not required:

``` text
[PAUSE]
→ immediate pause
→ Undo available
```

If required:

``` text
Why are you pausing?

○ Break
○ Switching task
○ Waiting for material
○ Machine issue
○ Waiting for approval
○ Other

[Pause]
```

Keep it fast.

# 18. Complete Flow

If no completion requirements:

``` text
[COMPLETE]
→ complete
→ success feedback
→ Undo/Reopen according to permission
```

If requirements exist, show only missing requirements:

``` text
Before completing:

☐ Complete checklist
☐ Upload required photo

[Complete]
```

# 19. Live Timer UX

Active timer format:

``` text
01:42:18
```

Rules: - update locally every second, - server is authoritative, - do
not animate excessively, - paused timer is visually distinct, - after
reconnect resync with server, - do not reset timer on page navigation.

# 20. Active Work Admin Page

Show all active work across projects.

Desktop table/card hybrid:

``` text
Employee | Project | Card | Step | Started | Timer | Status
```

Filters: - project, - team, - employee, - status, - workspace.

Timer updates without full-table rerender/jank.

# 21. TV Display

No sidebar. No normal application chrome.

Example:

``` text
┌─────────────────────┐ ┌─────────────────────┐
│ Mehmet              │ │ Ayşe                │
│ Project A           │ │ Project B           │
│ Card 1842           │ │ Card 1991           │
│ Cutting             │ │ Quality Control     │
│                     │ │                     │
│      01:24:18       │ │      00:18:42       │
└─────────────────────┘ └─────────────────────┘
```

Requirements: - large typography, - high contrast, - responsive grid, -
selected properties only, - no tiny controls, - realtime, - full-screen
friendly, - optional paused area, - brief completed animation/state, -
connection indicator if realtime is lost.

# 22. Notifications UX

Notification center:

``` text
Today
• New task assigned
• Task ready
• Approval requested

Yesterday
...
```

Each notification: - understandable sentence, - time, - read/unread, -
deep link.

Do not expose raw event names.

Allow: - mark read, - mark all read, - preferences.

# 23. Undo UX

Snackbar/toast:

``` text
Card deleted.                    [UNDO]
```

``` text
Task completed.                  [UNDO]
```

Use a short immediate undo window when safe. Long-term restoration lives
in Trash/history.

# 24. Trash

Admin view:

``` text
Deleted Item | Type | Deleted By | Deleted At | Restore
```

Filters and retention information.

Permanent deletion, if available, is visually separated and
high-friction.

# 25. Conflict UX

If revision conflict:

``` text
This item changed while you were editing it.

Changed by: [User]
Changed at: 16:42

[Review Changes] [Reload]
```

Do not silently discard the user's input. Preserve draft locally when
possible.

# 26. Offline / Connection UX

Connection states:

``` text
Online
Reconnecting…
Offline
Syncing…
```

Do not show persistent banners during normal healthy operation.

For time commands offline, if supported: - clearly show pending sync, -
never pretend server accepted it, - surface reconciliation conflict.

# 27. Search UX

Global shortcut:

``` text
Ctrl/Cmd + K
```

Search: - projects, - sections, - cards, - IDs, - users, - selected
property values.

Results grouped by type and keyboard navigable.

# 28. Saved Views

User can save filters/sorting/view type.

Example:

``` text
My Views
• Late Work
• Active Installation Team
• Waiting Approval
```

Keep advanced filter builder outside the normal employee UI.

# 29. Bulk Action UX

When multiple items selected:

``` text
143 selected

Assign | Move | Change Due Date | Add Process | Archive | More
```

For consequential operations: 1. preview, 2. show affected/blocked
counts, 3. confirm, 4. execute, 5. show result summary, 6. allow safe
undo when possible.

# 30. Templates UX

Templates page:

``` text
Card Templates
Process Templates
```

Template cards show: - name, - version, - status, - last updated.

Published versions are visually immutable.

Editing published template:

``` text
Create new version?
v3 → Draft v4
```

# 31. Roles & Permissions UX

Simple mode first:

``` text
Role: Team Lead

Projects
✓ View
✓ Edit

Tasks
✓ Start
✓ Pause
✓ Complete
✓ Reassign

Users
✓ View
✗ Invite

Billing
✗ Manage
```

Advanced scope configuration can be secondary.

# 32. Billing UX

Organization owner:

``` text
Current Plan
Professional

Users       18 / 25
Storage     31 / 50 GB
Automation  2,841 / 5,000

Renewal
21 Oct 2026

[Manage Plan]
```

Never interrupt employee task completion with billing upgrade dialogs.

Billing warnings go primarily to billing/admin roles.

# 33. Onboarding

First-run experience:

``` text
1 Create Organization
2 Create Workspace
3 Invite Team
4 Create First Project
5 Create First Card
6 Create/Apply Process
7 Assign Task
8 Employee Starts Task
9 Watch Live
```

Allow skipping nonessential steps.

Use sample/demo data only if clearly marked and easy to remove.

# 34. Simple Mode vs Advanced Mode

### Simple Mode

Expose:

``` text
Project
Card
Task
Assignee
Start
Complete
```

### Advanced Mode

Expose when enabled/needed:

``` text
Nested sections
Processes
Process steps
Dependencies
Forms
Approvals
Automations
SLA
API
Webhooks
```

This is progressive disclosure, not two separate products.

# 35. Responsive Breakpoints

Do not design desktop then shrink blindly.

Employee: - optimize 360px+ widths.

Admin: - mobile functional, - tablet good, - desktop optimal.

TV: - support common 1080p/4K landscape screens.

# 36. Accessibility

Minimum: - semantic HTML, - keyboard navigation, - visible focus, -
accessible dialogs, - ARIA only where necessary, - form labels, -
sufficient contrast, - status text in addition to color, -
reduced-motion support, - touch targets appropriate for mobile, -
screen-reader friendly timers/status updates without announcing every
second.

# 37. Design System

Create reusable tokens for:

``` text
spacing
radius
typography
surface hierarchy
border
focus
status semantics
```

Do not scatter arbitrary values across components.

Core components: - Button - IconButton - Input - Textarea - Select -
MultiSelect - Checkbox - Radio - Switch - DatePicker - Dialog - Drawer -
Dropdown - Tooltip - Toast - Tabs - Breadcrumb - Card - Table - Badge -
Avatar - Progress - ProgressRing - Skeleton - EmptyState - ErrorState -
PermissionGate - RealtimeIndicator - LiveTimer - AssigneePicker -
PropertyRenderer - StatusPicker - CommandPalette

# 38. Status Semantics

Canonical states have consistent visual semantics across the app:

``` text
not_started
ready
active
paused
blocked
awaiting_approval
completed
cancelled
```

Tenant may rename labels but not destroy semantic consistency.

# 39. Loading Strategy

Use: - skeletons for page-level content, - local spinners only for local
actions, - optimistic updates when safe, - disable only the action
currently pending, not the whole page.

Avoid blank-screen loading.

# 40. Empty States

Examples:

No projects:

``` text
No projects yet.
Create your first project to start organizing work.
[Create Project]
```

No employee tasks:

``` text
You're all caught up.
No ready tasks are currently assigned to you.
```

No active TV work:

``` text
No active work right now.
```

# 41. Error States

Errors must answer: 1. What happened? 2. What can the user do? 3. Was
their data saved?

Example:

``` text
We couldn't save this change.
Your edit is still here.
[Try Again]
```

# 42. Destructive Actions

Low impact/reversible: - execute, - toast + Undo.

High impact:

``` text
Delete Project?

This project contains:
148 cards
32 active tasks

It will be moved to Trash and can be restored.

[Cancel] [Move to Trash]
```

Do not use "Are you sure?" without context.

# 43. User-Facing Activity Timeline

Human language:

``` text
16:42 Mehmet completed Quality Control
16:31 Ayşe uploaded 3 files
15:54 Mehmet started Quality Control
15:42 Abdullah reassigned task from Ahmet to Mehmet
```

Audit log can remain more technical.

# 44. Import UX

Wizard:

``` text
1 Upload
2 Map Columns
3 Validate
4 Preview
5 Import
6 Results
```

Never commit immediately after upload.

Show row-level errors and downloadable failure report later if useful.

# 45. Workflow Simulation UX

Admin can test a draft workflow:

``` text
[Simulate]

Step A completed
→ Step B ready
→ Step C ready

Step B completed
→ Step D remains locked

Step C completed
→ Step D ready
```

No real operational data changes.

# 46. UX Acceptance Scenario

Before V1 is accepted, test with a first-time user:

Admin should be able to:

``` text
Create organization
Create workspace
Invite employee
Create project
Create nested section
Create card
Apply process
Assign employee
```

Employee should then be able to:

``` text
Open My Tasks
Understand what to do
Start
Pause
Resume
Complete
```

Without training documentation.

TV should immediately show active work.

If this requires explanation from a developer, simplify the UX.

------------------------------------------------------------------------

## Final UX Rule

> Every advanced capability must earn the right to appear on screen.
> Complexity belongs in the system; clarity belongs in the interface.

---

# 47. Language, Locale and Terminology UX

The initial/default interface language is **Turkish (`tr-TR`)**.

English must be available as a second supported language from the initial architecture.

User language preference:

```text
User preference
→ Organization default
→ tr-TR
```

Language selection belongs in account/preferences and organization default language belongs in organization settings.

Never mix languages unintentionally on one screen.

## Terminology customization

Language translation and tenant terminology are separate.

Example:

```text
Internal concept: Card

Turkish default: Kart
Tenant override: İş Emri

English default: Card
Tenant override: Job
```

Terminology overrides must flow through navigation, headings, create buttons, empty states, and relevant labels while preserving internal domain/API names.

## Locale formatting

Respect locale/timezone for:

```text
21.09.2026
21 September 2026
1.250,50 ₺
1,250.50 TRY
24-hour / locale-appropriate time
week start
```

Do not manually concatenate localized date/currency strings.

## Layout readiness

Translations may be longer than Turkish labels. Components must tolerate text expansion without clipping.

Architecture should remain compatible with future RTL languages, even though full RTL support is not required for V1.

## V1 language acceptance

The complete core flow must work in Turkish:

```text
Giriş
Projeler
Aktif İşler
Görevlerim
Çalışanlar
Şablonlar
Bildirimler
Raporlar
Ayarlar

Başlat
Duraklat
Devam Et
Tamamla
```

Changing language to English must update the core experience consistently.
