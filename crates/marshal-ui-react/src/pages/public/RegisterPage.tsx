import { useState, type FormEvent } from 'react'
import { Link, useNavigate, Navigate } from 'react-router-dom'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { Shield, Sparkles, ArrowLeft, Check, AlertCircle } from 'lucide-react'

type Step = 'email' | 'org' | 'account' | 'legal'

export function RegisterPage() {
 const isAuthenticated = useAuthStore((s) => s.isAuthenticated)
 if (isAuthenticated()) {
  return <Navigate to="/" replace />
 }

 const navigate = useNavigate()
 const login = useAuthStore((s) => s.login)
 const setAvailableOrgs = useOrgStore((s) => s.setAvailableOrgs)
 const setCurrentOrg = useOrgStore((s) => s.setCurrentOrg)

 const [step, setStep] = useState<Step>('email')
 const [error, setError] = useState<string | null>(null)
 const [loading, setLoading] = useState(false)

 // Form data
 const [email, setEmail] = useState('')
 const [orgName, setOrgName] = useState('')
 const [orgDisplayName, setOrgDisplayName] = useState('')
 const [firstName, setFirstName] = useState('')
 const [lastName, setLastName] = useState('')
 const [password, setPassword] = useState('')
 const [confirmPassword, setConfirmPassword] = useState('')
 const [eulaAccepted, setEulaAccepted] = useState(false)
 const [eulaId, setEulaId] = useState('')
 const [eulaContent, setEulaContent] = useState('')

 // Validation state
 const [domainValid, setDomainValid] = useState<boolean | null>(null)
 const [orgAvailable, setOrgAvailable] = useState<boolean | null>(null)
 const [existingOrg, setExistingOrg] = useState<string | null>(null)

 const { ragUrl, apiKey } = detectApiUrls()
 const headers: Record<string, string> = { 'Content-Type': 'application/json' }
 if (apiKey) headers['apikey'] = apiKey

 // --- Step 1: Check domain ---
 const checkDomain = async () => {
 const domain = email.split('@')[1]
 if (!domain) return
 setError(null)
 setLoading(true)
 try {
 const res = await fetch(`${ragUrl}/api/trial/check-domain`, {
 method: 'POST', headers, body: JSON.stringify({ domain })
 })
 const data = await res.json()
 if (data.blocked) {
 setError('This email domain is not allowed for registration.')
 setDomainValid(false)
 return
 }
 if (!data.has_mx) {
 setError('This email domain does not appear to be valid.')
 setDomainValid(false)
 return
 }
 setDomainValid(true)

 // Also check if org exists for this domain
 const orgRes = await fetch(`${ragUrl}/api/trial/check-org`, {
 method: 'POST', headers, body: JSON.stringify({ email_domain: domain })
 })
 const orgData = await orgRes.json()
 if (!orgData.domain_available) {
 setExistingOrg(orgData.existing_org_name)
 } else {
 setExistingOrg(null)
 setStep('org')
 }
 } catch {
 setError('Failed to validate domain. Please try again.')
 } finally {
 setLoading(false)
 }
 }

 // --- Step 2: Check org name ---
 const checkOrgName = async () => {
 if (orgName.length < 3) {
 setError('Organization name must be at least 3 characters.')
 return
 }
 setError(null)
 setLoading(true)
 try {
 const res = await fetch(`${ragUrl}/api/trial/check-org`, {
 method: 'POST', headers, body: JSON.stringify({ name: orgName })
 })
 const data = await res.json()
 if (!data.name_available) {
 setOrgAvailable(false)
 setError('This organization name is already taken.')
 return
 }
 setOrgAvailable(true)
 setStep('account')
 } catch {
 setError('Failed to check organization name.')
 } finally {
 setLoading(false)
 }
 }

 // --- Step 4: Load EULA + Submit ---
 const loadEula = async () => {
 setLoading(true)
 try {
 const res = await fetch(`${ragUrl}/api/trial/eula`, { headers })
 const data = await res.json()
 setEulaId(data.id)
 setEulaContent(data.content)
 setStep('legal')
 } catch {
 setError('Failed to load terms of service.')
 } finally {
 setLoading(false)
 }
 }

 const validateAccount = () => {
 if (!firstName.trim() || !lastName.trim()) {
 setError('First and last name are required.')
 return false
 }
 if (password.length < 8) {
 setError('Password must be at least 8 characters.')
 return false
 }
 if (password !== confirmPassword) {
 setError('Passwords do not match.')
 return false
 }
 setError(null)
 return true
 }

 const handleAccountNext = () => {
 if (validateAccount()) loadEula()
 }

 const handleSubmit = async (e: FormEvent) => {
 e.preventDefault()
 if (!eulaAccepted) {
 setError('You must accept the End User License Agreement.')
 return
 }
 setError(null)
 setLoading(true)

 try {
 const res = await fetch(`${ragUrl}/api/trial/register`, {
 method: 'POST',
 headers,
 body: JSON.stringify({
 email,
 password,
 first_name: firstName,
 last_name: lastName,
 org_name: orgName,
 org_display_name: orgDisplayName || orgName,
 eula_version_id: eulaId,
 recaptcha_token: '',
 }),
 })

 if (!res.ok) {
 const body = await res.json().catch(() => null)
 if (body?.error === 'org_exists') {
 setExistingOrg(body.org_name)
 setStep('email')
 throw new Error(body.message)
 }
 throw new Error(body?.error || `Registration failed (${res.status})`)
 }

 const data = await res.json()
 // Auto-login
 login(data.token, {
 id: data.user.id,
 email: data.user.email,
 first_name: data.user.first_name,
 last_name: data.user.last_name,
 organization_id: data.organization_id,
 is_platform_admin: false,
 } as any)

 const org = { id: data.organization_id, name: data.organization_name, display_name: data.organization_name }
 setAvailableOrgs([org as any])
 setCurrentOrg(org as any)

 navigate('/', { replace: true })
 } catch (err) {
 setError(err instanceof Error ? err.message : 'Registration failed')
 } finally {
 setLoading(false)
 }
 }

 const stepIndicator = (
 <div className="flex items-center gap-2 mb-6">
 {(['email', 'org', 'account', 'legal'] as Step[]).map((s, i) => (
 <div key={s} className="flex items-center gap-2">
 <div className={`w-7 h-7 rounded-full flex items-center justify-center text-xs font-bold ${
 step === s ? 'bg-indigo-600 text-white' :
 (['email', 'org', 'account', 'legal'].indexOf(step) > i) ? 'bg-green-500 text-white' :
 'bg-gray-200 text-gray-500'
 }`}>
 {(['email', 'org', 'account', 'legal'].indexOf(step) > i) ? <Check className="w-3.5 h-3.5" /> : i + 1}
 </div>
 {i < 3 && <div className="w-6 h-0.5 bg-gray-200" />}
 </div>
 ))}
 </div>
 )

 return (
 <div className="min-h-screen flex flex-col lg:flex-row bg-white">
 {/* Left brand panel */}
 <div className="hidden lg:flex lg:w-1/2 xl:w-[55%] relative overflow-hidden bg-gradient-to-br from-indigo-700 via-indigo-800 to-indigo-950 flex-col justify-between p-10 xl:p-14">
 <div className="absolute top-[-20%] left-[-10%] w-[500px] h-[500px] rounded-full bg-indigo-500/20 blur-3xl" />
 <div className="absolute bottom-[-20%] right-[-10%] w-[400px] h-[400px] rounded-full bg-purple-500/15 blur-3xl" />
 <div className="relative z-10">
 <Link to="/login" className="inline-flex items-center gap-2 text-indigo-200 hover:text-white text-sm transition-colors mb-16">
 <ArrowLeft className="w-4 h-4" />
 Back to login
 </Link>
 <div className="flex items-center gap-3 mb-8">
 <div className="w-10 h-10 rounded-xl bg-white/10 backdrop-blur-sm flex items-center justify-center border border-white/10">
 <Shield className="w-5 h-5 text-white" />
 </div>
 <span className="text-xl font-bold text-white tracking-tight">Marshal</span>
 </div>
 <h1 className="text-3xl xl:text-4xl font-bold text-white leading-tight mb-4">
 Start your free<br />90-day trial
 </h1>
 <p className="text-indigo-200 text-lg max-w-md leading-relaxed">
 No credit card required. Full access to all features. Create your organization and invite your team.
 </p>
 </div>
 <div className="relative z-10 space-y-3 text-sm text-indigo-200">
 <p>&#10003; 25 team members included</p>
 <p>&#10003; 10 GB document storage</p>
 <p>&#10003; AI-powered search & analytics</p>
 <p>&#10003; Full workflow automation</p>
 </div>
 </div>

 {/* Right form panel */}
 <div className="flex-1 flex flex-col min-h-screen lg:min-h-0">
 <div className="lg:hidden px-6 py-4 border-b border-gray-200">
 <Link to="/login" className="flex items-center gap-2 text-sm text-gray-600">
 <ArrowLeft className="w-4 h-4" /> Back to login
 </Link>
 </div>

 <div className="flex-1 flex items-center justify-center px-6 py-12">
 <div className="w-full max-w-md">
 <div className="text-center mb-6">
 <div className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full bg-indigo-50 border border-indigo-100 text-xs font-medium text-indigo-600 mb-4">
 <Sparkles className="w-3 h-3" />
 Free 90-Day Trial
 </div>
 <h2 className="text-2xl font-bold text-gray-900">Create your account</h2>
 </div>

 {stepIndicator}

 {error && (
 <div className="text-sm text-red-600 bg-red-50 border border-red-200 rounded-lg p-3 mb-4 flex items-start gap-2">
 <AlertCircle className="w-4 h-4 mt-0.5 flex-shrink-0" />
 <span>{error}</span>
 </div>
 )}

 {existingOrg && (
 <div className="text-sm text-blue-700 bg-blue-50 border border-blue-200 rounded-lg p-3 mb-4">
 <p><strong>{existingOrg}</strong> already uses this email domain.</p>
 <Link to="/join" className="text-blue-600 font-medium hover:underline">
 Request to join this organization &rarr;
 </Link>
 </div>
 )}

 <div className="bg-white border border-gray-200 rounded-2xl p-6 shadow-xl shadow-gray-200/40">
 {/* Step 1: Email */}
 {step === 'email' && (
 <div className="space-y-4">
 <div>
 <label className="block text-sm font-medium text-gray-700 mb-1.5">Work email address</label>
 <input type="email" value={email} onChange={(e) => setEmail(e.target.value)}
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 text-sm focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
 placeholder="you@company.com" autoFocus />
 <p className="text-xs text-gray-500 mt-1">Must be a valid organizational email domain</p>
 </div>
 <button onClick={checkDomain} disabled={!email.includes('@') || loading}
 className="w-full py-2.5 rounded-lg bg-indigo-600 text-white text-sm font-semibold hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed transition">
 {loading ? 'Checking...' : 'Continue'}
 </button>
 </div>
 )}

 {/* Step 2: Organization */}
 {step === 'org' && (
 <div className="space-y-4">
 <div>
 <label className="block text-sm font-medium text-gray-700 mb-1.5">Organization name</label>
 <input type="text" value={orgName} onChange={(e) => setOrgName(e.target.value)}
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 text-sm focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
 placeholder="Acme Corporation" autoFocus />
 {orgAvailable === false && <p className="text-xs text-red-500 mt-1">This name is taken</p>}
 </div>
 <div>
 <label className="block text-sm font-medium text-gray-700 mb-1.5">Display name (optional)</label>
 <input type="text" value={orgDisplayName} onChange={(e) => setOrgDisplayName(e.target.value)}
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 text-sm focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
 placeholder="Acme Corp" />
 </div>
 <div className="flex gap-3">
 <button onClick={() => setStep('email')} className="flex-1 py-2.5 rounded-lg border border-gray-300 text-sm font-medium text-gray-700 hover:bg-gray-50 transition">Back</button>
 <button onClick={checkOrgName} disabled={orgName.length < 3 || loading}
 className="flex-1 py-2.5 rounded-lg bg-indigo-600 text-white text-sm font-semibold hover:bg-indigo-700 disabled:opacity-50 transition">
 {loading ? 'Checking...' : 'Continue'}
 </button>
 </div>
 </div>
 )}

 {/* Step 3: Account */}
 {step === 'account' && (
 <div className="space-y-4">
 <div className="grid grid-cols-2 gap-3">
 <div>
 <label className="block text-sm font-medium text-gray-700 mb-1.5">First name</label>
 <input type="text" value={firstName} onChange={(e) => setFirstName(e.target.value)}
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 text-sm focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
 autoFocus />
 </div>
 <div>
 <label className="block text-sm font-medium text-gray-700 mb-1.5">Last name</label>
 <input type="text" value={lastName} onChange={(e) => setLastName(e.target.value)}
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 text-sm focus:ring-2 focus:ring-indigo-500 focus:border-transparent" />
 </div>
 </div>
 <div>
 <label className="block text-sm font-medium text-gray-700 mb-1.5">Password</label>
 <input type="password" value={password} onChange={(e) => setPassword(e.target.value)}
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 text-sm focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
 placeholder="Min 8 characters" />
 </div>
 <div>
 <label className="block text-sm font-medium text-gray-700 mb-1.5">Confirm password</label>
 <input type="password" value={confirmPassword} onChange={(e) => setConfirmPassword(e.target.value)}
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 text-sm focus:ring-2 focus:ring-indigo-500 focus:border-transparent" />
 </div>
 <div className="flex gap-3">
 <button onClick={() => setStep('org')} className="flex-1 py-2.5 rounded-lg border border-gray-300 text-sm font-medium text-gray-700 hover:bg-gray-50 transition">Back</button>
 <button onClick={handleAccountNext}
 className="flex-1 py-2.5 rounded-lg bg-indigo-600 text-white text-sm font-semibold hover:bg-indigo-700 transition">
 Continue
 </button>
 </div>
 </div>
 )}

 {/* Step 4: Legal */}
 {step === 'legal' && (
 <form onSubmit={handleSubmit} className="space-y-4">
 <div className="max-h-48 overflow-y-auto border border-gray-200 rounded-lg p-3 text-xs text-gray-600 whitespace-pre-wrap bg-gray-50">
 {eulaContent}
 </div>
 <label className="flex items-start gap-2 cursor-pointer">
 <input type="checkbox" checked={eulaAccepted} onChange={(e) => setEulaAccepted(e.target.checked)}
 className="mt-0.5 w-4 h-4 rounded border-gray-300 text-indigo-600 focus:ring-indigo-500" />
 <span className="text-sm text-gray-700">
 I have read and agree to the <Link to="/eula" target="_blank" className="text-indigo-600 hover:underline">End User License Agreement</Link>
 </span>
 </label>
 <div className="flex gap-3">
 <button type="button" onClick={() => setStep('account')} className="flex-1 py-2.5 rounded-lg border border-gray-300 text-sm font-medium text-gray-700 hover:bg-gray-50 transition">Back</button>
 <button type="submit" disabled={loading || !eulaAccepted}
 className="flex-1 py-2.5 rounded-lg bg-indigo-600 text-white text-sm font-semibold hover:bg-indigo-700 disabled:opacity-50 transition">
 {loading ? 'Creating account...' : 'Create Account'}
 </button>
 </div>
 </form>
 )}
 </div>

 <p className="text-center text-sm text-gray-500 mt-6">
 Already have an account?{' '}
 <Link to="/login" className="text-indigo-600 font-medium hover:underline">Sign in</Link>
 </p>
 </div>
 </div>
 </div>
 </div>
 )
}
