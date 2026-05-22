import { statusToError } from './error'

// ---------------------------------------------------------------------------
// PostgREST Client
// ---------------------------------------------------------------------------

export class PostgRestClient {
 constructor(
 public baseUrl: string,
 public apiKey: string,
 public timeoutMs: number = 15_000,
 ) {}

 private headers(token?: string | null): Record<string, string> {
 const h: Record<string, string> = {}
 if (this.apiKey) h['apikey'] = this.apiKey
 if (token) h['Authorization'] = `Bearer ${token}`
 return h
 }

 private signal(): AbortSignal {
 return AbortSignal.timeout(this.timeoutMs)
 }

 /** GET /{table}?{query} — returns an array of T. */
 async get<T>(table: string, query: string, token?: string | null): Promise<T[]> {
 const url = `${this.baseUrl}/${table}?${query}`
 const resp = await fetch(url, {
 headers: { ...this.headers(token), Accept: 'application/json' },
 signal: this.signal(),
 })
 if (!resp.ok) {
 const body = await resp.text()
 throw statusToError(resp.status, body)
 }
 return resp.json() as Promise<T[]>
 }

 /** GET /{table}?{query} — returns a single object T. */
 async getOne<T>(table: string, query: string, token?: string | null): Promise<T> {
 const url = `${this.baseUrl}/${table}?${query}`
 const resp = await fetch(url, {
 headers: {
 ...this.headers(token),
 Accept: 'application/vnd.pgrst.object+json',
 Prefer: 'return=representation',
 },
 signal: this.signal(),
 })
 if (!resp.ok) {
 const body = await resp.text()
 throw statusToError(resp.status, body)
 }
 return resp.json() as Promise<T>
 }

 /** POST /{table} — insert a row. Returns the created row. */
 async post<T>(table: string, body: unknown, token?: string | null): Promise<T> {
 const url = `${this.baseUrl}/${table}`
 const resp = await fetch(url, {
 method: 'POST',
 headers: {
 ...this.headers(token),
 'Content-Type': 'application/json',
 Accept: 'application/vnd.pgrst.object+json',
 Prefer: 'return=representation',
 },
 body: JSON.stringify(body),
 signal: this.signal(),
 })
 if (!resp.ok) {
 const text = await resp.text()
 throw statusToError(resp.status, text)
 }
 return resp.json() as Promise<T>
 }

 /** PATCH /{table}?{query} — update rows matching the filter. */
 async patch<T>(table: string, query: string, body: unknown, token?: string | null): Promise<T> {
 const url = `${this.baseUrl}/${table}?${query}`
 const resp = await fetch(url, {
 method: 'PATCH',
 headers: {
 ...this.headers(token),
 'Content-Type': 'application/json',
 Accept: 'application/vnd.pgrst.object+json',
 Prefer: 'return=representation',
 },
 body: JSON.stringify(body),
 signal: this.signal(),
 })
 if (!resp.ok) {
 const text = await resp.text()
 throw statusToError(resp.status, text)
 }
 return resp.json() as Promise<T>
 }

 /** DELETE /{table}?{query}. */
 async delete(table: string, query: string, token?: string | null): Promise<void> {
 const url = `${this.baseUrl}/${table}?${query}`
 const resp = await fetch(url, {
 method: 'DELETE',
 headers: this.headers(token),
 signal: this.signal(),
 })
 if (!resp.ok) {
 const body = await resp.text()
 throw statusToError(resp.status, body)
 }
 }

 /** POST /{table} — bulk insert an array of rows. Returns the created rows. */
 async postMany<T>(table: string, body: unknown[], token?: string | null): Promise<T[]> {
 const url = `${this.baseUrl}/${table}`
 const resp = await fetch(url, {
 method: 'POST',
 headers: {
 ...this.headers(token),
 'Content-Type': 'application/json',
 Accept: 'application/json',
 Prefer: 'return=representation',
 },
 body: JSON.stringify(body),
 signal: this.signal(),
 })
 if (!resp.ok) {
 const text = await resp.text()
 throw statusToError(resp.status, text)
 }
 return resp.json() as Promise<T[]>
 }

 /** POST /rpc/{function} — call a stored procedure. */
 async rpc<T>(fn: string, body: unknown, token?: string | null): Promise<T> {
 const url = `${this.baseUrl}/rpc/${fn}`
 const resp = await fetch(url, {
 method: 'POST',
 headers: {
 ...this.headers(token),
 'Content-Type': 'application/json',
 Accept: 'application/json',
 },
 body: JSON.stringify(body),
 signal: this.signal(),
 })
 if (!resp.ok) {
 const text = await resp.text()
 throw statusToError(resp.status, text)
 }
 return resp.json() as Promise<T>
 }
}

// ---------------------------------------------------------------------------
// Query Builder
// ---------------------------------------------------------------------------

export class QueryBuilder {
 private parts: string[] = []
 private selectCols?: string
 private orderClause?: string
 private limitVal?: number
 private offsetVal?: number

 select(cols: string): this {
 this.selectCols = cols
 return this
 }

 eq(col: string, value: string): this {
 this.parts.push(`${col}=eq.${value}`)
 return this
 }

 neq(col: string, value: string): this {
 this.parts.push(`${col}=neq.${value}`)
 return this
 }

 gt(col: string, value: string): this {
 this.parts.push(`${col}=gt.${value}`)
 return this
 }

 gte(col: string, value: string): this {
 this.parts.push(`${col}=gte.${value}`)
 return this
 }

 lt(col: string, value: string): this {
 this.parts.push(`${col}=lt.${value}`)
 return this
 }

 lte(col: string, value: string): this {
 this.parts.push(`${col}=lte.${value}`)
 return this
 }

 ilike(col: string, pattern: string): this {
 this.parts.push(`${col}=ilike.*${encodeURIComponent(pattern)}*`)
 return this
 }

 isNull(col: string): this {
 this.parts.push(`${col}=is.null`)
 return this
 }

 inList(col: string, values: string[]): this {
 this.parts.push(`${col}=in.(${values.join(',')})`)
 return this
 }

 order(col: string, ascending: boolean): this {
 const dir = ascending ? 'asc' : 'desc'
 this.orderClause = `order=${col}.${dir}`
 return this
 }

 limit(n: number): this {
 this.limitVal = n
 return this
 }

 offset(n: number): this {
 this.offsetVal = n
 return this
 }

 /** Produce the final query string (no leading ?). */
 build(): string {
 const result: string[] = []
 if (this.selectCols) result.push(`select=${this.selectCols}`)
 result.push(...this.parts)
 if (this.orderClause) result.push(this.orderClause)
 if (this.limitVal !== undefined) result.push(`limit=${this.limitVal}`)
 if (this.offsetVal !== undefined) result.push(`offset=${this.offsetVal}`)
 return result.join('&')
 }
}
