# BPE UI/UX Improvement Plan

**Date**: 2026-04-01
**Scope**: All 8 BPE pages, navigation, API client, and TypeScript models
**Files reviewed**: `crates/marshal-ui-react/src/pages/bpe/*.tsx`, `src/components/layout/{Navigation,Sidebar,AppShell}.tsx`, `src/api/bpe.ts`, `src/models/bpe.ts`, `src/App.tsx`

---

## Executive Summary

The BPE frontend is functionally complete with 8 pages covering workflows, approvals, entities, integrations, knowledge, reports, notifications, and a dashboard. The code is well-structured with consistent patterns (auth/org stores, BpeClient, loading/error states). However, there are significant UX gaps: no create/edit forms for any resource, no pagination, missing confirmation dialogs for destructive actions, no search/filter capabilities, and the Sidebar component is defined but not used in the AppShell layout (only Navigation.tsx is rendered). The pages are read-heavy with limited write operations exposed to users.

---

## Cross-Cutting Concerns

### CC-1: Sidebar is Defined But Not Rendered (P0)

The `Sidebar.tsx` file defines a full BPE nav section with 8 items, collapsible sections, and mobile support. However, `AppShell.tsx` only renders `<Navigation />` (the top bar). The Sidebar is never mounted. Users navigate BPE exclusively through the top-bar dropdown, which requires two clicks to reach any BPE sub-page.

**Recommendation**: Either integrate the Sidebar into AppShell as a layout option (e.g., for BPE routes) or remove the dead code. A sidebar layout would be superior for BPE since it has 8 sub-pages and users need frequent switching.

### CC-2: No Create/Edit Forms Exist (P0)

None of the 8 pages have forms for creating or editing resources:
- No "Create Workflow Definition" form
- No "Create Entity Type" or "Create Entity" form
- No "Create Approval Rule" form
- No "Add Integration Credential" form
- No "Create Report Template" form

The API client (`bpe.ts`) has methods for `createDefinition`, `createRule`, `createRequest`, `createCredential`, `createTemplate`, `suggestSequence` -- all unused by the UI.

**Recommendation**: Add modal or inline forms for all create operations. Prioritize workflow definitions and approval rules first, as these are the core BPE setup tasks.

### CC-3: No Consistent Confirmation Pattern (P1)

Destructive actions use inconsistent patterns:
- `BpeIntegrationsPage` uses `confirm('Delete this credential?')` (browser native)
- `BpeKnowledgePage` uses `confirm('Deactivate this learned sequence?')`
- `BpeReportsPage` uses `confirm('Delete this report template?')`
- `BpeApprovalsPage` approve/reject has no confirmation at all
- Workflow cancel has no confirmation

**Recommendation**: Create a shared `<ConfirmDialog>` component using the shadcn AlertDialog pattern. Use it consistently for all destructive actions. Approval decisions should require a confirmation with an optional comment field.

### CC-4: No Search, Filter, or Sort on Any Page (P1)

All list pages display data without any filtering or search capability. As data grows, these pages will become unusable:
- Workflows: No filter by status, category, or source
- Approvals: No filter by status, resource type, or date range
- Entities: No search by name or filter by type
- Notifications: No filter by read/unread, source type
- Reports: No search

**Recommendation**: Add a filter bar component with common patterns: text search (debounced), status dropdown, category dropdown, date range picker. Reuse across all list pages.

### CC-5: No Pagination (P1)

Only the Notifications page hints at pagination (`total > notifications.length` message), but provides no "Load More" or page controls. All other pages fetch all data in a single request with no limit/offset.

**Recommendation**: Implement cursor-based or offset pagination. Add a `<Pagination>` component. The API client already accepts `page` and `perPage` params for notifications; extend this pattern to other endpoints.

### CC-6: Inconsistent Error Display (P2)

Error banners appear inline but have no dismiss button. Some pages show errors differently:
- Dashboard: renders error as a full-page replacement with retry
- Other pages: show error as an inline banner while still displaying stale data

**Recommendation**: Standardize on inline dismissible error banners. Full-page error only when there is no data at all. Add auto-dismiss for transient errors (action failures).

### CC-7: No Toast/Snackbar for Action Feedback (P1)

When users execute a workflow, approve a request, test a credential, or mark notifications read, there is no success feedback. The page silently refetches data.

**Recommendation**: Add a toast notification system (e.g., sonner or react-hot-toast). Show success/failure toasts for all mutation actions.

### CC-8: BpeClient Instantiated Repeatedly (P2)

Every action handler creates `new BpeClient(token)`. This is wasteful and error-prone (no request deduplication, no caching).

**Recommendation**: Create a React context or hook (`useBpeClient()`) that memoizes the client instance and handles token changes. Consider adding SWR or TanStack Query for data fetching with automatic caching, revalidation, and optimistic updates.

### CC-9: No Loading States During Actions (P2)

While the initial page load shows a spinner, most action buttons only disable themselves during mutations. There is no visual indication of which action is in progress when multiple items are visible.

**Recommendation**: Show a spinner inside the button being acted upon (some pages do this, e.g., BpeWorkflowsPage, but not all). Add skeleton loading states for initial data fetch instead of a centered spinner.

### CC-10: BPE Pages Not Lazy-Loaded (P3)

All 8 BPE page components are eagerly imported in `App.tsx`, unlike analytics pages which use `lazy()`. This increases initial bundle size.

**Recommendation**: Wrap BPE page imports in `lazy()` with the same `<Lazy>` wrapper used for analytics pages.

---

## Page-by-Page Assessment

### 1. BPE Dashboard (`BpeDashboardPage.tsx`)

**Current state**: 6 metric cards (entities, workflow defs, active/completed workflows, pending approvals, unread notifications) + workflow performance table. Cards are clickable and navigate to respective pages. Sub-heading shows audit events and learned sequences count.

**Strengths**:
- Good visual hierarchy with card grid + table
- Cards act as navigation shortcuts
- Parallel data fetching (dashboard + performance + unread count)
- Good empty state for performance table

**Issues**:

| ID | Priority | Issue | Recommendation |
|----|----------|-------|----------------|
| D-1 | P1 | No trend indicators on metric cards | Add sparklines or delta indicators (e.g., "+12% vs last week") to give context to raw numbers |
| D-2 | P1 | No recent activity feed | Add a "Recent Activity" section showing the last 5-10 audit events with timestamps and actors |
| D-3 | P2 | Performance table has no visual indicators | Add a colored bar or mini chart for success rate instead of just a percentage badge |
| D-4 | P2 | No quick actions on dashboard | Add "Create Workflow" and "View Pending Approvals" as prominent CTAs |
| D-5 | P2 | Audit events count lacks context | Show a breakdown (e.g., "42 audit events: 18 workflow, 12 approval, 12 entity") |
| D-6 | P3 | Cards use `2xl font-bold` for all values | Large numbers (1000+) will overflow the card width on smaller screens |
| D-7 | P3 | No auto-refresh | Add optional 30s auto-refresh with a visible timer |

### 2. Workflows (`BpeWorkflowsPage.tsx`)

**Current state**: Two-tab layout (Definitions / Executions). Definitions show name, category, version, source, usage stats, and an "Execute" button. Executions show status badge, truncated ID, timestamp, and action buttons (Start/Pause/Resume/Cancel). Expandable timeline for each execution.

**Strengths**:
- Tab pattern effectively separates concerns
- Timeline expansion is a good drill-down pattern
- Status badges have appropriate color coding
- Action buttons are contextually shown based on execution status

**Issues**:

| ID | Priority | Issue | Recommendation |
|----|----------|-------|----------------|
| W-1 | P0 | No "Create Workflow Definition" button or form | Add a "New Workflow" button that opens a form/modal with name, description, category, and a step template builder |
| W-2 | P0 | No way to edit or delete workflow definitions | Add edit/delete actions to each definition card. The API client has `updateDefinition` and `deleteDefinition` methods |
| W-3 | P1 | Execution cards show truncated UUID but no workflow name | Resolve `definition_id` to display the workflow definition name alongside the execution |
| W-4 | P1 | No execution detail view | Clicking an execution should navigate to a detail page showing all steps, their statuses, assigned users, and results |
| W-5 | P1 | No workflow step management | The API has `completeStep`, `skipStep`, `retryStep` but the UI has no way to view or act on individual steps |
| W-6 | P2 | "Execute" button provides no parameter input | `executeDefinition` sends only `organization_id`. If workflows need input parameters or entity context, there is no form for that |
| W-7 | P2 | Tab counts show total, not filtered counts | If filters are added, counts should reflect filtered results |
| W-8 | P2 | Timeline events lack icons | Add icons per event_type (started, completed, failed, etc.) for faster scanning |
| W-9 | P3 | No execution progress indicator | For multi-step workflows, show a progress bar (e.g., "3/7 steps completed") |

### 3. Approvals (`BpeApprovalsPage.tsx`)

**Current state**: Three-tab layout (Pending / All Requests / Rules). Pending tab shows approval cards with Approve/Reject buttons. All Requests tab is a table. Rules tab shows rule cards with metadata.

**Strengths**:
- Pending tab has a good empty state with a shield icon
- Approve/Reject buttons are appropriately styled (primary + destructive)
- Rules display key metadata (resource type, approval type, min approvals, approver count)

**Issues**:

| ID | Priority | Issue | Recommendation |
|----|----------|-------|----------------|
| A-1 | P0 | No "Create Approval Rule" form | Add a form for creating rules. The API has `createRule` |
| A-2 | P0 | No comment field for approval decisions | `decideRequest` sends `comment: null`. Reviewers need to explain their decisions. Add a text area before the Approve/Reject buttons |
| A-3 | P1 | No rule edit/delete capabilities | API has `updateRule`, `deleteRule` -- expose these in the UI |
| A-4 | P1 | "All Requests" table shows raw UUIDs for `requested_by` | Resolve user UUIDs to display names |
| A-5 | P1 | No approval request creation flow | Users cannot request approvals from the UI. Add a "Request Approval" form linked to a specific resource |
| A-6 | P2 | Pending approvals show `resource_id` as truncated UUID | Show the actual resource name/title alongside the ID |
| A-7 | P2 | No decision history per request | Show who approved/rejected and when, with their comments |
| A-8 | P3 | Rules card does not show the actual approver names | Display resolved user names instead of UUIDs in `required_approvers` |

### 4. Entities (`BpeEntitiesPage.tsx`)

**Current state**: Two-tab layout (Entities / Entity Types). Entities tab is a table with name, type, status, created date. Types tab shows cards with name, slug, schema field count.

**Strengths**:
- Type map correctly resolves entity_type_id to names
- Good empty states with icons
- Entity types use a responsive grid layout

**Issues**:

| ID | Priority | Issue | Recommendation |
|----|----------|-------|----------------|
| E-1 | P0 | No "Create Entity" or "Create Entity Type" forms | Add creation forms. These are foundational -- without them, entities can only be created via API |
| E-2 | P1 | Entity detail view missing | Clicking an entity row should show its full `data` JSON, related workflows, and audit history |
| E-3 | P1 | Entity type schema is not viewable | Show the full JSON schema definition, not just a field count |
| E-4 | P2 | No entity status toggle | Users cannot activate/deactivate entities from the UI |
| E-5 | P2 | Table lacks sorting | Allow sorting by name, type, status, created date |
| E-6 | P3 | Entity type cards are too sparse | Show entity count per type, last modified date |

### 5. Integrations (`BpeIntegrationsPage.tsx`)

**Current state**: Two sections -- Available Types (grid of cards) and Configured Credentials (list of cards with Test/Delete actions). Test results display inline below the credential card.

**Strengths**:
- Test result feedback is inline and color-coded
- Integration type cards show required field count
- Test status history (last tested date + success/fail icon)

**Issues**:

| ID | Priority | Issue | Recommendation |
|----|----------|-------|----------------|
| I-1 | P0 | No "Add Credential" form | Users cannot create credentials. Add a form that dynamically renders fields based on the selected integration type's `required_fields` and `optional_fields` |
| I-2 | P1 | Available types show no description | The `IntegrationType` model has a `description` field, but it is not displayed |
| I-3 | P1 | No credential editing | Users must delete and recreate to change a credential |
| I-4 | P2 | Delete uses browser `confirm()` | Replace with a styled confirmation dialog |
| I-5 | P2 | Integration type cards are not actionable | Clicking a type card should pre-select it in the "Add Credential" form |
| I-6 | P3 | No credential name editing | Allow renaming credentials without recreating them |

### 6. Knowledge / Learned Sequences (`BpeKnowledgePage.tsx`)

**Current state**: List of learned sequence cards with name, description, step count, suggestion/acceptance stats, and Promote/Deactivate actions.

**Strengths**:
- Good empty state with explanatory subtitle
- Acceptance rate shown as a badge
- Promote and deactivate actions are appropriately placed

**Issues**:

| ID | Priority | Issue | Recommendation |
|----|----------|-------|----------------|
| K-1 | P1 | No step preview | Users cannot see what steps a learned sequence contains before promoting it. Add an expandable step list |
| K-2 | P1 | "Promote" action gives no feedback | After promoting, show a toast confirming the new workflow definition was created, with a link to it |
| K-3 | P2 | No filtering by acceptance rate | High-acceptance sequences should be surfaced first. Add sort-by-acceptance-rate option |
| K-4 | P2 | Deactivate uses browser `confirm()` | Replace with styled dialog |
| K-5 | P2 | No "Suggest Sequences" trigger | The API has `suggestSequence` but the UI provides no way to request suggestions for a given context |
| K-6 | P3 | No visual distinction for promoted vs unpromoted sequences | After promotion, the sequence should show a "Promoted" badge with a link to the resulting workflow definition |

### 7. Reports (`BpeReportsPage.tsx`)

**Current state**: Grid of report template cards with Run/Delete actions. Running a report displays results in a dynamic table below.

**Strengths**:
- Dynamic table rendering handles arbitrary column schemas
- Report result shows row count and generation timestamp
- Two-column responsive grid for templates

**Issues**:

| ID | Priority | Issue | Recommendation |
|----|----------|-------|----------------|
| R-1 | P0 | No "Create Report Template" form | Add a form with name, description, category, SQL template editor, parameter definitions, and column definitions |
| R-2 | P1 | Report results have no export capability | Add CSV/JSON download buttons for report results |
| R-3 | P1 | No parameter input for parameterized reports | Templates have a `parameters` field, but `runReport` sends only `organization_id`. If a report needs date ranges or filters, there is no input UI |
| R-4 | P2 | Only one report result visible at a time | Allow viewing results history or running multiple reports |
| R-5 | P2 | Large result sets have no pagination | If a report returns 1000+ rows, the table will be enormous |
| R-6 | P2 | Delete uses browser `confirm()` | Replace with styled dialog |
| R-7 | P3 | No report scheduling | Allow users to schedule recurring report runs |
| R-8 | P3 | SQL template not viewable | Power users may want to see/edit the SQL. Show it in a code block with syntax highlighting |

### 8. Notifications (`BpeNotificationsPage.tsx`)

**Current state**: List of notification cards with title, body, source type, timestamps. Unread notifications have a left indigo border accent. "Mark all read" button and per-notification "mark read" button. Shows count when more notifications exist than displayed.

**Strengths**:
- Best empty state design across all BPE pages (large icon + message)
- Unread visual treatment (border + background) is effective
- "Mark all read" only shows when there are unread notifications
- Pagination hint when total exceeds displayed count

**Issues**:

| ID | Priority | Issue | Recommendation |
|----|----------|-------|----------------|
| N-1 | P1 | No "Load More" button or pagination controls | The "showing X of Y" text is informational only. Add a "Load More" button or page controls |
| N-2 | P1 | No link/action from notification to source resource | Notifications reference a `source_type` and `source_id` but clicking a notification does nothing. Should navigate to the relevant workflow execution, approval request, etc. |
| N-3 | P2 | No filter by read/unread or source type | Add filter toggles |
| N-4 | P2 | No notification preferences/settings | Users cannot configure which notifications they receive or via which channel |
| N-5 | P3 | No relative timestamps | Show "2 hours ago" instead of or alongside the full locale string |
| N-6 | P3 | No batch select for mark-read | Allow selecting multiple specific notifications to mark read at once |

---

## Prioritized Action Items

### P0 - Critical (Blocks core functionality)

1. **Add create/edit forms for all resources** -- Without these, the BPE UI is read-only and requires API/CLI for all setup
   - Workflow Definition create/edit form
   - Entity Type and Entity create forms
   - Approval Rule create/edit form
   - Integration Credential create form (dynamic fields from type)
   - Report Template create form
2. **Add comment field to approval decisions** -- Approvals without rationale are operationally useless
3. **Decide on Sidebar vs top-nav for BPE** -- The Sidebar.tsx is dead code; either integrate it or remove it

### P1 - High Priority (Significant UX improvements)

4. Add toast notification system for action feedback
5. Add search/filter capabilities across all list pages
6. Add pagination (starting with Notifications which already has backend support)
7. Resolve UUIDs to human-readable names (executions -> definition names, approvals -> user names)
8. Add workflow execution detail page with step management (complete/skip/retry)
9. Add notification click-through navigation to source resources
10. Add confirmation dialog component to replace browser `confirm()`
11. Add workflow definition delete/edit actions
12. Add approval rule delete/edit actions
13. Add export capability for report results (CSV/JSON)
14. Add learned sequence step preview before promotion
15. Add dashboard recent activity feed
16. Add trend indicators to dashboard metric cards
17. Add report parameter input form for parameterized reports

### P2 - Medium Priority (Polish and consistency)

18. Standardize error display with dismissible banners
19. Replace all browser `confirm()` calls with styled dialog
20. Add skeleton loading states instead of centered spinners
21. Add sorting to all table views
22. Add entity detail view (full data JSON, related workflows)
23. Add integration type descriptions to cards
24. Add credential editing capability
25. Memoize BpeClient via React context or hook
26. Show entity count per entity type
27. Add entity status toggle
28. Show SQL template in report cards for power users

### P3 - Nice to Have

29. Lazy-load BPE page components
30. Add auto-refresh to dashboard with visible timer
31. Add relative timestamps (e.g., "2 hours ago")
32. Add batch notification selection for mark-read
33. Add notification preferences/settings page
34. Add report scheduling
35. Add execution progress indicator for multi-step workflows
36. Add sparklines/mini-charts to dashboard cards
37. Handle large numbers gracefully in metric cards

---

## Recommended Component Additions

### Shared Components to Build

1. **`<ConfirmDialog>`** -- Wraps shadcn AlertDialog. Props: `title`, `description`, `onConfirm`, `variant` (danger/warning/info). Replaces all `confirm()` calls.

2. **`<FilterBar>`** -- Reusable filter component. Props: `searchPlaceholder`, `filters` (array of {label, key, options}), `onFilterChange`. Handles debounced text search + dropdown filters.

3. **`<Pagination>`** -- Standard page controls. Props: `page`, `perPage`, `total`, `onPageChange`.

4. **`<EmptyState>`** -- Standardize the empty state pattern. Props: `icon`, `title`, `description`, `action` (optional button).

5. **`<ResourceForm>`** -- Generic modal form component. Props: `title`, `fields` (schema-driven), `onSubmit`, `loading`.

6. **`<StatusTimeline>`** -- Vertical timeline component for workflow executions. Better visual than the current flat list.

7. **Toast provider** -- Integrate `sonner` or `react-hot-toast` at the AppShell level.

### Data Layer Improvements

1. **`useBpeClient()` hook** -- Memoizes client, handles token refresh.
2. **`useBpeQuery(key, fetcher)` hook** -- SWR-style hook for BPE data with caching, revalidation, and loading/error states built in.
3. **`useBpeMutation(mutator)` hook** -- Handles mutation lifecycle: loading, error, success toast, automatic revalidation.

---

## Navigation Architecture Decision

The current state has two navigation systems defined:
- `Navigation.tsx` (top bar with dropdowns) -- **rendered**
- `Sidebar.tsx` (collapsible sidebar with sections) -- **not rendered**

**Option A: BPE-specific sidebar layout**
Add a `BpeShell` layout component that renders a sidebar for BPE routes only. This provides the best UX for BPE since users frequently switch between 8 sub-pages. The top nav dropdown would link to `/bpe` which enters the sidebar-based layout.

**Option B: Remove Sidebar, improve top nav**
Remove dead Sidebar code. Add breadcrumbs to BPE pages. Improve the top-nav BPE dropdown with icons and descriptions. This keeps the UI simpler but makes BPE navigation slightly slower.

**Recommendation**: Option A. The BPE module is complex enough to warrant its own sidebar navigation. The main app navigation (tasks, goals, calendar, etc.) stays in the top bar. BPE pages get a left sidebar. This is a common pattern for admin/operations tools.

---

## Implementation Order

**Phase 1 (1-2 weeks)**: Foundation
- Build shared components (ConfirmDialog, FilterBar, Pagination, EmptyState, toast system)
- Create `useBpeClient` hook
- Add BPE sidebar layout (or remove dead Sidebar code)
- Lazy-load BPE pages

**Phase 2 (2-3 weeks)**: Core CRUD
- Workflow definition create/edit/delete forms
- Entity type and entity create forms
- Approval rule create/edit/delete forms
- Integration credential create form with dynamic fields
- Report template create form

**Phase 3 (1-2 weeks)**: Enhanced UX
- Search/filter on all list pages
- Pagination on all list pages
- UUID resolution to human-readable names
- Toast feedback for all actions
- Workflow execution detail page with step management

**Phase 4 (1 week)**: Polish
- Dashboard enhancements (activity feed, trends, quick actions)
- Notification click-through navigation
- Report export (CSV/JSON)
- Relative timestamps
- Skeleton loading states
