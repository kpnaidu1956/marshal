import { useState, type FormEvent } from 'react'
import { Link, useNavigate, Navigate } from 'react-router-dom'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import type { User } from '@/models/user'
import type { Organization } from '@/models/organization'
import { Shield, Sparkles, ArrowLeft, Brain, Target, BarChart3, FileSearch } from 'lucide-react'

interface LoginResponse {
 token: string
 user: User
 organizations: Organization[]
 permissions?: Record<string, string[]>
}

export function LoginPage() {
 const isAuthenticated = useAuthStore((s) => s.isAuthenticated)
 if (isAuthenticated()) {
  return <Navigate to="/" replace />
 }

 const navigate = useNavigate()
 const login = useAuthStore((s) => s.login)
 const setAvailableOrgs = useOrgStore((s) => s.setAvailableOrgs)
 const setCurrentOrg = useOrgStore((s) => s.setCurrentOrg)

 const [email, setEmail] = useState('')
 const [password, setPassword] = useState('')
 const [error, setError] = useState<string | null>(null)
 const [loading, setLoading] = useState(false)

 const handleSubmit = async (e: FormEvent) => {
 e.preventDefault()
 setError(null)
 setLoading(true)

 try {
 const { ragUrl, apiKey } = detectApiUrls()
 const headers: Record<string, string> = {
 'Content-Type': 'application/json',
 }
 if (apiKey) {
 headers['apikey'] = apiKey
 }

 const res = await fetch(`${ragUrl}/api/auth/login`, {
 method: 'POST',
 headers,
 body: JSON.stringify({ email, password }),
 })

 if (!res.ok) {
 const body = await res.json().catch(() => null)
 throw new Error(body?.error || `Login failed (${res.status})`)
 }

 const data: LoginResponse = await res.json()
 login(data.token, data.user, data.permissions ?? undefined)

 // Login response may include organizations; if not, fetch them
 let orgs: Organization[] = data.organizations ?? []
 if (!orgs.length) {
 try {
 const orgsRes = await fetch(`${ragUrl}/api/auth/organizations`, {
 headers: {
 ...headers,
 Authorization: `Bearer ${data.token}`,
 },
 })
 if (orgsRes.ok) orgs = await orgsRes.json()
 } catch { /* proceed without orgs */ }
 }

 if (orgs.length) {
 setAvailableOrgs(orgs)
 // Prefer org matching user's organization_id
 const userOrg = orgs.find((o) => o.id === data.user.organization_id)
 setCurrentOrg(userOrg ?? orgs[0])
 }

 navigate('/', { replace: true })
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Login failed')
 } finally {
 setLoading(false)
 }
 }

 const brandFeatures = [
 { icon: Brain, text: 'AI-powered document search with source citations' },
 { icon: Target, text: 'Hierarchical goal and task management' },
 { icon: BarChart3, text: 'Real-time performance analytics' },
 { icon: FileSearch, text: 'Semantic search across all your documents' },
 ]

 return (
 <div className="min-h-screen flex flex-col lg:flex-row bg-white">
 {/* Left brand panel - hidden on mobile */}
 <div className="hidden lg:flex lg:w-1/2 xl:w-[55%] relative overflow-hidden bg-gradient-to-br from-indigo-700 via-indigo-800 to-indigo-950 flex-col justify-between p-10 xl:p-14">
 {/* Decorative orbs */}
 <div className="absolute top-[-20%] left-[-10%] w-[500px] h-[500px] rounded-full bg-indigo-500/20 blur-3xl" />
 <div className="absolute bottom-[-20%] right-[-10%] w-[400px] h-[400px] rounded-full bg-purple-500/15 blur-3xl" />
 <div className="absolute top-[40%] left-[60%] w-[200px] h-[200px] rounded-full bg-cyan-400/10 blur-2xl" />

 {/* Top: Logo + back */}
 <div className="relative z-10">
 <Link to="/demo" className="inline-flex items-center gap-2 text-indigo-200 hover:text-white text-sm transition-colors mb-16">
 <ArrowLeft className="w-4 h-4" />
 Back to home
 </Link>

 <div className="flex items-center gap-3 mb-8">
 <div className="w-10 h-10 rounded-xl bg-white/10 backdrop-blur-sm flex items-center justify-center border border-white/10">
 <Shield className="w-5 h-5 text-white" />
 </div>
 <span className="text-xl font-bold text-white tracking-tight">MARSHAL</span>
 </div>

 <h1 className="text-3xl xl:text-4xl font-bold text-white leading-tight mb-4">
 The AI-powered platform<br />
 for modern teams
 </h1>
 <p className="text-indigo-200 text-lg max-w-md leading-relaxed">
 Manage goals, track tasks, and unlock insights from your documents with intelligent search.
 </p>
 </div>

 {/* Features list */}
 <div className="relative z-10 space-y-4">
 {brandFeatures.map((feat) => (
 <div key={feat.text} className="flex items-center gap-3">
 <div className="w-8 h-8 rounded-lg bg-white/10 backdrop-blur-sm flex items-center justify-center flex-shrink-0 border border-white/5">
 <feat.icon className="w-4 h-4 text-indigo-200" />
 </div>
 <span className="text-sm text-indigo-100">{feat.text}</span>
 </div>
 ))}
 </div>

 {/* Bottom quote */}
 <div className="relative z-10">
 <div className="border-t border-white/10 pt-6">
 <p className="text-indigo-200 text-sm italic leading-relaxed">
 &ldquo;Marshal transformed how our team manages documents and tracks goals. The AI search alone saved us hours every week.&rdquo;
 </p>
 <p className="text-white/60 text-xs mt-3">Engineering Team Lead, Fortune 500</p>
 </div>
 </div>
 </div>

 {/* Right form panel */}
 <div className="flex-1 flex flex-col min-h-screen lg:min-h-0">
 {/* Mobile header */}
 <div className="lg:hidden px-6 py-4 border-b border-gray-200">
 <div className="flex items-center justify-between">
 <Link to="/demo" className="flex items-center gap-2">
 <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-indigo-500 to-indigo-700 flex items-center justify-center">
 <Shield className="w-4 h-4 text-white" />
 </div>
 <span className="text-lg font-bold text-gray-900">MARSHAL</span>
 </Link>
 <Link to="/demo" className="text-sm text-gray-500 hover:text-gray-700 transition-colors">
 <ArrowLeft className="w-4 h-4 inline mr-1" />
 Home
 </Link>
 </div>
 </div>

 {/* Form area */}
 <div className="flex-1 flex items-center justify-center px-6 py-12">
 <div className="w-full max-w-sm">
 <div className="text-center mb-8">
 <div className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full bg-indigo-50 border border-indigo-100 text-xs font-medium text-indigo-600 mb-5">
 <Sparkles className="w-3 h-3" />
 AI-Powered Management
 </div>
 <h2 className="text-2xl font-bold text-gray-900">Welcome back</h2>
 <p className="text-sm text-gray-500 mt-2">Sign in to your Marshal account</p>
 </div>

 <form
 onSubmit={handleSubmit}
 className="bg-white border border-gray-200 rounded-2xl p-6 shadow-xl shadow-gray-200/40 space-y-5"
 >
 {error && (
 <div className="text-sm text-red-600 bg-red-50 border border-red-200 rounded-lg p-3">
 {error}
 </div>
 )}

 <div>
 <label htmlFor="email" className="block text-sm font-medium text-gray-700 mb-1.5">
 Email address
 </label>
 <input
 id="email"
 type="email"
 value={email}
 onChange={(e) => setEmail(e.target.value)}
 required
 autoFocus
 autoComplete="email"
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 bg-white text-gray-900 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent placeholder-gray-400 transition"
 placeholder="you@example.com"
 />
 </div>

 <div>
 <label htmlFor="password" className="block text-sm font-medium text-gray-700 mb-1.5">
 Password
 </label>
 <input
 id="password"
 type="password"
 value={password}
 onChange={(e) => setPassword(e.target.value)}
 required
 autoComplete="current-password"
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 bg-white text-gray-900 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent placeholder-gray-400 transition"
 placeholder="Enter your password"
 />
 </div>

 <button
 type="submit"
 disabled={loading}
 className="w-full py-2.5 rounded-lg bg-gradient-to-r from-indigo-600 to-indigo-700 text-white text-sm font-semibold hover:from-indigo-700 hover:to-indigo-800 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 disabled:opacity-50 disabled:cursor-not-allowed shadow-md shadow-indigo-500/20 transition-all"
 >
 {loading ? (
 <span className="flex items-center justify-center gap-2">
 <span className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
 Signing in...
 </span>
 ) : (
 'Sign in'
 )}
 </button>
 </form>

 <p className="text-center text-sm text-gray-500 mt-6">
 Don&rsquo;t have an account?{' '}
 <Link to="/register" className="text-indigo-600 font-medium hover:underline">
 Start free 90-day trial
 </Link>
 </p>

 <p className="text-center text-xs text-gray-400 mt-4">
 Powered by Marshal AI &middot; Secure authentication
 </p>
 </div>
 </div>
 </div>
 </div>
 )
}
