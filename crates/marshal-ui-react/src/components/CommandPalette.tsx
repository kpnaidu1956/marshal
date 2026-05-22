import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import {
 Search,
 LayoutGrid,
 CheckSquare,
 Target,
 Users,
 BarChart3,
 Calendar,
 Star,
 BookOpen,
 MessageSquare,
 User,
 Network,
 Activity,
 BookOpenText,
 Shield,
 Briefcase,
 type LucideIcon,
} from 'lucide-react'

interface Command {
 label: string
 to: string
 icon: LucideIcon
 section: string
}

const commands: Command[] = [
 { label: 'Dashboard', to: '/', icon: LayoutGrid, section: 'Navigation' },
 { label: 'Tasks', to: '/tasks', icon: CheckSquare, section: 'Navigation' },
 { label: 'Goals', to: '/goals', icon: Target, section: 'Navigation' },
 // { label: 'Team Assignments', to: '/team-assignments', icon: Users, section: 'Navigation' }, // Hidden
 { label: 'Team Workload', to: '/team-workload', icon: BarChart3, section: 'Navigation' },
 { label: 'Calendar', to: '/calendar', icon: Calendar, section: 'Navigation' },
 { label: 'Special Events', to: '/special-events', icon: Star, section: 'Navigation' },
 { label: 'Knowledge Base', to: '/knowledge-base', icon: BookOpen, section: 'Navigation' },
 { label: 'Messages', to: '/messages', icon: MessageSquare, section: 'Navigation' },
 { label: 'Profile', to: '/profile', icon: User, section: 'Navigation' },
 { label: 'Analytics Overview', to: '/analytics/overview', icon: BarChart3, section: 'Analytics' },
 { label: 'Team Analytics', to: '/analytics/teams', icon: Users, section: 'Analytics' },
 { label: 'Network Analysis', to: '/analytics/network', icon: Network, section: 'Analytics' },
 { label: 'Performance', to: '/analytics/performance', icon: Activity, section: 'Analytics' },
 { label: 'Knowledge & Learning', to: '/analytics/learning', icon: BookOpenText, section: 'Analytics' },
 { label: 'Admin Users', to: '/admin/users', icon: User, section: 'Admin' },
 { label: 'Admin Groups', to: '/admin/groups', icon: LayoutGrid, section: 'Admin' },
 { label: 'Admin Roles', to: '/admin/roles', icon: Shield, section: 'Admin' },
 { label: 'Admin Organizations', to: '/admin/organizations', icon: Briefcase, section: 'Admin' },
]

export function CommandPalette() {
 const [open, setOpen] = useState(false)
 const [query, setQuery] = useState('')
 const [selectedIndex, setSelectedIndex] = useState(0)
 const inputRef = useRef<HTMLInputElement>(null)
 const navigate = useNavigate()

 // Cmd+K / Ctrl+K handler
 useEffect(() => {
 const handler = (e: KeyboardEvent) => {
 if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
 e.preventDefault()
 setOpen((o) => !o)
 }
 if (e.key === 'Escape') {
 setOpen(false)
 }
 }
 document.addEventListener('keydown', handler)
 return () => document.removeEventListener('keydown', handler)
 }, [])

 // Focus input when opened
 useEffect(() => {
 if (open) {
 setQuery('')
 setSelectedIndex(0)
 setTimeout(() => inputRef.current?.focus(), 50)
 }
 }, [open])

 const filtered = useMemo(() => {
 if (!query.trim()) return commands
 const q = query.toLowerCase()
 return commands.filter(
 (c) => c.label.toLowerCase().includes(q) || c.section.toLowerCase().includes(q),
 )
 }, [query])

 // Reset selection when results change
 useEffect(() => {
 setSelectedIndex(0)
 }, [filtered.length])

 const go = useCallback(
 (to: string) => {
 setOpen(false)
 navigate(to)
 },
 [navigate],
 )

 const handleKeyDown = (e: React.KeyboardEvent) => {
 if (e.key === 'ArrowDown') {
 e.preventDefault()
 setSelectedIndex((i) => Math.min(i + 1, filtered.length - 1))
 } else if (e.key === 'ArrowUp') {
 e.preventDefault()
 setSelectedIndex((i) => Math.max(i - 1, 0))
 } else if (e.key === 'Enter' && filtered[selectedIndex]) {
 e.preventDefault()
 go(filtered[selectedIndex].to)
 }
 }

 // Group by section
 const sections = useMemo(() => {
 const map = new Map<string, Command[]>()
 for (const c of filtered) {
 const arr = map.get(c.section) ?? []
 arr.push(c)
 map.set(c.section, arr)
 }
 return map
 }, [filtered])

 if (!open) return null

 let flatIdx = -1

 return (
 <div className="fixed inset-0 z-[100] flex items-start justify-center pt-[15vh]">
 {/* Backdrop */}
 <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" onClick={() => setOpen(false)} />

 {/* Dialog */}
 <div className="relative w-full max-w-lg mx-4 bg-white border border-gray-200 rounded-xl shadow-2xl overflow-hidden">
 {/* Search input */}
 <div className="flex items-center gap-3 px-4 py-3 border-b border-gray-200">
 <Search className="w-4 h-4 text-gray-500 flex-shrink-0" />
 <input
 ref={inputRef}
 value={query}
 onChange={(e) => setQuery(e.target.value)}
 onKeyDown={handleKeyDown}
 placeholder="Search pages..."
 className="flex-1 bg-transparent text-sm text-gray-900 placeholder-gray-400 outline-none"
 />
 <kbd className="hidden sm:inline-flex items-center px-1.5 py-0.5 text-[10px] font-mono text-gray-500 bg-gray-100 rounded border border-gray-200">
 ESC
 </kbd>
 </div>

 {/* Results */}
 <div className="max-h-80 overflow-y-auto py-2">
 {filtered.length === 0 ? (
 <p className="px-4 py-6 text-center text-sm text-gray-500">No results found.</p>
 ) : (
 Array.from(sections.entries()).map(([section, items]) => (
 <div key={section}>
 <p className="px-4 pt-2 pb-1 text-[10px] font-semibold text-gray-500 uppercase tracking-wider">
 {section}
 </p>
 {items.map((cmd) => {
 flatIdx++
 const idx = flatIdx
 const Icon = cmd.icon
 return (
 <button
 key={cmd.to}
 onClick={() => go(cmd.to)}
 onMouseEnter={() => setSelectedIndex(idx)}
 className={`flex items-center gap-3 w-full px-4 py-2 text-sm transition-colors ${
 idx === selectedIndex
 ? 'bg-indigo-50 text-indigo-700'
 : 'text-gray-700 hover:bg-gray-50'
 }`}
 >
 <Icon className="w-4 h-4 flex-shrink-0 opacity-60" />
 <span>{cmd.label}</span>
 </button>
 )
 })}
 </div>
 ))
 )}
 </div>

 {/* Footer hint */}
 <div className="flex items-center gap-4 px-4 py-2 border-t border-gray-200 text-[10px] text-gray-500">
 <span><kbd className="font-mono">&#8593;&#8595;</kbd> navigate</span>
 <span><kbd className="font-mono">&#9166;</kbd> select</span>
 <span><kbd className="font-mono">esc</kbd> close</span>
 </div>
 </div>
 </div>
 )
}
