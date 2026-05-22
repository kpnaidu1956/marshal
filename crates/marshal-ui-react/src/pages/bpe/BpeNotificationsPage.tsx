import { useState, useEffect, useCallback } from 'react'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { BpeClient } from '@/api/bpe'
import { Loader2, Bell, CheckCheck, Mail, RefreshCw } from 'lucide-react'
import type { BpeNotification } from '@/models/bpe'

export function BpeNotificationsPage() {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [notifications, setNotifications] = useState<BpeNotification[]>([])
 const [total, setTotal] = useState(0)
 const [unread, setUnread] = useState(0)
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)

 const fetchData = useCallback(async () => {
 if (!token || !orgSlug) return
 setLoading(true)
 setError(null)
 try {
 const client = new BpeClient(token)
 const [notifs, count] = await Promise.all([
 client.listNotifications(orgSlug),
 client.unreadCount(orgSlug),
 ])
 setNotifications(notifs.data)
 setTotal(notifs.total)
 setUnread(count.unread_count)
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to load notifications')
 } finally {
 setLoading(false)
 }
 }, [token, orgSlug])

 useEffect(() => { fetchData() }, [fetchData])

 const markAllRead = async () => {
 if (!token || !orgSlug) return
 try {
 const client = new BpeClient(token)
 await client.markAllRead(orgSlug)
 await fetchData()
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to mark all read')
 }
 }

 const markRead = async (ids: string[]) => {
 if (!token) return
 try {
 const client = new BpeClient(token)
 await client.markRead(ids)
 await fetchData()
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Failed to mark read')
 }
 }

 if (!orgSlug) {
 return <div className="text-center py-12"><p className="text-gray-500">Select an organization to view notifications.</p></div>
 }

 if (loading) {
 return <div className="flex items-center justify-center h-64"><Loader2 className="w-6 h-6 animate-spin text-indigo-500" /></div>
 }

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <div className="flex items-center gap-3">
 <h1 className="text-2xl font-bold text-gray-900">Notifications</h1>
 {unread > 0 && (
 <Badge variant="destructive">{unread} unread</Badge>
 )}
 </div>
 <div className="flex gap-2">
 {unread > 0 && (
 <Button variant="outline" size="sm" onClick={markAllRead}>
 <CheckCheck className="w-4 h-4 mr-2" />Mark all read
 </Button>
 )}
 <Button variant="outline" size="sm" onClick={fetchData}>
 <RefreshCw className="w-4 h-4 mr-2" />Refresh
 </Button>
 </div>
 </div>

 {error && <div className="text-red-600 text-sm bg-red-50 p-3 rounded-lg">{error}</div>}

 {notifications.length === 0 ? (
 <div className="text-center py-16">
 <Bell className="w-16 h-16 mx-auto text-gray-300 mb-4" />
 <p className="text-gray-500">No notifications</p>
 </div>
 ) : (
 <div className="space-y-2">
 {notifications.map((n) => (
 <Card
 key={n.id}
 className={`transition-colors ${!n.is_read ? 'border-l-4 border-l-indigo-500 bg-indigo-50/30' : ''}`}
 >
 <CardContent className="pt-3 pb-3">
 <div className="flex items-start justify-between gap-4">
 <div className="flex-1 min-w-0">
 <div className="flex items-center gap-2 mb-1">
 <h3 className={`text-sm ${!n.is_read ? 'font-semibold text-gray-900' : 'font-medium text-gray-600'}`}>
 {n.title}
 </h3>
 <Badge variant="outline" className="text-[10px]">{n.source_type}</Badge>
 {n.channel !== 'in_app' && (
 <Mail className="w-3 h-3 text-gray-400" />
 )}
 </div>
 {n.body && (
 <p className="text-sm text-gray-500">{n.body}</p>
 )}
 <p className="text-xs text-gray-400 mt-1">
 {new Date(n.created_at).toLocaleString()}
 {n.is_read && n.read_at && (
 <span className="ml-2">&middot; Read {new Date(n.read_at).toLocaleString()}</span>
 )}
 </p>
 </div>
 {!n.is_read && (
 <Button size="sm" variant="ghost" onClick={() => markRead([n.id])}>
 <CheckCheck className="w-3.5 h-3.5" />
 </Button>
 )}
 </div>
 </CardContent>
 </Card>
 ))}

 {total > notifications.length && (
 <p className="text-center text-xs text-gray-400 py-2">
 Showing {notifications.length} of {total} notifications
 </p>
 )}
 </div>
 )}
 </div>
 )
}
