import { useState, useEffect, useCallback } from 'react'
import { useAuthStore } from '@/stores/auth'
import { detectApiUrls } from '@/lib/config'
import { Clock, AlertTriangle } from 'lucide-react'

interface TrialStatus {
 days_remaining: number
 trial_status: string
 quotas: {
 max_users: number
 current_users: number
 max_storage_bytes: number
 current_storage_bytes: number
 max_documents: number
 current_documents: number
 } | null
}

export function TrialBanner() {
 const token = useAuthStore((s) => s.token)
 const [status, setStatus] = useState<TrialStatus | null>(null)

 const fetchStatus = useCallback(() => {
 if (!token) return
 const { ragUrl, apiKey } = detectApiUrls()
 const headers: Record<string, string> = { Authorization: `Bearer ${token}` }
 if (apiKey) headers['apikey'] = apiKey

 fetch(`${ragUrl}/api/trial/status`, { headers })
 .then(r => r.ok ? r.json() : null)
 .then(data => { if (data) setStatus(data) })
 .catch(() => {})
 }, [token])

 useEffect(() => {
 fetchStatus()
 // Refresh every 30 minutes
 const interval = setInterval(fetchStatus, 30 * 60 * 1000)
 return () => clearInterval(interval)
 }, [fetchStatus])

 if (!status || status.trial_status === 'converted') return null

 const days = status.days_remaining
 const isExpired = status.trial_status === 'expired' || status.trial_status === 'suspended'
 const isUrgent = days <= 3
 const isWarning = days <= 7

 const bgColor = isExpired ? 'bg-red-600' : isUrgent ? 'bg-orange-500' : isWarning ? 'bg-yellow-500' : 'bg-indigo-600'
 const textColor = (isWarning && !isUrgent && !isExpired) ? 'text-yellow-900' : 'text-white'

 const formatGB = (bytes: number) => (bytes / (1024 * 1024 * 1024)).toFixed(1)

 return (
 <div className={`${bgColor} ${textColor} px-4 py-1.5 text-xs font-medium flex items-center justify-center gap-4`}>
 <div className="flex items-center gap-1.5">
 {isExpired ? <AlertTriangle className="w-3.5 h-3.5" /> : <Clock className="w-3.5 h-3.5" />}
 <span>
 {isExpired ? 'Trial Expired' : `Trial: ${days} day${days !== 1 ? 's' : ''} remaining`}
 </span>
 </div>
 {status.quotas && !isExpired && (
 <>
 <span className="opacity-60">|</span>
 <span>{status.quotas.current_users}/{status.quotas.max_users} users</span>
 <span className="opacity-60">|</span>
 <span>{formatGB(status.quotas.current_storage_bytes)}/{formatGB(status.quotas.max_storage_bytes)} GB</span>
 </>
 )}
 {isExpired && (
 <a href="/contact" className="ml-2 underline hover:no-underline">Contact Sales</a>
 )}
 </div>
 )
}
