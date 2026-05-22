import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'

function useClient() {
 const { postgrestUrl, apiKey } = detectApiUrls()
 return new PostgRestClient(postgrestUrl, apiKey)
}

export function usePostgrestList<T>(
 table: string,
 buildQuery: (qb: QueryBuilder, orgId: string) => QueryBuilder,
 opts?: { enabled?: boolean },
) {
 const client = useClient()
 const token = useAuthStore((s) => s.token)
 const orgId = useOrgStore((s) => s.currentOrg?.id)

 return useQuery<T[]>({
 queryKey: [table, orgId],
 queryFn: async () => {
 if (!orgId) return []
 const qs = buildQuery(new QueryBuilder(), orgId).build()
 return client.get<T>(table, qs, token)
 },
 enabled: !!orgId && (opts?.enabled ?? true),
 })
}

export function usePostgrestOne<T>(
 table: string,
 id: string | undefined,
 select?: string,
) {
 const client = useClient()
 const token = useAuthStore((s) => s.token)

 return useQuery<T | null>({
 queryKey: [table, id],
 queryFn: async () => {
 if (!id) return null
 const qs = new QueryBuilder().eq('id', id).build()
 if (select) {
 const qsFull = `select=${select}&${qs}`
 return client.getOne<T>(table, qsFull, token)
 }
 return client.getOne<T>(table, qs, token)
 },
 enabled: !!id,
 })
}

export function usePostgrestCreate<T>(table: string) {
 const client = useClient()
 const token = useAuthStore((s) => s.token)
 const queryClient = useQueryClient()

 return useMutation<T, Error, Record<string, unknown>>({
 mutationFn: (body) => client.post<T>(table, body, token),
 onSuccess: () => {
 queryClient.invalidateQueries({ queryKey: [table] })
 },
 })
}

export function usePostgrestUpdate<T>(table: string) {
 const client = useClient()
 const token = useAuthStore((s) => s.token)
 const queryClient = useQueryClient()

 return useMutation<T, Error, { id: string; body: Record<string, unknown> }>({
 mutationFn: ({ id, body }) => {
 const qs = new QueryBuilder().eq('id', id).build()
 return client.patch<T>(table, qs, body, token)
 },
 onSuccess: () => {
 queryClient.invalidateQueries({ queryKey: [table] })
 },
 })
}
