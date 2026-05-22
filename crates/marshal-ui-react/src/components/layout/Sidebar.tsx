import { NavLink, useLocation } from 'react-router-dom'
import { useSidebarStore } from '@/stores/sidebar'
import { useAuthStore } from '@/stores/auth'
import {
 LayoutGrid,
 CheckSquare,
 Target,
 Users,
 BarChart3,
 Calendar,
 Star,
 BookOpen,
 Network,
 Activity,
 BookOpenText,
 MessageSquare,
 User,
 Shield,
 Briefcase,
 ChevronLeft,
 ChevronRight,
 ChevronDown,
 ChevronUp,
 GitBranch,
 ShieldCheck,
 Database,
 Plug,
 Brain,
 FileText,
 Bell,
 Zap,
 Stethoscope,
 Clock,
 UserCheck,
 CalendarDays,
 ClipboardList,
 AlertTriangle,
 Settings,
 type LucideIcon,
} from 'lucide-react'
import { useState } from 'react'
import { cn } from '@/lib/utils'

interface NavItem {
 label: string
 to: string
 icon: LucideIcon
}

const mainNav: NavItem[] = [
 { label: 'Dashboard', to: '/', icon: LayoutGrid },
 { label: 'Tasks', to: '/tasks', icon: CheckSquare },
 { label: 'Goals', to: '/goals', icon: Target },
 { label: 'Team Assignments', to: '/team-assignments', icon: Users },
 { label: 'Team Workload', to: '/team-workload', icon: BarChart3 },
 { label: 'Calendar', to: '/calendar', icon: Calendar },
 { label: 'Special Events', to: '/special-events', icon: Star },
 { label: 'Knowledge Base', to: '/knowledge-base', icon: BookOpen },
]

const bpeNav: NavItem[] = [
 { label: 'BPE Dashboard', to: '/bpe', icon: Zap },
 { label: 'Workflows', to: '/bpe/workflows', icon: GitBranch },
 { label: 'Approvals', to: '/bpe/approvals', icon: ShieldCheck },
 { label: 'Entities', to: '/bpe/entities', icon: Database },
 { label: 'Integrations', to: '/bpe/integrations', icon: Plug },
 { label: 'Knowledge', to: '/bpe/knowledge', icon: Brain },
 { label: 'Reports', to: '/bpe/reports', icon: FileText },
 { label: 'Notifications', to: '/bpe/notifications', icon: Bell },
 { label: 'Diagnostics', to: '/bpe/diagnostics', icon: Stethoscope },
]

const timekeepingNav: NavItem[] = [
 { label: 'TK Dashboard', to: '/bpe/timekeeping', icon: Clock },
 { label: 'Employees', to: '/bpe/timekeeping/employees', icon: UserCheck },
 { label: 'Shift Roster', to: '/bpe/timekeeping/roster', icon: CalendarDays },
 { label: 'Timecard', to: '/bpe/timekeeping/time-entry', icon: ClipboardList },
 { label: 'Approvals', to: '/bpe/timekeeping/approvals', icon: ShieldCheck },
 { label: 'Reports', to: '/bpe/timekeeping/reports', icon: FileText },
 { label: 'Flags', to: '/bpe/timekeeping/flags', icon: AlertTriangle },
 { label: 'Audit Trail', to: '/bpe/timekeeping/audit', icon: FileText },
 { label: 'Settings', to: '/bpe/timekeeping/settings', icon: Settings },
]

const analyticsNav: NavItem[] = [
 { label: 'Overview', to: '/analytics/overview', icon: BarChart3 },
 { label: 'Teams', to: '/analytics/teams', icon: Users },
 { label: 'Network', to: '/analytics/network', icon: Network },
 { label: 'Performance', to: '/analytics/performance', icon: Activity },
 { label: 'Learning', to: '/analytics/learning', icon: BookOpenText },
]

const adminNav: NavItem[] = [
 { label: 'Users', to: '/admin/users', icon: User },
 { label: 'Groups', to: '/admin/groups', icon: LayoutGrid },
 { label: 'Roles', to: '/admin/roles', icon: Shield },
 { label: 'Organizations', to: '/admin/organizations', icon: Briefcase },
]

function SidebarLink({ item, collapsed }: { item: NavItem; collapsed: boolean }) {
 const closeMobile = useSidebarStore((s) => s.closeMobile)
 const Icon = item.icon

 return (
 <NavLink
 to={item.to}
 end={item.to === '/'}
 onClick={closeMobile}
 className={({ isActive }) =>
 cn(
 'flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors',
 isActive
 ? 'bg-indigo-50 text-indigo-700 font-medium'
 : 'text-gray-600 hover:bg-gray-100',
 collapsed && 'justify-center px-2',
 )
 }
 >
 <Icon className="w-5 h-5 flex-shrink-0" />
 {!collapsed && <span className="truncate">{item.label}</span>}
 </NavLink>
 )
}

function CollapsibleSection({
 title,
 items,
 collapsed,
 defaultOpen = false,
}: {
 title: string
 items: NavItem[]
 collapsed: boolean
 defaultOpen?: boolean
}) {
 const [open, setOpen] = useState(defaultOpen)
 const location = useLocation()
 const isActive = items.some((i) => location.pathname.startsWith(i.to))

 if (collapsed) {
 return (
 <div className="space-y-1">
 {items.map((item) => (
 <SidebarLink key={item.to} item={item} collapsed />
 ))}
 </div>
 )
 }

 return (
 <div>
 <button
 onClick={() => setOpen(!open)}
 className={cn(
 'flex items-center justify-between w-full px-3 py-2 text-xs font-semibold uppercase tracking-wider rounded-lg transition-colors',
 isActive ? 'text-indigo-600' : 'text-gray-500',
 'hover:bg-gray-100',
 )}
 >
 <span>{title}</span>
 {open ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
 </button>
 {open && (
 <div className="mt-1 space-y-0.5 ml-1">
 {items.map((item) => (
 <SidebarLink key={item.to} item={item} collapsed={false} />
 ))}
 </div>
 )}
 </div>
 )
}

export function Sidebar() {
 const collapsed = useSidebarStore((s) => s.collapsed)
 const mobileOpen = useSidebarStore((s) => s.mobileOpen)
 const toggleCollapsed = useSidebarStore((s) => s.toggleCollapsed)
 const closeMobile = useSidebarStore((s) => s.closeMobile)
 const isAdmin = useAuthStore((s) => s.user?.is_platform_admin ?? false)
 const permissions = useAuthStore((s) => s.permissions)
 const canAccessFeature = (feature: string) => {
  if (isAdmin) return true
  if (permissions === null) return true // legacy login without RBAC
  const actions = permissions[feature]
  if (!actions) return false
  return actions.includes('read') || actions.includes('admin')
 }

 return (
 <>
 {/* Mobile backdrop */}
 {mobileOpen && (
 <div className="fixed inset-0 z-40 bg-black/50 md:hidden" onClick={closeMobile} />
 )}

 {/* Sidebar */}
 <aside
 className={cn(
 'fixed top-0 left-0 z-50 h-full bg-white border-r border-gray-200 flex flex-col transition-all duration-200',
 // Mobile
 'md:relative md:z-auto',
 mobileOpen ? 'translate-x-0' : '-translate-x-full md:translate-x-0',
 collapsed ? 'w-16' : 'w-64',
 )}
 >
 {/* Brand */}
 <div className="flex items-center h-14 px-4 border-b border-gray-200">
 {collapsed ? (
 <span className="text-xl font-bold text-indigo-600 mx-auto">M</span>
 ) : (
 <div>
 <span className="text-lg font-bold text-indigo-600">MARSHAL</span>
 <span className="text-[10px] text-gray-500 block -mt-1">AI Management System</span>
 </div>
 )}
 </div>

 {/* Navigation */}
 <nav className="flex-1 overflow-y-auto px-2 py-3 space-y-1">
 {mainNav.map((item) => (
 <SidebarLink key={item.to} item={item} collapsed={collapsed} />
 ))}

 {canAccessFeature('admin') && (
 <div className="pt-3">
 <CollapsibleSection title="BPE" items={bpeNav} collapsed={collapsed} />
 </div>
 )}

 {canAccessFeature('timekeeping') && (
 <div className="pt-3">
 <CollapsibleSection title="Timekeeping" items={timekeepingNav} collapsed={collapsed} />
 </div>
 )}

 {canAccessFeature('analytics') && (
 <div className="pt-3">
 <CollapsibleSection title="Analytics" items={analyticsNav} collapsed={collapsed} defaultOpen />
 </div>
 )}

 {isAdmin && (
 <div className="pt-3">
 <CollapsibleSection title="Admin" items={adminNav} collapsed={collapsed} />
 </div>
 )}

 <div className="pt-3">
 <SidebarLink
 item={{ label: 'Messages', to: '/messages', icon: MessageSquare }}
 collapsed={collapsed}
 />
 </div>
 </nav>

 {/* Collapse toggle (desktop only) */}
 <div className="hidden md:flex items-center justify-center h-12 border-t border-gray-200">
 <button
 onClick={toggleCollapsed}
 className="p-1.5 rounded-lg text-gray-500 hover:bg-gray-100 transition-colors"
 >
 {collapsed ? <ChevronRight className="w-4 h-4" /> : <ChevronLeft className="w-4 h-4" />}
 </button>
 </div>
 </aside>
 </>
 )
}
