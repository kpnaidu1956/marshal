export interface ApiConfig {
 postgrestUrl: string
 ragUrl: string
 apiKey: string
}

// Runtime config loaded from /config.json (injected at deploy time)
let _cachedConfig: ApiConfig | null = null

export async function loadAppConfig(): Promise<void> {
 try {
 const res = await fetch('/config.json')
 if (res.ok) {
 const data = await res.json()
 _cachedConfig = {
 postgrestUrl: data.postgrestUrl || '/api/db',
 ragUrl: data.ragUrl || '',
 apiKey: data.apiKey || '',
 }
 return
 }
 } catch { /* fall through to defaults */ }

 // Default: same-origin relative paths (works behind reverse proxy)
 _cachedConfig = {
 postgrestUrl: '/api/db',
 ragUrl: '',
 apiKey: '',
 }
}

export function detectApiUrls(): ApiConfig {
 if (_cachedConfig) return _cachedConfig

 // Synchronous fallback for code that calls before loadAppConfig completes
 const hostname = window.location.hostname
 if (hostname === 'localhost' || hostname === '127.0.0.1') {
 return {
 postgrestUrl: 'http://localhost:3000',
 ragUrl: 'http://localhost:8080',
 apiKey: '',
 }
 }

 // Production: relative paths behind reverse proxy
 return {
 postgrestUrl: '/api/db',
 ragUrl: '',
 apiKey: '',
 }
}

export function detectBasePath(): string {
 return '/marshal'
}
