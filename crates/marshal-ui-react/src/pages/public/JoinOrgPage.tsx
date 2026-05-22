import { useState, useEffect, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { detectApiUrls } from '@/lib/config'
import { Shield, ArrowLeft, CheckCircle } from 'lucide-react'

export function JoinOrgPage() {
 const [email, setEmail] = useState('')
 const [firstName, setFirstName] = useState('')
 const [lastName, setLastName] = useState('')
 const [password, setPassword] = useState('')
 const [error, setError] = useState<string | null>(null)
 const [success, setSuccess] = useState(false)
 const [orgName, setOrgName] = useState('')
 const [loading, setLoading] = useState(false)
 const [eulaAccepted, setEulaAccepted] = useState(false)
 const [eulaContent, setEulaContent] = useState('')

 useEffect(() => {
  const { ragUrl, apiKey } = detectApiUrls()
  const headers: Record<string, string> = {}
  if (apiKey) headers['apikey'] = apiKey
  fetch(`${ragUrl}/api/trial/eula`, { headers })
   .then(r => r.json())
   .then(data => setEulaContent(data.content || ''))
   .catch(() => {})
 }, [])

 const handleSubmit = async (e: FormEvent) => {
 e.preventDefault()
 setError(null)
 setLoading(true)

 try {
 const { ragUrl, apiKey } = detectApiUrls()
 const headers: Record<string, string> = { 'Content-Type': 'application/json' }
 if (apiKey) headers['apikey'] = apiKey

 const res = await fetch(`${ragUrl}/api/trial/join-request`, {
 method: 'POST',
 headers,
 body: JSON.stringify({ email, first_name: firstName, last_name: lastName, password }),
 })

 if (!res.ok) {
 const body = await res.json().catch(() => null)
 throw new Error(body?.error || `Request failed (${res.status})`)
 }

 const data = await res.json()
 setOrgName(data.organization_name || '')
 setSuccess(true)
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Request failed')
 } finally {
 setLoading(false)
 }
 }

 if (success) {
 return (
 <div className="min-h-screen flex items-center justify-center bg-white px-6">
 <div className="max-w-md text-center">
 <CheckCircle className="w-16 h-16 text-green-500 mx-auto mb-4" />
 <h2 className="text-2xl font-bold text-gray-900 mb-2">Request Sent!</h2>
 <p className="text-gray-600 mb-6">
 Your request to join <strong>{orgName}</strong> has been sent to the organization administrator.
 You'll receive an email when your request is reviewed.
 </p>
 <Link to="/login" className="text-indigo-600 font-medium hover:underline">Back to login</Link>
 </div>
 </div>
 )
 }

 return (
 <div className="min-h-screen flex items-center justify-center bg-white px-6">
 <div className="w-full max-w-md">
 <Link to="/register" className="inline-flex items-center gap-2 text-sm text-gray-500 hover:text-gray-700 mb-8">
 <ArrowLeft className="w-4 h-4" /> Back to registration
 </Link>

 <div className="text-center mb-6">
 <div className="w-12 h-12 rounded-xl bg-indigo-100 flex items-center justify-center mx-auto mb-4">
 <Shield className="w-6 h-6 text-indigo-600" />
 </div>
 <h2 className="text-2xl font-bold text-gray-900">Join an Organization</h2>
 <p className="text-sm text-gray-500 mt-2">
 Your email domain matches an existing organization. Request to join below.
 </p>
 </div>

 {error && (
 <div className="text-sm text-red-600 bg-red-50 border border-red-200 rounded-lg p-3 mb-4">{error}</div>
 )}

 <form onSubmit={handleSubmit} className="bg-white border border-gray-200 rounded-2xl p-6 shadow-xl shadow-gray-200/40 space-y-4">
 <div>
 <label className="block text-sm font-medium text-gray-700 mb-1.5">Work email</label>
 <input type="email" value={email} onChange={(e) => setEmail(e.target.value)} required
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 text-sm focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
 placeholder="you@company.com" autoFocus />
 </div>
 <div className="grid grid-cols-2 gap-3">
 <div>
 <label className="block text-sm font-medium text-gray-700 mb-1.5">First name</label>
 <input type="text" value={firstName} onChange={(e) => setFirstName(e.target.value)} required
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 text-sm focus:ring-2 focus:ring-indigo-500 focus:border-transparent" />
 </div>
 <div>
 <label className="block text-sm font-medium text-gray-700 mb-1.5">Last name</label>
 <input type="text" value={lastName} onChange={(e) => setLastName(e.target.value)} required
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 text-sm focus:ring-2 focus:ring-indigo-500 focus:border-transparent" />
 </div>
 </div>
 <div>
 <label className="block text-sm font-medium text-gray-700 mb-1.5">Password</label>
 <input type="password" value={password} onChange={(e) => setPassword(e.target.value)} required minLength={8}
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 text-sm focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
 placeholder="Min 8 characters" />
 </div>
 {eulaContent && (
 <div className="space-y-3">
  <div className="max-h-32 overflow-y-auto border border-gray-200 rounded-lg p-3 text-xs text-gray-600 whitespace-pre-wrap bg-gray-50">
   {eulaContent}
  </div>
  <label className="flex items-start gap-2 cursor-pointer">
   <input type="checkbox" checked={eulaAccepted} onChange={(e) => setEulaAccepted(e.target.checked)}
    className="mt-0.5 w-4 h-4 rounded border-gray-300 text-indigo-600 focus:ring-indigo-500" />
   <span className="text-sm text-gray-700">I agree to the End User License Agreement</span>
  </label>
 </div>
 )}
 <button type="submit" disabled={loading || !eulaAccepted}
 className="w-full py-2.5 rounded-lg bg-indigo-600 text-white text-sm font-semibold hover:bg-indigo-700 disabled:opacity-50 transition">
 {loading ? 'Sending request...' : 'Request to Join'}
 </button>
 </form>

 <p className="text-center text-sm text-gray-500 mt-6">
 Want to create a new organization?{' '}
 <Link to="/register" className="text-indigo-600 font-medium hover:underline">Start a trial</Link>
 </p>
 </div>
 </div>
 )
}
