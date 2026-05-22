import { NavLink, useNavigate } from 'react-router-dom'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { useState, useEffect, useRef, useCallback } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { LogOut, User, MessageSquare, ChevronDown } from 'lucide-react'
import { cn } from '@/lib/utils'
// Logo loaded dynamically from org settings; fallback to text-only branding

/* ------------------------------------------------------------------ */
/* Types */
/* ------------------------------------------------------------------ */

interface NavEntry {
 label: string
 to: string
}

interface NavGroup {
 label: string
 items: NavEntry[]
 adminOnly?: boolean
}

type NavItem =
 | { kind: 'link'; label: string; to: string }
 | { kind: 'dropdown'; label: string; items: NavEntry[]; adminOnly?: boolean }

/* ------------------------------------------------------------------ */
/* Nav structure */
/* ------------------------------------------------------------------ */

const NAV_ITEMS: NavItem[] = [
 { kind: 'link', label: 'Home', to: '/' },
 {
 kind: 'dropdown',
 label: 'Workflow',
 items: [
 { label: 'Goals', to: '/goals' },
 { label: 'Tasks', to: '/tasks' },
 { label: 'Workload', to: '/team-workload' },
 ],
 },
 { kind: 'link', label: 'Calendar', to: '/calendar' },
 {
 kind: 'dropdown',
 label: 'Marshal',
 items: [
 { label: 'Ask Marshal', to: '/knowledge-base' },
 { label: 'Knowledge Base', to: '/document-management' },
 ],
 },
 {
 kind: 'dropdown',
 label: 'Analytics',
 items: [
 { label: 'Overview', to: '/analytics/overview' },
 // { label: 'Teams', to: '/analytics/teams' }, // Hidden
 // { label: 'Network', to: '/analytics/network' }, // Hidden
 { label: 'Performance', to: '/analytics/performance' },
 // { label: 'Learning', to: '/analytics/learning' }, // Hidden
 ],
 },
 {
 kind: 'dropdown',
 label: 'BPE',
 items: [
 { label: 'Dashboard', to: '/bpe' },
 { label: 'Workflows', to: '/bpe/workflows' },
 { label: 'Approvals', to: '/bpe/approvals' },
 { label: 'Entities', to: '/bpe/entities' },
 { label: 'Integrations', to: '/bpe/integrations' },
 { label: 'Knowledge', to: '/bpe/knowledge' },
 { label: 'Reports', to: '/bpe/reports' },
 { label: 'Notifications', to: '/bpe/notifications' },
 ],
 },
 {
 kind: 'dropdown',
 label: 'GoTime',
 items: [
 { label: 'Dashboard', to: '/bpe/timekeeping' },
 { label: 'Employee Roster', to: '/bpe/timekeeping/employees' },
 { label: 'Shift Roster', to: '/bpe/timekeeping/roster' },
 { label: 'Time Entry', to: '/bpe/timekeeping/time-entry' },
 { label: 'Reports', to: '/bpe/timekeeping/reports' },
 { label: 'Approvals', to: '/bpe/timekeeping/approvals' },
 { label: 'Flags', to: '/bpe/timekeeping/flags' },
 { label: 'Settings', to: '/bpe/timekeeping/settings' },
 { label: 'Audit Trail', to: '/bpe/timekeeping/audit' },
 ],
 },
 { kind: 'link', label: 'Events', to: '/special-events' },
 {
 kind: 'dropdown',
 label: 'Admin',
 adminOnly: true,
 items: [
 { label: 'Users', to: '/admin/users' },
 { label: 'Groups', to: '/admin/groups' },
 { label: 'Roles', to: '/admin/roles' },
 // { label: 'Organizations', to: '/admin/organizations' }, // Hidden
 ],
 },
]

/* ------------------------------------------------------------------ */
/* Navigation */
/* ------------------------------------------------------------------ */

export function Navigation() {
 const { token, user, logout } = useAuthStore(useShallow((s) => ({ token: s.token, user: s.user, logout: s.logout })))
 const isAdmin = user?.is_platform_admin ?? false
 const { currentOrg, availableOrgs, setAvailableOrgs, setCurrentOrg } = useOrgStore(useShallow((s) => ({ currentOrg: s.currentOrg, availableOrgs: s.availableOrgs, setAvailableOrgs: s.setAvailableOrgs, setCurrentOrg: s.setCurrentOrg })))
 const navigate = useNavigate()

 // Auto-fetch orgs if authenticated but org store is empty (session restore)
 useEffect(() => {
 if (!token || availableOrgs.length > 0) return
 const { ragUrl, apiKey } = detectApiUrls()
 const headers: Record<string, string> = { Authorization: `Bearer ${token}` }
 if (apiKey) headers['apikey'] = apiKey
 fetch(`${ragUrl}/api/auth/organizations`, { headers })
 .then((r) => (r.ok ? r.json() : []))
 .then((orgs) => {
 if (Array.isArray(orgs) && orgs.length) {
 setAvailableOrgs(orgs)
 if (!currentOrg) {
 const userOrg = orgs.find((o: { id: string }) => o.id === user?.organization_id)
 setCurrentOrg(userOrg ?? orgs[0])
 }
 }
 })
 .catch(() => {})
 }, [token, availableOrgs.length, currentOrg, user?.organization_id, setAvailableOrgs, setCurrentOrg])

 // Track which dropdown is open (nav key or 'user' or 'org')
 const [openDropdown, setOpenDropdown] = useState<string | null>(null)
 const navRef = useRef<HTMLElement>(null)

 // Close dropdowns on outside click
 useEffect(() => {
 if (!openDropdown) return
 const handler = (e: MouseEvent) => {
 if (navRef.current && !navRef.current.contains(e.target as Node)) {
 setOpenDropdown(null)
 }
 }
 document.addEventListener('mousedown', handler)
 return () => document.removeEventListener('mousedown', handler)
 }, [openDropdown])

 const toggleDropdown = useCallback(
 (key: string) =>
 setOpenDropdown((prev) => (prev === key ? null : key)),
 [],
 )

 const displayName = user
 ? [user.first_name, user.last_name].filter(Boolean).join(' ') || user.email || 'User'
 : 'User'
 const initials = displayName
 .split(' ')
 .map((w) => w[0])
 .join('')
 .slice(0, 2)
 .toUpperCase()

 const handleLogout = () => {
 logout()
 navigate('/login')
 }

 // Mobile menu
 const [mobileOpen, setMobileOpen] = useState(false)

 return (
 <nav
 ref={navRef}
 className="sticky top-0 z-50 bg-white shadow-sm select-none"
 >
 {/* Main bar */}
 <div className="flex items-center h-[72px] px-4 lg:px-6">
 {/* Left: Org logo + name */}
 <div className="flex items-center gap-3 mr-6 flex-shrink-0">
 <span className="text-lg font-bold text-primary">Marshal</span>
 </div>

 {/* Center: Nav items (desktop) */}
 <div className="hidden lg:flex items-center gap-1 flex-1">
 {NAV_ITEMS.map((item) => {
 if (item.kind === 'dropdown' && item.adminOnly && !isAdmin) return null

 if (item.kind === 'link') {
 return (
 <NavLink
 key={item.label}
 to={item.to}
 end={item.to === '/'}
 className={({ isActive }) =>
 cn(
 'px-3 py-2 rounded-md text-sm font-medium transition-colors',
 isActive
 ? 'text-indigo-600'
 : 'text-gray-500 hover:text-gray-900',
 )
 }
 >
 {item.label}
 </NavLink>
 )
 }

 // Dropdown
 const key = item.label
 const isOpen = openDropdown === key
 return (
 <div key={key} className="relative">
 <button
 onClick={() => toggleDropdown(key)}
 className={cn(
 'flex items-center gap-1 px-3 py-2 rounded-md text-sm font-medium transition-colors',
 isOpen
 ? 'text-indigo-600'
 : 'text-gray-500 hover:text-gray-900',
 )}
 >
 {item.label}
 <ChevronDown
 className={cn('w-3.5 h-3.5 transition-transform', isOpen && 'rotate-180')}
 />
 </button>
 {isOpen && (
 <div className="absolute left-0 top-full mt-1 w-48 bg-white border border-gray-200 rounded-lg shadow-lg py-1 z-50">
 {item.items.map((sub) => (
 <NavLink
 key={sub.to}
 to={sub.to}
 onClick={() => setOpenDropdown(null)}
 className={({ isActive }) =>
 cn(
 'block px-4 py-2 text-sm transition-colors',
 isActive
 ? 'text-indigo-600 bg-indigo-50'
 : 'text-gray-700 hover:bg-gray-100',
 )
 }
 >
 {sub.label}
 </NavLink>
 ))}
 </div>
 )}
 </div>
 )
 })}
 </div>

 {/* Right: Messages + Dark mode + User avatar */}
 <div className="flex items-center gap-2 ml-auto">
 {/* Messages icon */}
 <NavLink
 to="/messages"
 className={({ isActive }) =>
 cn(
 'relative p-2 rounded-lg transition-colors',
 isActive
 ? 'text-indigo-600'
 : 'text-gray-500 hover:text-gray-700',
 )
 }
 >
 <MessageSquare className="w-5 h-5" />
 </NavLink>

 {/* User dropdown */}
 <div className="relative">
 <button
 onClick={(e) => {
 e.stopPropagation()
 toggleDropdown('user')
 }}
 className="flex items-center gap-2 p-1.5 rounded-lg hover:bg-gray-100 transition-colors"
 >
 <div className="w-8 h-8 rounded-full bg-indigo-500 text-white flex items-center justify-center text-xs font-semibold">
 {initials}
 </div>
 <span className="hidden sm:block text-sm text-gray-700 max-w-[120px] truncate">
 {displayName}
 </span>
 <ChevronDown className="hidden sm:block w-3.5 h-3.5 text-gray-500" />
 </button>

 {openDropdown === 'user' && (
 <div className="absolute right-0 top-full mt-1 w-48 bg-white border border-gray-200 rounded-lg shadow-lg py-1 z-50">
 <button
 onClick={() => {
 setOpenDropdown(null)
 navigate('/profile')
 }}
 className="flex items-center gap-2 w-full px-3 py-2 text-sm text-gray-700 hover:bg-gray-100"
 >
 <User className="w-4 h-4" />
 Profile
 </button>
 <button
 onClick={handleLogout}
 className="flex items-center gap-2 w-full px-3 py-2 text-sm text-red-600 hover:bg-red-50"
 >
 <LogOut className="w-4 h-4" />
 Log out
 </button>
 </div>
 )}
 </div>

 {/* Mobile hamburger */}
 <button
 onClick={() => setMobileOpen(!mobileOpen)}
 className="lg:hidden p-2 rounded-lg text-gray-500 hover:bg-gray-100"
 >
 <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
 {mobileOpen ? (
 <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
 ) : (
 <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
 )}
 </svg>
 </button>
 </div>
 </div>

 {/* Bottom accent border (FD-NEW style) */}
 <div className="border-t border-gray-200" />
 <div className="h-[3px] bg-indigo-600" />

 {/* Mobile nav panel */}
 {mobileOpen && (
 <div className="lg:hidden border-t border-gray-200 bg-white shadow-lg">
 <div className="px-4 py-3 space-y-1">
 {NAV_ITEMS.map((item) => {
 if (item.kind === 'dropdown' && item.adminOnly && !isAdmin) return null

 if (item.kind === 'link') {
 return (
 <NavLink
 key={item.label}
 to={item.to}
 end={item.to === '/'}
 onClick={() => setMobileOpen(false)}
 className={({ isActive }) =>
 cn(
 'block px-3 py-2 rounded-md text-sm font-medium',
 isActive
 ? 'text-indigo-600 bg-indigo-50'
 : 'text-gray-600 hover:bg-gray-100',
 )
 }
 >
 {item.label}
 </NavLink>
 )
 }

 // Mobile dropdown section
 return (
 <MobileDropdownSection
 key={item.label}
 label={item.label}
 items={item.items}
 onNavigate={() => setMobileOpen(false)}
 />
 )
 })}
 </div>
 </div>
 )}
 </nav>
 )
}

/* ------------------------------------------------------------------ */
/* Mobile dropdown section */
/* ------------------------------------------------------------------ */

function MobileDropdownSection({
 label,
 items,
 onNavigate,
}: {
 label: string
 items: NavEntry[]
 onNavigate: () => void
}) {
 const [open, setOpen] = useState(false)

 return (
 <div>
 <button
 onClick={() => setOpen(!open)}
 className="flex items-center justify-between w-full px-3 py-2 rounded-md text-sm font-medium text-gray-600 hover:bg-gray-100"
 >
 <span>{label}</span>
 <ChevronDown className={cn('w-4 h-4 transition-transform', open && 'rotate-180')} />
 </button>
 {open && (
 <div className="ml-4 space-y-0.5">
 {items.map((sub) => (
 <NavLink
 key={sub.to}
 to={sub.to}
 onClick={onNavigate}
 className={({ isActive }) =>
 cn(
 'block px-3 py-1.5 rounded-md text-sm',
 isActive
 ? 'text-indigo-600'
 : 'text-gray-500 hover:text-gray-700',
 )
 }
 >
 {sub.label}
 </NavLink>
 ))}
 </div>
 )}
 </div>
 )
}
