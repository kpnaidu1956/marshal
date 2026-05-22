import { useState, useEffect, useCallback } from 'react'
import { useAuthStore } from '@/stores/auth'
import { detectApiUrls } from '@/lib/config'
import { UserPlus, Check, X, RefreshCw } from 'lucide-react'

interface JoinRequest {
 id: string
 requester_email: string
 requester_first_name: string
 requester_last_name: string
 status: string
 created_at: string
 expires_at: string
}

export function JoinRequestsPage() {
 const token = useAuthStore((s) => s.token)
 const [requests, setRequests] = useState<JoinRequest[]>([])
 const [loading, setLoading] = useState(true)
 const [actionLoading, setActionLoading] = useState<string | null>(null)

 const { ragUrl, apiKey } = detectApiUrls()
 const getHeaders = useCallback(() => {
 const h: Record<string, string> = { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` }
 if (apiKey) h['apikey'] = apiKey
 return h
 }, [token, apiKey])

 const fetchRequests = useCallback(async () => {
 try {
 const res = await fetch(`${ragUrl}/api/admin/join-requests`, { headers: getHeaders() })
 if (res.ok) setRequests(await res.json())
 } catch { /* ignore */ }
 finally { setLoading(false) }
 }, [ragUrl, getHeaders])

 useEffect(() => {
 fetchRequests()
 const interval = setInterval(fetchRequests, 30000)
 return () => clearInterval(interval)
 }, [fetchRequests])

 const handleAction = async (id: string, action: 'approve' | 'reject') => {
 setActionLoading(id)
 try {
 const url = `${ragUrl}/api/admin/join-requests/${id}/${action}`
 const body = action === 'reject' ? JSON.stringify({ reason: null }) : undefined
 const res = await fetch(url, { method: 'POST', headers: getHeaders(), body })
 if (res.ok) {
 setRequests(prev => prev.filter(r => r.id !== id))
 }
 } catch { /* ignore */ }
 finally { setActionLoading(null) }
 }

 const formatDate = (d: string) => {
 try { return new Date(d).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' }) }
 catch { return d }
 }

 return (
 <div className="p-6 max-w-4xl mx-auto">
 <div className="flex items-center justify-between mb-6">
 <div className="flex items-center gap-3">
 <UserPlus className="w-6 h-6 text-indigo-600" />
 <h1 className="text-2xl font-bold text-gray-900">Join Requests</h1>
 </div>
 <button onClick={fetchRequests} className="p-2 rounded-lg hover:bg-gray-100 transition">
 <RefreshCw className="w-4 h-4 text-gray-500" />
 </button>
 </div>

 {loading ? (
 <div className="text-center py-12 text-gray-500">Loading...</div>
 ) : requests.length === 0 ? (
 <div className="text-center py-12 bg-gray-50 rounded-xl border border-gray-200">
 <UserPlus className="w-10 h-10 text-gray-300 mx-auto mb-3" />
 <p className="text-gray-500">No pending join requests</p>
 </div>
 ) : (
 <div className="space-y-3">
 {requests.map(req => (
 <div key={req.id} className="bg-white border border-gray-200 rounded-xl p-4 flex items-center justify-between">
 <div>
 <p className="font-medium text-gray-900">{req.requester_first_name} {req.requester_last_name}</p>
 <p className="text-sm text-gray-500">{req.requester_email}</p>
 <p className="text-xs text-gray-400 mt-1">Requested {formatDate(req.created_at)} &middot; Expires {formatDate(req.expires_at)}</p>
 </div>
 <div className="flex gap-2">
 <button
 onClick={() => handleAction(req.id, 'approve')}
 disabled={actionLoading === req.id}
 className="inline-flex items-center gap-1.5 px-4 py-2 rounded-lg bg-green-600 text-white text-sm font-medium hover:bg-green-700 disabled:opacity-50 transition"
 >
 <Check className="w-4 h-4" /> Approve
 </button>
 <button
 onClick={() => handleAction(req.id, 'reject')}
 disabled={actionLoading === req.id}
 className="inline-flex items-center gap-1.5 px-4 py-2 rounded-lg bg-red-100 text-red-700 text-sm font-medium hover:bg-red-200 disabled:opacity-50 transition"
 >
 <X className="w-4 h-4" /> Reject
 </button>
 </div>
 </div>
 ))}
 </div>
 )}
 </div>
 )
}
