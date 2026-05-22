import { useState, useEffect, useMemo, useRef, type FormEvent, type ChangeEvent } from 'react'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import { RagClient } from '@/api/rag'
import type { RagDocument, QueryResponse, Citation } from '@/api/rag'
import { loadRecent, saveRecent } from '@/lib/recent-queries'
import { orgNameToSlug } from '@/models/organization'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { ScrollArea } from '@/components/ui/scroll-area'
import {
 MessageSquare, Search, FileText, Send, Loader2, Copy, Upload, Trash2, Archive,
 ArchiveRestore, RefreshCw, Eye, Printer, X, ExternalLink, Lock, Globe, Shield,
} from 'lucide-react'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { BpeClient } from '@/api/bpe'
import { toast } from 'sonner'

/* ── helpers ─────────────────────────────────────────────────────── */

function formatFileSize(bytes: number) {
 if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(1)} MB`
 if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`
 return `${bytes} B`
}

function fileTypeBadgeClass(ft: string) {
 switch (ft.toLowerCase()) {
 case 'pdf': return 'bg-red-100 text-red-800'
 case 'docx': case 'doc': return 'bg-blue-100 text-blue-800'
 case 'xlsx': case 'xls': return 'bg-green-100 text-green-800'
 default: return 'bg-gray-100 text-gray-700'
 }
}

function dedupCitations(citations: Citation[]): Citation[] {
 // Dedup by content snippet (first 80 chars) to merge duplicate PDF/DOCX versions of same doc
 const map = new Map<string, Citation>()
 for (const c of citations) {
 const snippet = (c.snippet?.preview || c.snippet?.text || '').slice(0, 80).toLowerCase().trim()
 const key = snippet || c.source.chunk_id || `${c.source.filename}|${c.source.page}`
 const existing = map.get(key)
 if (!existing || c.relevance.score > existing.relevance.score) map.set(key, c)
 }
 return [...map.values()].sort((a, b) => b.relevance.score - a.relevance.score)
}

/** Strip inline [Source: ...] markers and trailing source blocks from LLM answer text */
function stripInlineSources(text: string): string {
 return text
  .replace(/\s*\[Source:[^\]]*\]/g, '') // remove [Source: filename, Page X] markers
  .replace(/\n*Sources?\s*used:[\s\S]*/gi, '') // remove "Sources used:" block and everything after
  .replace(/\n*References?:[\s\S]*/gi, '') // remove "References:" block and everything after
  .replace(/\n*-\s*\[Source:[^\]]*\]\s*/g, '') // remove bullet-style source lines
  .replace(/\n{3,}/g, '\n\n') // collapse excessive newlines
  .trim()
}

function relevanceColor(score: number) {
 if (score >= 90) return 'text-emerald-600'
 if (score >= 75) return 'text-blue-600'
 if (score >= 50) return 'text-amber-600'
 return 'text-muted-foreground'
}

interface ChatMessage {
 role: 'user' | 'assistant'
 content: string
 citations?: Citation[]
}

/* ── AI Chat Tab ─────────────────────────────────────────────────── */

function AIChatTab({ slug, ragUrl, apiKey, token }: { slug: string; ragUrl: string; apiKey: string; token: string | null }) {
 const [messages, setMessages] = useState<ChatMessage[]>([])
 const [input, setInput] = useState('')
 const [loading, setLoading] = useState(false)
 const [recent, setRecent] = useState<string[]>([])
 const bottomRef = useRef<HTMLDivElement>(null)

 useEffect(() => {
 setRecent(loadRecent(slug))
 }, [slug])

 useEffect(() => {
 bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
 }, [messages])

 const executeQuery = async (q: string) => {
 if (!q.trim() || loading) return
 const recents = [q, ...loadRecent(slug).filter((r) => r !== q)].slice(0, 10)
 saveRecent(slug, recents)
 setRecent(recents)

 const userMsg: ChatMessage = { role: 'user', content: q }
 setMessages((prev) => [...prev, userMsg])
 setInput('')
 setLoading(true)

 try {
 const client = new RagClient(ragUrl, apiKey, token)
 const resp = await client.queryV2({ question: q, organization_id: slug })
 setMessages((prev) => [...prev, { role: 'assistant', content: resp.answer, citations: resp.citations }])
 } catch (err) {
 const msg = err instanceof Error ? err.message : typeof err === 'object' && err !== null && 'message' in err ? String((err as {message:string}).message) : typeof err === 'object' && err !== null && 'type' in err ? String((err as {type:string}).type) : 'Query failed'
 toast.error(msg)
 setMessages((prev) => prev.slice(0, -1)) // remove user msg on error
 } finally {
 setLoading(false)
 }
 }

 const onSubmit = (e: FormEvent) => { e.preventDefault(); executeQuery(input) }

 const handleCopy = async (content: string, cites?: Citation[]) => {
 const sourceText = cites?.length
 ? `\n\nSources:\n${dedupCitations(cites).map((c) => `- ${c.source.filename}${c.source.page ? ` (p. ${c.source.page})` : ''}`).join('\n')}`
 : ''
 await navigator.clipboard.writeText(`${content}${sourceText}`)
 toast.success('Copied to clipboard')
 }

 return (
 <div className="flex h-[600px] border rounded-lg bg-background overflow-hidden">
 {/* Main chat area */}
 <div className="flex flex-col flex-1 min-w-0">
 {/* Input */}
 <form onSubmit={onSubmit} className="p-4 border-b">
  <div className="flex gap-2">
  <Textarea
   value={input}
   onChange={(e) => setInput(e.target.value)}
   placeholder="Ask a question about your documents..."
   className="resize-none"
   rows={2}
   disabled={loading}
   onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); onSubmit(e) } }}
  />
  <Button type="submit" size="icon" disabled={!input.trim() || loading} className="shrink-0 h-auto">
   {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : <Send className="h-4 w-4" />}
  </Button>
  </div>
  <p className="text-xs text-muted-foreground mt-1">Press Enter to send, Shift+Enter for new line</p>
 </form>

 {messages.length > 0 && (
  <div className="px-4 py-2 border-b bg-muted/30 flex justify-end">
  <Button variant="outline" size="sm" onClick={() => setMessages([])}>New Chat</Button>
  </div>
 )}

 {/* Messages */}
 <ScrollArea className="flex-1">
  <div className="p-4 space-y-4">
  {messages.length === 0 && !loading && (
   <div className="text-center text-muted-foreground py-12">
   <MessageSquare className="h-10 w-10 mx-auto mb-3 opacity-30" />
   <p className="text-sm">Start a conversation by asking a question</p>
   </div>
 )}

 {messages.map((msg, i) => {
 const deduped = msg.citations ? dedupCitations(msg.citations) : []
 return (
 <div key={i} className={`flex ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}>
 <Card className={`max-w-[85%] p-4 ${msg.role === 'user' ? 'bg-primary text-primary-foreground' : 'bg-muted'}`}>
 <div className="text-sm whitespace-pre-wrap leading-relaxed">{msg.role === 'assistant' ? stripInlineSources(msg.content) : msg.content}</div>

 {msg.role === 'assistant' && (
 <div className="mt-2 pt-2 border-t border-border/40">
 <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" onClick={() => handleCopy(msg.content, msg.citations)}>
 <Copy className="h-3 w-3 mr-1" /> Copy
 </Button>
 </div>
 )}

 {deduped.length > 0 && (
 <div className="mt-2">
 <p className="text-xs font-semibold mb-1 flex items-center gap-1">
 <FileText className="h-3 w-3" /> Sources ({deduped.length}):
 </p>
 <div className="space-y-1.5">
 {deduped.map((c, idx) => (
 <div key={idx} className="rounded border border-border/50 p-1.5 text-xs">
  <div className="flex items-center gap-2">
  <span className="truncate font-medium">{c.source.filename}</span>
  <span className={`font-medium shrink-0 ${relevanceColor(c.relevance.score)}`}>
   {Math.round(c.relevance.score)}%
  </span>
  </div>
  {c.snippet?.preview && (
  <p className="text-[11px] text-muted-foreground mt-1 line-clamp-2">{c.snippet.preview}</p>
  )}
 </div>
 ))}
 </div>
 </div>
 )}
 </Card>
 </div>
 )
 })}

 {loading && (
 <div className="flex justify-start">
 <Card className="max-w-[85%] p-4 bg-muted">
 <div className="flex items-center gap-2 text-sm">
 <Loader2 className="h-4 w-4 animate-spin" />
 Searching and generating answer...
 </div>
 </Card>
 </div>
 )}

 <div ref={bottomRef} />
 </div>
 </ScrollArea>
 </div>

 {/* Right sidebar — Recent queries */}
 {recent.length > 0 && (
 <div className="w-64 border-l bg-muted/20 flex flex-col shrink-0">
  <div className="px-3 py-2 border-b">
  <p className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">Recent Queries</p>
  </div>
  <ScrollArea className="flex-1">
  <div className="p-2 space-y-1">
   {recent.map((q) => (
   <button
    key={q}
    type="button"
    className="w-full text-left px-3 py-2 text-sm bg-background hover:bg-primary/10 rounded-lg transition-colors break-words whitespace-normal leading-snug"
    onClick={() => { setInput(q); executeQuery(q) }}
   >
    {q}
   </button>
   ))}
  </div>
  </ScrollArea>
 </div>
 )}
 </div>
 )
}

/* ── Text Search Tab ──────────────────────────────────────────────── */

interface StringSearchResult {
 document_id: string
 document_name: string
 chunk_text: string
 page_number?: number
 score?: number
}

function TextSearchTab({ slug, ragUrl, apiKey, token }: { slug: string; ragUrl: string; apiKey: string; token: string | null }) {
 const [query, setQuery] = useState('')
 const [results, setResults] = useState<StringSearchResult[]>([])
 const [searching, setSearching] = useState(false)
 const [searched, setSearched] = useState(false)

 const handleSearch = async (e: FormEvent) => {
 e.preventDefault()
 if (!query.trim()) return
 setSearching(true)
 setSearched(true)
 try {
 const client = new RagClient(ragUrl, apiKey, token)
 const raw = await client.stringSearch(query.trim(), slug, 20) as any
 const arr = Array.isArray(raw) ? raw : (raw?.results ?? raw?.matches ?? raw?.data ?? [])
 const mapped: StringSearchResult[] = (Array.isArray(arr) ? arr : []).map((r: any) => ({
 document_id: r.document_id ?? r.id ?? '',
 document_name: r.document_name ?? r.filename ?? r.file_name ?? 'Unknown',
 chunk_text: r.chunk_text ?? r.highlighted_snippet ?? r.preview ?? r.snippet ?? '',
 page_number: r.page_number ?? r.page,
 score: r.score ?? r.similarity,
 }))
 setResults(mapped)
 if (mapped.length === 0) toast.info('No matches found')
 else toast.success(`Found ${mapped.length} match${mapped.length === 1 ? '' : 'es'}`)
 } catch (err) {
 toast.error(String(err))
 setResults([])
 } finally {
 setSearching(false)
 }
 }

 const highlightMatch = (text: string, term: string) => {
 if (!term.trim()) return text
 const regex = new RegExp(`(${term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'gi')
 const parts = text.split(regex)
 return parts.map((part, i) =>
 regex.test(part) ? <mark key={i} className="bg-yellow-200 rounded px-0.5">{part}</mark> : part,
 )
 }

 return (
 <div className="border rounded-lg bg-background overflow-hidden">
 <div className="p-4 border-b">
 <form onSubmit={handleSearch} className="flex gap-2">
 <div className="relative flex-1">
 <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
 <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search for exact text in documents..." className="pl-9" disabled={searching} />
 </div>
 <Button type="submit" disabled={!query.trim() || searching}>
 {searching ? <Loader2 className="h-4 w-4 animate-spin" /> : <><Search className="h-4 w-4 mr-2" />Search</>}
 </Button>
 </form>
 <p className="text-xs text-muted-foreground mt-1">Literal text search across all documents</p>
 </div>

 {searched ? (
 <ScrollArea className="h-[450px]">
 <div className="p-4 space-y-3">
 {results.length === 0 && !searching && (
 <div className="text-center text-muted-foreground py-8">
 <Search className="h-8 w-8 mx-auto mb-2 opacity-50" />
 <p className="text-sm">No results found for &quot;{query}&quot;</p>
 </div>
 )}
 {results.map((r, i) => (
 <Card key={`${r.document_id}-${i}`} className="p-4">
 <div className="flex items-center gap-2 mb-2">
 <FileText className="h-4 w-4 text-muted-foreground shrink-0" />
 <span className="font-medium text-sm truncate">{r.document_name}</span>
 {r.page_number != null && <Badge variant="secondary" className="text-xs">Page {r.page_number}</Badge>}
 {r.score != null && <Badge variant="outline" className="text-xs">{Math.round(r.score * 100)}%</Badge>}
 </div>
 <p className="text-sm text-muted-foreground line-clamp-3">{highlightMatch(r.chunk_text, query)}</p>
 </Card>
 ))}
 </div>
 </ScrollArea>
 ) : (
 <div className="p-8 text-center text-muted-foreground">
 <Search className="h-12 w-12 mx-auto mb-3 opacity-30" />
 <p className="text-sm">Enter a search term to find exact text matches</p>
 </div>
 )}
 </div>
 )
}

/* ── Documents Tab ────────────────────────────────────────────────── */

/* ── Document Viewer Dialog ──────────────────────────────────── */

function DocumentViewerDialog({ doc, slug, ragUrl, apiKey, token, onClose }: {
 doc: RagDocument | null
 slug: string
 ragUrl: string
 apiKey: string
 token: string | null
 onClose: () => void
}) {
 const [blobUrl, setBlobUrl] = useState<string | null>(null)
 const [viewLoading, setViewLoading] = useState(false)
 const [viewError, setViewError] = useState<string | null>(null)

 const ft = doc ? doc.file_type.toLowerCase() : ''
 const isOfficeType = ['doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'odt', 'ods', 'odp', 'rtf'].includes(ft)
 // For Office files, request PDF conversion for inline viewing
 const downloadUrl = doc ? `${ragUrl}/api/documents/${doc.id}/download?organization_id=${encodeURIComponent(slug)}${isOfficeType ? '&format=pdf' : ''}` : ''
 // Raw download URL (no conversion) for the Download button
 const rawDownloadUrl = doc ? `${ragUrl}/api/documents/${doc.id}/download?organization_id=${encodeURIComponent(slug)}` : ''

 const buildHeaders = () => {
 const h: Record<string, string> = {}
 if (token) h['Authorization'] = `Bearer ${token}`
 if (apiKey) h['apikey'] = apiKey
 return h
 }

 // Load document when dialog opens
 useEffect(() => {
 if (!doc) { setBlobUrl(null); return }
 setViewLoading(true)
 setViewError(null)
 setBlobUrl(null)

 const url = downloadUrl
 if (!url) { setViewError('No download URL'); setViewLoading(false); return }

 fetch(url, { headers: buildHeaders() })
 .then(async (resp) => {
 if (!resp.ok) {
 const text = await resp.text().catch(() => '')
 let msg = `Server returned ${resp.status}`
 try { const j = JSON.parse(text); msg = j.error?.message || j.error || msg } catch {}
 throw new Error(msg)
 }
 const blob = await resp.blob()
 if (blob.size === 0) throw new Error('Empty response')
 setBlobUrl(URL.createObjectURL(blob))
 })
 .catch((e) => setViewError(String(e.message || e)))
 .finally(() => setViewLoading(false))

 return () => { if (blobUrl) URL.revokeObjectURL(blobUrl) }
 // eslint-disable-next-line react-hooks/exhaustive-deps
 }, [doc?.id])

 if (!doc) return null

 const docFt = doc.file_type.toLowerCase()
 const isPdf = docFt === 'pdf'
 const isImage = ['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp', 'bmp', 'tiff'].includes(docFt)
 const isText = ['txt', 'md', 'csv', 'html', 'htm'].includes(docFt)
 const isOffice = ['docx', 'doc', 'xlsx', 'xls', 'pptx', 'ppt', 'odt', 'ods', 'odp', 'rtf'].includes(docFt)
 // Office files are converted to PDF server-side, so they can also be previewed inline
 const canInlinePreview = isPdf || isText || isOffice
 const canImagePreview = isImage

 const handleDownload = async () => {
 // Download the original file (not the PDF conversion)
 try {
 const resp = await fetch(rawDownloadUrl, { headers: buildHeaders() })
 if (!resp.ok) { toast.error('Download failed'); return }
 const blob = await resp.blob()
 const url = URL.createObjectURL(blob)
 const a = document.createElement('a')
 a.href = url
 a.download = doc.filename
 a.click()
 URL.revokeObjectURL(url)
 } catch { toast.error('Download failed') }
 }

 const handlePrint = () => {
 if (!blobUrl) return
 const w = window.open(blobUrl, '_blank')
 if (w) w.onload = () => w.print()
 }

 return (
 <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
 <div className="bg-white rounded-xl shadow-2xl w-full max-w-5xl max-h-[90vh] mx-4 flex flex-col" onClick={(e) => e.stopPropagation()}>
 {/* Header */}
 <div className="flex items-center justify-between p-4 border-b">
 <div className="flex items-center gap-3 min-w-0">
 <FileText className="w-5 h-5 text-primary flex-shrink-0" />
 <div className="min-w-0">
 <h2 className="font-semibold text-gray-900 truncate">{doc.filename}</h2>
 <div className="flex items-center gap-2 text-xs text-gray-500">
 <Badge variant="secondary" className={fileTypeBadgeClass(doc.file_type)}>{doc.file_type.toUpperCase()}</Badge>
 {doc.file_size ? <span>{formatFileSize(doc.file_size)}</span> : null}
 {doc.total_chunks ? <span>{doc.total_chunks} chunks</span> : null}
 </div>
 </div>
 </div>
 <div className="flex items-center gap-2 flex-shrink-0">
 <Button variant="outline" size="sm" onClick={handlePrint} disabled={!blobUrl} title="Print">
 <Printer className="w-4 h-4 mr-1" /> Print
 </Button>
 <Button variant="outline" size="sm" onClick={handleDownload} disabled={!blobUrl} title="Download">
 <ExternalLink className="w-4 h-4 mr-1" /> Download
 </Button>
 <Button variant="ghost" size="sm" onClick={onClose}>
 <X className="w-4 h-4" />
 </Button>
 </div>
 </div>

 {/* Content */}
 <div className="flex-1 overflow-hidden min-h-[400px]">
 {viewLoading && (
 <div className="flex items-center justify-center h-96">
 <Loader2 className="w-6 h-6 animate-spin text-primary" />
 <span className="ml-2 text-gray-500">Loading document...</span>
 </div>
 )}
 {viewError && (
 <div className="flex flex-col items-center justify-center h-96 gap-4">
 <FileText className="w-12 h-12 text-gray-300" />
 <p className="text-sm text-gray-500">Unable to load preview</p>
 <p className="text-xs text-gray-400 max-w-md text-center">{viewError}</p>
 <div className="flex gap-2">
 <Button variant="outline" size="sm" onClick={() => {
 const a = document.createElement('a')
 a.href = downloadUrl
 a.download = doc.filename
 a.click()
 }}>
 <ExternalLink className="w-4 h-4 mr-1" /> Try Direct Download
 </Button>
 </div>
 </div>
 )}
 {/* PDF and text files: inline iframe */}
 {blobUrl && canInlinePreview && (
 <iframe src={blobUrl} className="w-full h-[75vh] border-0" title={doc.filename} />
 )}
 {/* Images: img tag */}
 {blobUrl && canImagePreview && (
 <div className="flex items-center justify-center h-[75vh] p-4 bg-gray-50">
 <img src={blobUrl} alt={doc.filename} className="max-w-full max-h-full object-contain rounded shadow" />
 </div>
 )}
 {/* Office and other files: download/print buttons */}
 {blobUrl && !canInlinePreview && !canImagePreview && (
 <div className="flex flex-col items-center justify-center h-96 gap-4">
 <FileText className="w-16 h-16 text-gray-300" />
 <p className="text-gray-600 font-medium">{doc.filename}</p>
 <p className="text-sm text-gray-500">
 {isOffice
 ? `${doc.file_type.toUpperCase()} files can be downloaded and opened in your office application`
 : `Preview not available for ${doc.file_type.toUpperCase()} files`}
 </p>
 <div className="flex gap-2">
 <Button onClick={handleDownload}><ExternalLink className="w-4 h-4 mr-1" /> Download</Button>
 <Button variant="outline" onClick={handlePrint}><Printer className="w-4 h-4 mr-1" /> Print</Button>
 </div>
 </div>
 )}
 </div>
 </div>
 </div>
 )
}

function DocumentsTab({ slug, ragUrl, apiKey, token }: { slug: string; ragUrl: string; apiKey: string; token: string | null }) {
 const [docs, setDocs] = useState<RagDocument[]>([])
 const [loading, setLoading] = useState(true)
 const [error, setError] = useState<string | null>(null)
 const [showArchived, setShowArchived] = useState(false)
 const [uploading, setUploading] = useState(false)
 const [viewingDoc, setViewingDoc] = useState<RagDocument | null>(null)
 const [aclDocId, setAclDocId] = useState<string | null>(null)
 const [aclEntries, setAclEntries] = useState<{ id: string; grant_type: string; grant_id: string; grant_name: string | null; action: string }[]>([])
 const [aclLoading, setAclLoading] = useState(false)
 const [aclGroups, setAclGroups] = useState<{ id: string; name: string }[]>([])
 const [aclUsers, setAclUsers] = useState<{ id: string; first_name: string; last_name: string }[]>([])
 const [aclAddType, setAclAddType] = useState('group')
 const [aclAddId, setAclAddId] = useState('')
 const fileRef = useRef<HTMLInputElement>(null)

 // Load ACLs, groups, and users when dialog opens
 useEffect(() => {
 if (!aclDocId || !token) return
 setAclLoading(true)
 const bpe = new BpeClient(token)
 const { postgrestUrl, apiKey: pgApiKey } = detectApiUrls()
 const headers: Record<string, string> = { Authorization: `Bearer ${token}`, Accept: 'application/json' }
 if (pgApiKey) headers['apikey'] = pgApiKey
 const currentOrg = useOrgStore.getState().currentOrg
 const orgId = currentOrg?.id ?? ''
 Promise.all([
  bpe.listDocumentAcls(aclDocId, slug),
  bpe.listGroups(slug),
  orgId ? fetch(`${postgrestUrl}/users?select=id,first_name,last_name&organization_id=eq.${orgId}&order=first_name.asc&limit=200`, { headers }).then((r) => r.json()) : Promise.resolve([]),
 ]).then(([acls, groups, users]) => {
  setAclEntries(acls.data)
  setAclGroups(groups.data.map((g) => ({ id: g.id, name: g.name })))
  setAclUsers(Array.isArray(users) ? users : [])
 }).catch(() => toast.error('Failed to load access settings'))
 .finally(() => setAclLoading(false))
 }, [aclDocId, token, slug])

 const handleAddAcl = async () => {
 if (!aclDocId || !aclAddId || !token) return
 const bpe = new BpeClient(token)
 try {
  await bpe.createDocumentAcl(aclDocId, { organization_id: slug, grant_type: aclAddType, grant_id: aclAddId })
  const res = await bpe.listDocumentAcls(aclDocId, slug)
  setAclEntries(res.data)
  setAclAddId('')
  toast.success('Access granted')
 } catch { toast.error('Failed to add access') }
 }

 const handleRemoveAcl = async (aclId: string) => {
 if (!aclDocId || !token) return
 const bpe = new BpeClient(token)
 try {
  await bpe.deleteDocumentAcl(aclDocId, aclId, slug)
  setAclEntries((prev) => prev.filter((e) => e.id !== aclId))
  toast.success('Access removed')
 } catch { toast.error('Failed to remove access') }
 }

 const handleClearAcls = async () => {
 if (!aclDocId || !token) return
 const bpe = new BpeClient(token)
 try {
  await bpe.clearDocumentAcls(aclDocId, slug)
  setAclEntries([])
  toast.success('Document is now open to all')
 } catch { toast.error('Failed to clear access') }
 }

 const aclDoc = docs.find((d) => d.id === aclDocId)

 const loadDocs = () => {
 setLoading(true)
 setError(null)
 const client = new RagClient(ragUrl, apiKey, token)
 client.listDocuments(slug)
 .then((resp) => setDocs(resp.documents))
 .catch((e) => setError(String(e)))
 .finally(() => setLoading(false))
 }

 useEffect(() => { if (slug) loadDocs() }, [slug])

 const execAction = async (docId: string, action: 'archive' | 'unarchive' | 'delete') => {
 const client = new RagClient(ragUrl, apiKey, token)
 try {
 if (action === 'archive') await client.archiveDocument(docId, slug)
 else if (action === 'unarchive') await client.unarchiveDocument(docId, slug)
 else await client.deleteDocument(docId, slug)
 toast.success(`Document ${action}d successfully`)
 loadDocs()
 } catch (e) {
 toast.error(`Failed to ${action}: ${e}`)
 }
 }

 const onFileSelected = async (e: ChangeEvent<HTMLInputElement>) => {
 const file = e.target.files?.[0]
 if (!file) return
 setUploading(true)
 try {
 const form = new FormData()
 form.append('organization_id', slug)
 form.append('filename', file.name)
 form.append('file', file)
 const headers: Record<string, string> = {}
 if (token) headers['Authorization'] = `Bearer ${token}`
 if (apiKey) headers['apikey'] = apiKey
 const resp = await fetch(`${ragUrl}/api/files/upload`, { method: 'POST', headers, body: form })
 if (resp.ok) { toast.success(`Uploaded '${file.name}'. Processing in background.`); loadDocs() }
 else { const body = await resp.text(); toast.error(`Upload failed: ${body}`) }
 } catch (err) {
 toast.error(`Upload error: ${err}`)
 } finally {
 setUploading(false)
 if (fileRef.current) fileRef.current.value = ''
 }
 }

 const filtered = useMemo(() => docs.filter((d) => showArchived || !d.archived), [docs, showArchived])
 const activeDocs = docs.filter((d) => !d.archived)
 const totalChunks = activeDocs.reduce((sum, d) => sum + (d.total_chunks ?? 0), 0)

 return (
 <div className="space-y-4">
 <div className="flex items-center justify-between flex-wrap gap-3">
 <div className="flex items-center gap-3">
 <label className="cursor-pointer">
 <Button variant="default" size="sm" disabled={uploading} asChild>
 <span>
 <Upload className="h-4 w-4 mr-2" />
 {uploading ? 'Uploading...' : 'Upload Document'}
 </span>
 </Button>
 <input
 ref={fileRef}
 type="file"
 className="hidden"
 accept=".pdf,.docx,.doc,.xlsx,.xls,.pptx,.ppt,.txt,.md,.csv,.html,.rtf,.epub,.odt"
 onChange={onFileSelected}
 disabled={uploading}
 />
 </label>
 <Button variant="outline" size="sm" onClick={loadDocs}>
 <RefreshCw className="h-4 w-4 mr-1" /> Refresh
 </Button>
 <label className="inline-flex items-center gap-2 text-sm text-muted-foreground cursor-pointer select-none">
 <input type="checkbox" className="rounded border-gray-300" checked={showArchived} onChange={(e) => setShowArchived(e.target.checked)} />
 Show archived
 </label>
 </div>
 <span className="text-sm text-muted-foreground">{activeDocs.length} active docs | {totalChunks} chunks</span>
 </div>

 {error && <div className="p-3 text-sm text-red-700 bg-red-50 rounded-lg border border-red-200">{error}</div>}

 {loading ? (
 <div className="flex items-center justify-center py-12">
 <Loader2 className="h-6 w-6 animate-spin text-primary" />
 <span className="ml-2 text-muted-foreground">Loading documents...</span>
 </div>
 ) : (
 <div className="bg-card border rounded-xl overflow-x-auto">
 <table className="w-full text-left min-w-[640px]">
 <thead>
 <tr className="border-b bg-muted/50">
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Document</th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Type</th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase text-right">Chunks</th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase text-right">Size</th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Ingested</th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Status</th>
 <th className="px-4 py-3 text-xs font-semibold text-muted-foreground uppercase">Actions</th>
 </tr>
 </thead>
 <tbody>
 {filtered.map((d) => (
 <tr key={d.id} className={`hover:bg-muted/50 border-b last:border-0 ${d.archived ? 'opacity-60' : ''}`}>
 <td className="px-4 py-3 text-sm font-medium max-w-xs truncate">{d.filename}</td>
 <td className="px-4 py-3">
 <Badge variant="secondary" className={`text-xs ${fileTypeBadgeClass(d.file_type)}`}>{d.file_type.toUpperCase()}</Badge>
 </td>
 <td className="px-4 py-3 text-sm text-muted-foreground text-right">{d.total_chunks ?? '--'}</td>
 <td className="px-4 py-3 text-sm text-muted-foreground text-right">{d.file_size ? formatFileSize(d.file_size) : '--'}</td>
 <td className="px-4 py-3 text-sm text-muted-foreground">{d.ingested_at?.slice(0, 10) ?? '--'}</td>
 <td className="px-4 py-3">
 {d.archived
 ? <Badge variant="outline" className="bg-amber-100 text-amber-800">Archived</Badge>
 : <Badge variant="outline" className="bg-emerald-100 text-emerald-800">Active</Badge>
 }
 </td>
 <td className="px-4 py-3">
 <div className="flex items-center gap-1">
 <Button variant="ghost" size="sm" className="h-7 px-2 text-xs" onClick={() => setViewingDoc(d)} title="View document">
 <Eye className="h-3 w-3 mr-1" /> View
 </Button>
 <Button variant="ghost" size="sm" className="h-7 px-2 text-xs" onClick={() => execAction(d.id, d.archived ? 'unarchive' : 'archive')}>
 {d.archived ? <ArchiveRestore className="h-3 w-3 mr-1" /> : <Archive className="h-3 w-3 mr-1" />}
 {d.archived ? 'Unarchive' : 'Archive'}
 </Button>
 <Button
 variant="ghost"
 size="sm"
 className="h-7 px-2 text-xs text-destructive hover:text-destructive"
 onClick={() => { if (window.confirm(`Permanently delete '${d.filename}'?`)) execAction(d.id, 'delete') }}
 >
 <Trash2 className="h-3 w-3 mr-1" /> Delete
 </Button>
 <Button variant="ghost" size="sm" className="h-7 px-2 text-xs" onClick={() => setAclDocId(d.id)} title="Manage access">
  <Shield className="h-3 w-3 mr-1" /> Access
 </Button>
 </div>
 </td>
 </tr>
 ))}
 {filtered.length === 0 && (
 <tr><td colSpan={7} className="px-4 py-8 text-center text-sm text-muted-foreground">No documents found.</td></tr>
 )}
 </tbody>
 </table>
 </div>
 )}

 {/* Document Viewer Dialog */}
 <DocumentViewerDialog
 doc={viewingDoc}
 slug={slug}
 ragUrl={ragUrl}
 apiKey={apiKey}
 token={token}
 onClose={() => setViewingDoc(null)}
 />

 {/* Document ACL Dialog */}
 <Dialog open={aclDocId !== null} onOpenChange={(open) => { if (!open) setAclDocId(null) }}>
 <DialogContent className="sm:max-w-md">
  <DialogHeader>
  <DialogTitle className="flex items-center gap-2">
   <Shield className="h-5 w-5" />
   Manage Access — {aclDoc?.filename ?? 'Document'}
  </DialogTitle>
  </DialogHeader>
  {aclLoading ? (
  <div className="flex items-center justify-center py-8"><Loader2 className="h-5 w-5 animate-spin" /></div>
  ) : (
  <div className="space-y-4">
   {/* Current status */}
   <div className="flex items-center gap-2 text-sm">
   {aclEntries.length === 0 ? (
    <><Globe className="h-4 w-4 text-emerald-600" /><span className="text-emerald-700 font-medium">Open to all organization members</span></>
   ) : (
    <><Lock className="h-4 w-4 text-amber-600" /><span className="text-amber-700 font-medium">Restricted — {aclEntries.length} access grant(s)</span></>
   )}
   </div>

   {/* Current grants */}
   {aclEntries.length > 0 && (
   <div className="space-y-1">
    {aclEntries.map((e) => (
    <div key={e.id} className="flex items-center justify-between p-2 rounded-lg bg-muted/30 text-sm">
     <div>
     <Badge variant="outline" className="text-[10px] mr-2">{e.grant_type}</Badge>
     <span className="font-medium">{e.grant_name || e.grant_id.slice(0, 8)}</span>
     <span className="text-muted-foreground ml-2">({e.action})</span>
     </div>
     <Button variant="ghost" size="icon" className="h-6 w-6 text-destructive" onClick={() => handleRemoveAcl(e.id)}>
     <X className="h-3 w-3" />
     </Button>
    </div>
    ))}
    <Button variant="outline" size="sm" className="w-full mt-2 text-xs" onClick={handleClearAcls}>
    <Globe className="h-3 w-3 mr-1" /> Make open to all (remove all restrictions)
    </Button>
   </div>
   )}

   {/* Add grant */}
   <div className="border-t pt-3 space-y-2">
   <p className="text-xs font-semibold uppercase text-muted-foreground">Add access grant</p>
   <div className="flex items-center gap-2">
    <Select value={aclAddType} onValueChange={(v) => { setAclAddType(v); setAclAddId('') }}>
    <SelectTrigger className="w-28"><SelectValue /></SelectTrigger>
    <SelectContent>
     <SelectItem value="group">Group</SelectItem>
     <SelectItem value="user">User</SelectItem>
    </SelectContent>
    </Select>
    <Select value={aclAddId} onValueChange={setAclAddId}>
    <SelectTrigger className="flex-1"><SelectValue placeholder="Select..." /></SelectTrigger>
    <SelectContent>
     {aclAddType === 'group' ? aclGroups.map((g) => (
     <SelectItem key={g.id} value={g.id}>{g.name}</SelectItem>
     )) : aclUsers.map((u) => (
     <SelectItem key={u.id} value={u.id}>{u.first_name} {u.last_name}</SelectItem>
     ))}
    </SelectContent>
    </Select>
    <Button size="sm" onClick={handleAddAcl} disabled={!aclAddId}>Add</Button>
   </div>
   </div>
  </div>
  )}
 </DialogContent>
 </Dialog>
 </div>
 )
}

/* ── Main Page ────────────────────────────────────────────────────── */

export function KnowledgeBasePage() {
 const currentOrg = useOrgStore((s) => s.currentOrg)
 const token = useAuthStore((s) => s.token)
 const { ragUrl, apiKey } = detectApiUrls()
 const slug = currentOrg ? orgNameToSlug(currentOrg.name) : ''
 const [activeTab, setActiveTab] = useState('chat')

 return (
 <div className="space-y-6">
 <div>
 <h1 className="text-3xl font-bold">Ask Marshal</h1>
 <p className="text-muted-foreground mt-1">Query your knowledge base using AI, search for text, or manage documents</p>
 </div>

 <Tabs value={activeTab} onValueChange={setActiveTab}>
 <TabsList>
 <TabsTrigger value="chat" className="gap-2">
 <MessageSquare className="h-4 w-4" />
 AI Chat
 </TabsTrigger>
 <TabsTrigger value="search" className="gap-2">
 <Search className="h-4 w-4" />
 Text Search
 </TabsTrigger>
 <TabsTrigger value="documents" className="gap-2">
 <FileText className="h-4 w-4" />
 Documents
 </TabsTrigger>
 </TabsList>

 <TabsContent value="chat" className="mt-4">
 <AIChatTab slug={slug} ragUrl={ragUrl} apiKey={apiKey} token={token} />
 </TabsContent>

 <TabsContent value="search" className="mt-4">
 <TextSearchTab slug={slug} ragUrl={ragUrl} apiKey={apiKey} token={token} />
 </TabsContent>

 <TabsContent value="documents" className="mt-4">
 <DocumentsTab slug={slug} ragUrl={ragUrl} apiKey={apiKey} token={token} />
 </TabsContent>
 </Tabs>
 </div>
 )
}
