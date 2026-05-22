import { useState, useEffect } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { detectApiUrls } from '@/lib/config'
import { CheckCircle, XCircle, Loader2 } from 'lucide-react'

export function VerifyEmailPage() {
 const [searchParams] = useSearchParams()
 const token = searchParams.get('token')
 const [status, setStatus] = useState<'loading' | 'success' | 'error'>('loading')
 const [message, setMessage] = useState('')

 useEffect(() => {
 if (!token) {
 setStatus('error')
 setMessage('No verification token provided.')
 return
 }

 const { ragUrl, apiKey } = detectApiUrls()
 const headers: Record<string, string> = { 'Content-Type': 'application/json' }
 if (apiKey) headers['apikey'] = apiKey

 fetch(`${ragUrl}/api/trial/verify-email`, {
 method: 'POST',
 headers,
 body: JSON.stringify({ token }),
 })
 .then(async (res) => {
 if (res.ok) {
 setStatus('success')
 setMessage('Your email has been verified successfully!')
 } else {
 const body = await res.json().catch(() => null)
 setStatus('error')
 setMessage(body?.error || 'Verification failed. The link may have expired.')
 }
 })
 .catch(() => {
 setStatus('error')
 setMessage('Verification failed. Please try again later.')
 })
 }, [token])

 return (
 <div className="min-h-screen flex items-center justify-center bg-white px-6">
 <div className="max-w-md text-center">
 {status === 'loading' && (
 <>
 <Loader2 className="w-16 h-16 text-indigo-500 mx-auto mb-4 animate-spin" />
 <h2 className="text-xl font-bold text-gray-900">Verifying your email...</h2>
 </>
 )}

 {status === 'success' && (
 <>
 <CheckCircle className="w-16 h-16 text-green-500 mx-auto mb-4" />
 <h2 className="text-2xl font-bold text-gray-900 mb-2">Email Verified!</h2>
 <p className="text-gray-600 mb-6">{message}</p>
 <Link to="/login" className="inline-flex px-6 py-2.5 rounded-lg bg-indigo-600 text-white text-sm font-semibold hover:bg-indigo-700 transition">
 Go to Login
 </Link>
 </>
 )}

 {status === 'error' && (
 <>
 <XCircle className="w-16 h-16 text-red-500 mx-auto mb-4" />
 <h2 className="text-2xl font-bold text-gray-900 mb-2">Verification Failed</h2>
 <p className="text-gray-600 mb-6">{message}</p>
 <Link to="/login" className="inline-flex px-6 py-2.5 rounded-lg bg-indigo-600 text-white text-sm font-semibold hover:bg-indigo-700 transition">
 Go to Login
 </Link>
 </>
 )}
 </div>
 </div>
 )
}
