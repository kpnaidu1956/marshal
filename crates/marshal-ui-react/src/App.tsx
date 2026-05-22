import { lazy, Suspense } from 'react'
import { Routes, Route, Navigate } from 'react-router-dom'
import { AuthGuard } from './components/layout/AuthGuard'
import { AdminGuard } from './components/layout/AdminGuard'
import { FeatureGuard } from './components/layout/FeatureGuard'
import { AppShell } from './components/layout/AppShell'
import { ErrorBoundary } from './components/ErrorBoundary'
import { LoginPage } from './pages/public/LoginPage'
const RegisterPage = lazy(() => import('./pages/public/RegisterPage').then(m => ({ default: m.RegisterPage })))
const EulaPage = lazy(() => import('./pages/public/EulaPage').then(m => ({ default: m.EulaPage })))
const JoinOrgPage = lazy(() => import('./pages/public/JoinOrgPage').then(m => ({ default: m.JoinOrgPage })))
const VerifyEmailPage = lazy(() => import('./pages/public/VerifyEmailPage').then(m => ({ default: m.VerifyEmailPage })))
const DemoPage = lazy(() => import('./pages/public/DemoPage').then(m => ({ default: m.DemoPage })))
const PricingPage = lazy(() => import('./pages/public/PricingPage').then(m => ({ default: m.PricingPage })))
const ContactPage = lazy(() => import('./pages/public/ContactPage').then(m => ({ default: m.ContactPage })))
import { DashboardPage } from './pages/DashboardPage'
import { TaskListPage } from './pages/tasks/TaskListPage'
import { TaskDetailPage } from './pages/tasks/TaskDetailPage'
import { GoalListPage } from './pages/goals/GoalListPage'
import { GoalDetailPage } from './pages/goals/GoalDetailPage'
import { TeamAssignmentsPage } from './pages/team/TeamAssignmentsPage'
import { TeamWorkloadPage } from './pages/team/TeamWorkloadPage'
import { TeamDetailPage } from './pages/team/TeamDetailPage'
import { CalendarPage } from './pages/calendar/CalendarPage'
import { SpecialEventsPage } from './pages/calendar/SpecialEventsPage'
import { MessagesPage } from './pages/messages/MessagesPage'
import { ProfilePage } from './pages/profile/ProfilePage'
import { AdminUsersPage } from './pages/admin/AdminUsersPage'
import { AdminOrganizationsPage } from './pages/admin/AdminOrganizationsPage'
import { AdminGroupsPage } from './pages/admin/AdminGroupsPage'
import { AdminRolesPage } from './pages/admin/AdminRolesPage'
import { JoinRequestsPage } from './pages/admin/JoinRequestsPage'
import { NotFoundPage } from './pages/NotFoundPage'
// BPE pages (lazy-loaded)
const BpeDashboardPage = lazy(() => import('./pages/bpe/BpeDashboardPage').then((m) => ({ default: m.BpeDashboardPage })))
const BpeWorkflowsPage = lazy(() => import('./pages/bpe/BpeWorkflowsPage').then((m) => ({ default: m.BpeWorkflowsPage })))
const BpeApprovalsPage = lazy(() => import('./pages/bpe/BpeApprovalsPage').then((m) => ({ default: m.BpeApprovalsPage })))
const BpeEntitiesPage = lazy(() => import('./pages/bpe/BpeEntitiesPage').then((m) => ({ default: m.BpeEntitiesPage })))
const BpeIntegrationsPage = lazy(() => import('./pages/bpe/BpeIntegrationsPage').then((m) => ({ default: m.BpeIntegrationsPage })))
const BpeKnowledgePage = lazy(() => import('./pages/bpe/BpeKnowledgePage').then((m) => ({ default: m.BpeKnowledgePage })))
const BpeNotificationsPage = lazy(() => import('./pages/bpe/BpeNotificationsPage').then((m) => ({ default: m.BpeNotificationsPage })))
const BpeReportsPage = lazy(() => import('./pages/bpe/BpeReportsPage').then((m) => ({ default: m.BpeReportsPage })))
const BpeDiagnosticsPage = lazy(() => import('./pages/bpe/BpeDiagnosticsPage').then((m) => ({ default: m.BpeDiagnosticsPage })))
// Timekeeping pages (lazy-loaded)
const TimekeepingDashboardPage = lazy(() => import('./pages/bpe/timekeeping/TimekeepingDashboardPage').then((m) => ({ default: m.TimekeepingDashboardPage })))
const EmployeeRosterPage = lazy(() => import('./pages/bpe/timekeeping/EmployeeRosterPage').then((m) => ({ default: m.EmployeeRosterPage })))
const ShiftRosterPage = lazy(() => import('./pages/bpe/timekeeping/ShiftRosterPage').then((m) => ({ default: m.ShiftRosterPage })))
const TimeEntryPage = lazy(() => import('./pages/bpe/timekeeping/TimeEntryPage').then((m) => ({ default: m.TimeEntryPage })))
const TimekeepingReportsPage = lazy(() => import('./pages/bpe/timekeeping/TimekeepingReportsPage').then((m) => ({ default: m.TimekeepingReportsPage })))
const ValidationFlagsPage = lazy(() => import('./pages/bpe/timekeeping/ValidationFlagsPage').then((m) => ({ default: m.ValidationFlagsPage })))
const TkApprovalsPage = lazy(() => import('./pages/bpe/timekeeping/ApprovalsPage').then((m) => ({ default: m.ApprovalsPage })))
const TimekeepingSettingsPage = lazy(() => import('./pages/bpe/timekeeping/SettingsPage').then((m) => ({ default: m.TimekeepingSettingsPage })))
const AuditTrailPage = lazy(() => import('./pages/bpe/timekeeping/AuditTrailPage').then((m) => ({ default: m.AuditTrailPage })))

// Lazy-load heavy pages (ECharts, RAG client)
const KnowledgeBasePage = lazy(() => import('./pages/knowledge/KnowledgeBasePage').then((m) => ({ default: m.KnowledgeBasePage })))
const AnalyticsOverviewPage = lazy(() => import('./pages/analytics/AnalyticsOverviewPage').then((m) => ({ default: m.AnalyticsOverviewPage })))
const AnalyticsTeamsPage = lazy(() => import('./pages/analytics/AnalyticsTeamsPage').then((m) => ({ default: m.AnalyticsTeamsPage })))
const AnalyticsNetworkPage = lazy(() => import('./pages/analytics/AnalyticsNetworkPage').then((m) => ({ default: m.AnalyticsNetworkPage })))
const AnalyticsPerformancePage = lazy(() => import('./pages/analytics/AnalyticsPerformancePage').then((m) => ({ default: m.AnalyticsPerformancePage })))
const AnalyticsLearningPage = lazy(() => import('./pages/analytics/AnalyticsLearningPage').then((m) => ({ default: m.AnalyticsLearningPage })))

function PageLoader() {
 return (
 <div className="flex items-center justify-center h-32">
 <div className="w-5 h-5 border-2 border-primary border-t-transparent rounded-full animate-spin" />
 </div>
 )
}

function Lazy({ children }: { children: React.ReactNode }) {
 return (
 <ErrorBoundary>
 <Suspense fallback={<PageLoader />}>{children}</Suspense>
 </ErrorBoundary>
 )
}

export function App() {
 return (
 <ErrorBoundary>
 <Routes>
 {/* Public routes */}
 <Route path="/login" element={<LoginPage />} />
 <Route path="/register" element={<Lazy><RegisterPage /></Lazy>} />
 <Route path="/eula" element={<Lazy><EulaPage /></Lazy>} />
 <Route path="/join" element={<Lazy><JoinOrgPage /></Lazy>} />
 <Route path="/verify-email" element={<Lazy><VerifyEmailPage /></Lazy>} />
 <Route path="/demo" element={<Lazy><DemoPage /></Lazy>} />
 <Route path="/pricing" element={<Lazy><PricingPage /></Lazy>} />
 <Route path="/contact" element={<Lazy><ContactPage /></Lazy>} />

 {/* Protected routes */}
 <Route element={<AuthGuard />}>
 <Route element={<AppShell />}>
 <Route index element={<DashboardPage />} />
 <Route element={<FeatureGuard feature="tasks" />}>
 <Route path="tasks" element={<TaskListPage />} />
 <Route path="tasks/:id" element={<TaskDetailPage />} />
 </Route>
 <Route element={<FeatureGuard feature="goals" />}>
 <Route path="goals" element={<GoalListPage />} />
 <Route path="goals/:id" element={<GoalDetailPage />} />
 </Route>
 <Route path="team-assignments" element={<TeamAssignmentsPage />} />
 <Route path="team-workload" element={<TeamWorkloadPage />} />
 <Route path="team/:id" element={<TeamDetailPage />} />
 <Route path="calendar" element={<CalendarPage />} />
 <Route path="special-events" element={<SpecialEventsPage />} />
 <Route element={<FeatureGuard feature="documents" />}>
 <Route path="knowledge-base" element={<Lazy><KnowledgeBasePage /></Lazy>} />
 <Route path="document-management" element={<Navigate to="/knowledge-base" replace />} />
 </Route>
 <Route path="messages" element={<MessagesPage />} />
 <Route path="profile" element={<ProfilePage />} />

 <Route element={<FeatureGuard feature="admin" />}>
 <Route path="bpe" element={<Lazy><BpeDashboardPage /></Lazy>} />
 <Route path="bpe/workflows" element={<Lazy><BpeWorkflowsPage /></Lazy>} />
 <Route path="bpe/entities" element={<Lazy><BpeEntitiesPage /></Lazy>} />
 <Route path="bpe/integrations" element={<Lazy><BpeIntegrationsPage /></Lazy>} />
 <Route path="bpe/diagnostics" element={<Lazy><BpeDiagnosticsPage /></Lazy>} />
 </Route>
 <Route element={<FeatureGuard feature="approvals" />}>
 <Route path="bpe/approvals" element={<Lazy><BpeApprovalsPage /></Lazy>} />
 </Route>
 <Route element={<FeatureGuard feature="knowledge" />}>
 <Route path="bpe/knowledge" element={<Lazy><BpeKnowledgePage /></Lazy>} />
 </Route>
 <Route path="bpe/notifications" element={<Lazy><BpeNotificationsPage /></Lazy>} />
 <Route element={<FeatureGuard feature="reports" />}>
 <Route path="bpe/reports" element={<Lazy><BpeReportsPage /></Lazy>} />
 </Route>
 {/* Timekeeping */}
 <Route element={<FeatureGuard feature="timekeeping" />}>
 <Route path="bpe/timekeeping" element={<Lazy><TimekeepingDashboardPage /></Lazy>} />
 <Route path="bpe/timekeeping/employees" element={<Lazy><EmployeeRosterPage /></Lazy>} />
 <Route path="bpe/timekeeping/roster" element={<Lazy><ShiftRosterPage /></Lazy>} />
 <Route path="bpe/timekeeping/time-entry" element={<Lazy><TimeEntryPage /></Lazy>} />
 <Route path="bpe/timekeeping/reports" element={<Lazy><TimekeepingReportsPage /></Lazy>} />
 <Route path="bpe/timekeeping/flags" element={<Lazy><ValidationFlagsPage /></Lazy>} />
 <Route path="bpe/timekeeping/approvals" element={<Lazy><TkApprovalsPage /></Lazy>} />
 <Route path="bpe/timekeeping/settings" element={<Lazy><TimekeepingSettingsPage /></Lazy>} />
 <Route path="bpe/timekeeping/audit" element={<Lazy><AuditTrailPage /></Lazy>} />
 </Route>

 <Route element={<FeatureGuard feature="analytics" />}>
 <Route path="analytics/overview" element={<Lazy><AnalyticsOverviewPage /></Lazy>} />
 <Route path="analytics/teams" element={<Navigate to="/analytics/overview" replace />} /> {/* Hidden */}
 <Route path="analytics/network" element={<Navigate to="/analytics/overview" replace />} /> {/* Hidden — redirects to overview */}
 <Route path="analytics/performance" element={<Lazy><AnalyticsPerformancePage /></Lazy>} />
 <Route path="analytics/learning" element={<Navigate to="/analytics/overview" replace />} /> {/* Hidden */}
 </Route>

 <Route element={<AdminGuard />}>
 <Route path="admin/users" element={<AdminUsersPage />} />
 <Route path="admin/groups" element={<AdminGroupsPage />} />
 <Route path="admin/roles" element={<AdminRolesPage />} />
 <Route path="admin/organizations" element={<Navigate to="/admin/users" replace />} /> {/* Hidden */}
 <Route path="admin/join-requests" element={<JoinRequestsPage />} />
 </Route>

 <Route path="*" element={<NotFoundPage />} />
 </Route>
 </Route>
 </Routes>
 </ErrorBoundary>
 )
}
