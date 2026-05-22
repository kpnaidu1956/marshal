import { useState, useEffect } from 'react'
import { Link } from 'react-router-dom'
import { detectApiUrls } from '@/lib/config'
import { ArrowLeft } from 'lucide-react'

export function EulaPage() {
 const [content, setContent] = useState('')
 const [version, setVersion] = useState('')
 const [loading, setLoading] = useState(true)

 useEffect(() => {
 const { ragUrl, apiKey } = detectApiUrls()
 const headers: Record<string, string> = {}
 if (apiKey) headers['apikey'] = apiKey

 fetch(`${ragUrl}/api/trial/eula`, { headers })
 .then(r => r.json())
 .then(data => {
 setContent(data.content || '')
 setVersion(data.version || '')
 })
 .catch(() => setContent('Failed to load EULA.'))
 .finally(() => setLoading(false))
 }, [])

 return (
 <div className="min-h-screen bg-white">
 <div className="max-w-3xl mx-auto px-6 py-12">
 <Link to="/register" className="inline-flex items-center gap-2 text-sm text-gray-500 hover:text-gray-700 mb-8">
 <ArrowLeft className="w-4 h-4" /> Back to registration
 </Link>

 <h1 className="text-3xl font-bold text-gray-900 mb-2">End User License Agreement</h1>
 {version && <p className="text-sm text-gray-500 mb-8">Version {version}</p>}

 {loading ? (
 <div className="animate-pulse space-y-3">
 <div className="h-4 bg-gray-200 rounded w-3/4" />
 <div className="h-4 bg-gray-200 rounded w-full" />
 <div className="h-4 bg-gray-200 rounded w-5/6" />
 </div>
 ) : (
 <div className="prose prose-sm max-w-none whitespace-pre-wrap text-gray-700">
 {content}
 </div>
 )}
 </div>
 </div>
 )
}
