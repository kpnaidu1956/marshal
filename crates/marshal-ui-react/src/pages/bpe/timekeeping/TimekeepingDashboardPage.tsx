import { useState, useEffect, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import {
 Loader2,
 RefreshCw,
 Users,
 Clock,
 AlertTriangle,
 CheckCircle,
 Calendar,
 FileText,
 Flag,
 ClipboardCheck,
 Building2,
 Shield,
 ArrowRight,
} from 'lucide-react'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyData = any

interface DashboardState {
 staffing: AnyData | null
 pendingApprovals: AnyData[]
 flags: AnyData[]
 employeeCount: number
}

export function TimekeepingDashboardPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)
 const navigate = useNavigate()

 const [data, setData] = useState<DashboardState>({
 staffing: null,
 pendingApprovals: [],
 flags: [],
 employeeCount: 0,
 })
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)

 const [today] = useState(() => new Date().toISOString().split('T')[0])

 const fetchData = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const [staffingRes, approvalsRes, flagsRes, employeesRes] = await Promise.all([
 client.tkStaffingReport(orgSlug, today).catch(() => ({ data: null })),
 client.tkPendingApprovals(orgSlug).catch(() => ({ data: [] })),
 client.tkListFlags(orgSlug, { resolved: 'false' }).catch(() => ({ data: [] })),
 client.tkListEmployees(orgSlug, { status: 'active' }).catch(() => ({ data: [], total: 0, page: 1, per_page: 50 })),
 ])
 setData({
 staffing: staffingRes.data,
 pendingApprovals: Array.isArray(approvalsRes.data) ? approvalsRes.data : [],
 flags: Array.isArray(flagsRes.data) ? flagsRes.data : [],
 employeeCount: (employeesRes as AnyData).total ?? (Array.isArray(employeesRes.data) ? employeesRes.data.length : 0),
 })
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load timekeeping dashboard')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug, today])

 useEffect(() => { fetchData() }, [fetchData])

 if (!orgSlug) {
 return (
 <div className="text-center py-12">
 <p className="text-gray-500">Select an organization to view the timekeeping dashboard.</p>
 </div>
 )
 }

 if (loading) {
 return (
 <div className="flex items-center justify-center h-64">
 <Loader2 className="w-6 h-6 animate-spin text-indigo-500" />
 </div>
 )
 }

 if (error) {
 return (
 <div className="text-center py-12">
 <p className="text-red-600 mb-4">{error}</p>
 <Button variant="outline" onClick={fetchData}><RefreshCw className="w-4 h-4 mr-2" />Retry</Button>
 </div>
 )
 }

 const { staffing, pendingApprovals, flags, employeeCount } = data

 // Derive staffing info from StaffingReport API response
 // Fields: shift_label, stations[{station_name, actual_staffing, personnel[], is_below_minimum}], total_on_duty, total_absent, alerts
 const onDutyShift = staffing?.shift_label ?? 'N/A'
 const totalOnDuty = staffing?.total_on_duty ?? 0
 const totalAbsent = staffing?.total_absent ?? 0
 const stations: { station_id: string; station_name: string; actual_staffing: number; personnel: string[]; is_below_minimum: boolean }[] =
 Array.isArray(staffing?.stations) ? staffing.stations : []
 const staffingAlerts: string[] = Array.isArray(staffing?.alerts) ? staffing.alerts : []

 // Derive pending action counts
 const pendingApprovalCount = pendingApprovals.length
 const unresolvedFlagCount = flags.length

 // Recent flags (last 5)
 const recentFlags = flags.slice(0, 5)

 const severityColor = (severity: string) => {
 switch (severity?.toLowerCase()) {
 case 'critical': return 'destructive' as const
 case 'high': return 'destructive' as const
 case 'medium': return 'secondary' as const
 default: return 'outline' as const
 }
 }

 const quickLinks = [
 { label: 'Employees', icon: Users, to: '/bpe/timekeeping/employees', color: 'text-blue-600', bg: 'bg-blue-50' },
 { label: 'Roster', icon: Calendar, to: '/bpe/timekeeping/roster', color: 'text-emerald-600', bg: 'bg-emerald-50' },
 { label: 'Time Entry', icon: Clock, to: '/bpe/timekeeping/time-entry', color: 'text-amber-600', bg: 'bg-amber-50' },
 { label: 'Reports', icon: FileText, to: '/bpe/timekeeping/reports', color: 'text-indigo-600', bg: 'bg-indigo-50' },
 { label: 'Flags', icon: Flag, to: '/bpe/timekeeping/flags', color: 'text-rose-600', bg: 'bg-rose-50' },
 { label: 'Approvals', icon: ClipboardCheck, to: '/bpe/timekeeping/approvals', color: 'text-orange-600', bg: 'bg-orange-50' },
 ]

 return (
 <div className="space-y-6">
 {/* Header */}
 <div className="flex items-center justify-between">
 <div>
 <h1 className="text-2xl font-bold text-gray-900">GoTime</h1>
 <p className="text-sm text-gray-500 mt-1">
 {today} &middot; {employeeCount} active employee{employeeCount !== 1 ? 's' : ''}
 </p>
 </div>
 <Button variant="outline" size="sm" onClick={fetchData}>
 <RefreshCw className="w-4 h-4 mr-2" />Refresh
 </Button>
 </div>

 {/* Today's Roster Summary */}
 <Card>
 <CardHeader>
 <CardTitle className="text-lg flex items-center gap-2">
 <Shield className="w-5 h-5 text-emerald-600" />
 Today's Roster Summary
 </CardTitle>
 </CardHeader>
 <CardContent>
 <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-4">
 <div className="flex items-center gap-3">
 <div className="w-10 h-10 rounded-lg bg-emerald-50 flex items-center justify-center">
 <Calendar className="w-5 h-5 text-emerald-600" />
 </div>
 <div>
 <p className="text-xs text-gray-500">On-Duty Shift</p>
 <p className="text-lg font-bold text-gray-900">{onDutyShift} Shift</p>
 </div>
 </div>
 <div className="flex items-center gap-3">
 <div className="w-10 h-10 rounded-lg bg-blue-50 flex items-center justify-center">
 <Users className="w-5 h-5 text-blue-600" />
 </div>
 <div>
 <p className="text-xs text-gray-500">Total On Duty</p>
 <p className="text-lg font-bold text-gray-900">{totalOnDuty}</p>
 </div>
 </div>
 <div className="flex items-center gap-3">
 <div className="w-10 h-10 rounded-lg bg-amber-50 flex items-center justify-center">
 <AlertTriangle className="w-5 h-5 text-amber-600" />
 </div>
 <div>
 <p className="text-xs text-gray-500">Total Absent</p>
 <p className="text-lg font-bold text-gray-900">{totalAbsent}</p>
 </div>
 </div>
 </div>
 {/* Station breakdown */}
 {stations.length > 0 && (
 <TooltipProvider>
 <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
 {stations.map((s) => (
 <Tooltip key={s.station_name}>
  <TooltipTrigger asChild>
  <div className={`rounded-lg border p-3 cursor-pointer ${s.is_below_minimum ? 'border-red-300 bg-red-50' : 'border-border hover:bg-muted/30'}`}>
  <div className="flex items-center justify-between mb-1">
   <span className="text-sm font-semibold">{s.station_name}</span>
   <Badge variant={s.is_below_minimum ? 'destructive' : 'secondary'} className="text-xs">
   {s.actual_staffing} staff
   </Badge>
  </div>
  {s.personnel.length > 0 && (
   <div className="text-xs text-gray-500">
   {s.personnel.slice(0, 3).join(', ')}
   {s.personnel.length > 3 && ` +${s.personnel.length - 3} more`}
   </div>
  )}
  </div>
  </TooltipTrigger>
  <TooltipContent side="bottom" className="max-w-xs">
  <p className="font-semibold text-xs mb-1">{s.station_name} — Roster</p>
  {s.personnel.length > 0 ? (
   <ul className="text-xs space-y-0.5">
   {s.personnel.map((name, i) => (
    <li key={i}>• {name}</li>
   ))}
   </ul>
  ) : (
   <p className="text-xs text-muted-foreground">No personnel assigned</p>
  )}
  </TooltipContent>
 </Tooltip>
 ))}
 </div>
 </TooltipProvider>
 )}
 {/* Staffing alerts */}
 {staffingAlerts.length > 0 && (
 <div className="mt-3 space-y-1">
 {staffingAlerts.map((a, i) => (
 <div key={i} className="flex items-center gap-2 text-xs text-red-600">
 <AlertTriangle className="w-3 h-3" />
 <span>{a}</span>
 </div>
 ))}
 </div>
 )}
 </CardContent>
 </Card>

 {/* Quick Stats + Pending Actions */}
 <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-4">
 {/* Quick Stats */}
 <Card>
 <CardContent className="pt-4 pb-4">
 <div className="w-10 h-10 rounded-lg bg-blue-50 flex items-center justify-center mb-3">
 <Users className="w-5 h-5 text-blue-600" />
 </div>
 <p className="text-2xl font-bold text-gray-900">{employeeCount}</p>
 <p className="text-xs text-gray-500 mt-1">Active Employees</p>
 </CardContent>
 </Card>
 <Card>
 <CardContent className="pt-4 pb-4">
 <div className="w-10 h-10 rounded-lg bg-emerald-50 flex items-center justify-center mb-3">
 <Users className="w-5 h-5 text-emerald-600" />
 </div>
 <p className="text-2xl font-bold text-gray-900">{totalOnDuty}</p>
 <p className="text-xs text-gray-500 mt-1">On Duty Today</p>
 </CardContent>
 </Card>
 <Card>
 <CardContent className="pt-4 pb-4">
 <div className="w-10 h-10 rounded-lg bg-amber-50 flex items-center justify-center mb-3">
 <AlertTriangle className="w-5 h-5 text-amber-600" />
 </div>
 <p className="text-2xl font-bold text-gray-900">{totalAbsent}</p>
 <p className="text-xs text-gray-500 mt-1">Absent Today</p>
 </CardContent>
 </Card>

 {/* Pending Actions */}
 <Card
 className="cursor-pointer hover:shadow-md transition-shadow"
 onClick={() => navigate('/bpe/timekeeping/approvals')}
 >
 <CardContent className="pt-4 pb-4">
 <div className="w-10 h-10 rounded-lg bg-orange-50 flex items-center justify-center mb-3">
 <ClipboardCheck className="w-5 h-5 text-orange-600" />
 </div>
 <p className="text-2xl font-bold text-gray-900">{pendingApprovalCount}</p>
 <p className="text-xs text-gray-500 mt-1">Pending Approvals</p>
 </CardContent>
 </Card>
 <Card
 className="cursor-pointer hover:shadow-md transition-shadow"
 onClick={() => navigate('/bpe/timekeeping/flags')}
 >
 <CardContent className="pt-4 pb-4">
 <div className="w-10 h-10 rounded-lg bg-rose-50 flex items-center justify-center mb-3">
 <Flag className="w-5 h-5 text-rose-600" />
 </div>
 <p className="text-2xl font-bold text-gray-900">{unresolvedFlagCount}</p>
 <p className="text-xs text-gray-500 mt-1">Unresolved Flags</p>
 </CardContent>
 </Card>
 <Card>
 <CardContent className="pt-4 pb-4">
 <div className="w-10 h-10 rounded-lg bg-purple-50 flex items-center justify-center mb-3">
 <Building2 className="w-5 h-5 text-purple-600" />
 </div>
 <p className="text-2xl font-bold text-gray-900">{stations.length}</p>
 <p className="text-xs text-gray-500 mt-1">Stations</p>
 </CardContent>
 </Card>
 </div>

 {/* Recent Validation Flags */}
 <Card>
 <CardHeader>
 <div className="flex items-center justify-between">
 <CardTitle className="text-lg flex items-center gap-2">
 <Flag className="w-5 h-5 text-rose-600" />
 Recent Validation Flags
 </CardTitle>
 {flags.length > 5 && (
 <Button variant="ghost" size="sm" onClick={() => navigate('/bpe/timekeeping/flags')}>
 View All <ArrowRight className="w-4 h-4 ml-1" />
 </Button>
 )}
 </div>
 </CardHeader>
 <CardContent>
 {recentFlags.length === 0 ? (
 <p className="text-sm text-gray-500 py-4 text-center">
 No unresolved validation flags
 </p>
 ) : (
 <div className="space-y-3">
 {recentFlags.map((flag: AnyData, idx: number) => (
 <div
 key={flag.id ?? idx}
 className="flex items-center justify-between py-2 px-3 rounded-lg bg-gray-50"
 >
 <div className="flex items-center gap-3 min-w-0">
 <Badge variant={severityColor(flag.severity ?? flag.level)}>
 {flag.severity ?? flag.level ?? 'info'}
 </Badge>
 <div className="min-w-0">
 <p className="text-sm font-medium text-gray-900 truncate">
 {flag.flag_type ?? flag.type ?? 'Validation Flag'}
 </p>
 <p className="text-xs text-gray-500 truncate">
 {flag.description ?? flag.message ?? ''}
 </p>
 </div>
 </div>
 <span className="text-xs text-gray-400 whitespace-nowrap ml-2">
 {flag.created_at ? new Date(flag.created_at).toLocaleDateString() : ''}
 </span>
 </div>
 ))}
 </div>
 )}
 </CardContent>
 </Card>

 {/* Hidden: Quick Links (now in top nav dropdown)
 <Card>
 <CardHeader>
 <CardTitle className="text-lg">Quick Links</CardTitle>
 </CardHeader>
 <CardContent>
 <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-3">
 {quickLinks.map((link) => (
 <Button
 key={link.label}
 variant="outline"
 className="flex flex-col items-center gap-2 h-auto py-4"
 onClick={() => navigate(link.to)}
 >
 <div className={`w-10 h-10 rounded-lg ${link.bg} flex items-center justify-center`}>
 <link.icon className={`w-5 h-5 ${link.color}`} />
 </div>
 <span className="text-xs font-medium text-gray-700">{link.label}</span>
 </Button>
 ))}
 </div>
 </CardContent>
 </Card>
 */}
 </div>
 )
}
