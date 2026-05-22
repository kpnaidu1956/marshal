import { cn } from '@/lib/utils'

const STATUS_COLORS: Record<string, string> = {
 Completed: 'bg-green-400',
 'In Progress': 'bg-blue-400',
 Assigned: 'bg-gray-300',
 Blocked: 'bg-red-400',
 'On Hold': 'bg-yellow-400',
 Cancelled: 'bg-orange-400',
 completed: 'bg-green-400',
 in_progress: 'bg-blue-400',
 not_started: 'bg-gray-300',
 cancelled: 'bg-red-400',
 on_hold: 'bg-yellow-400',
 blocked: 'bg-orange-400',
}

const STATUS_LABELS: Record<string, string> = {
 in_progress: 'In Progress',
 not_started: 'Not Started',
 on_hold: 'On Hold',
}

export function StatusBadge({ status }: { status: string }) {
 const color = STATUS_COLORS[status] ?? 'bg-gray-300'
 const label = STATUS_LABELS[status] ?? status
 return (
 <span className={cn('inline-flex items-center justify-center w-24 px-2 py-0.5 text-xs font-medium rounded-full text-white', color)}>
 {label}
 </span>
 )
}

const PRIORITY_COLORS: Record<string, string> = {
 Critical: 'bg-red-600',
 High: 'bg-orange-500',
 Medium: 'bg-blue-500',
 Low: 'bg-gray-400',
}

const PRIORITY_ABBREV: Record<string, string> = {
 Critical: 'C',
 High: 'H',
 Medium: 'M',
 Low: 'L',
}

export function PriorityBadge({ priority }: { priority: string | null }) {
 if (!priority) return <span className="text-gray-500">--</span>
 const color = PRIORITY_COLORS[priority] ?? 'bg-gray-400'
 const abbrev = PRIORITY_ABBREV[priority] ?? priority.charAt(0)
 return (
 <span
 className={cn('inline-flex items-center justify-center w-7 h-7 rounded-full text-xs font-bold text-white', color)}
 title={priority}
 >
 {abbrev}
 </span>
 )
}
