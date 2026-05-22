const MAX_RECENT = 10

function storageKey(orgSlug: string) {
 return `marshal_recent_queries_${orgSlug}`
}

export function loadRecent(orgSlug: string): string[] {
 try {
 const raw = localStorage.getItem(storageKey(orgSlug))
 if (!raw) return []
 return JSON.parse(raw) as string[]
 } catch {
 return []
 }
}

export function saveRecent(orgSlug: string, queries: string[]) {
 localStorage.setItem(storageKey(orgSlug), JSON.stringify(queries.slice(0, MAX_RECENT)))
}
