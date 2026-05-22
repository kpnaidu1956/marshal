import { useState, useMemo, useEffect } from 'react'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Network, Users, Link2, Star, RefreshCw, GitBranch, LayoutGrid, List } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Badge } from '@/components/ui/badge'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Slider } from '@/components/ui/slider'
import { analyticsApi } from '@/lib/analyticsApi'
import { PostgRestClient, QueryBuilder } from '@/api/postgrest'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { detectApiUrls } from '@/lib/config'
import type { NetworkGraph, ParticipationMetrics, ParticipationEdge, Team, CrossTeamConnector } from '@/types/analytics'

interface PgUser { id: string; first_name: string; last_name: string; title: string | null; manager_id: string | null }
interface PgComment { author_id: string; task_id: string }

/** Build network graph + teams + connectors from PostgREST data. */
function buildFromPostgREST(
 users: PgUser[],
 comments: PgComment[],
): { graph: NetworkGraph; teams: Team[]; connectors: CrossTeamConnector[] } {
 const userMap = new Map(users.map((u) => [u.id, u]))

 // Build teams from manager_id
 const teamsByManager = new Map<string, string[]>()
 for (const u of users) {
 const mgr = u.manager_id
 if (mgr && userMap.has(mgr)) {
 const list = teamsByManager.get(mgr) || []
 list.push(u.id)
 teamsByManager.set(mgr, list)
 }
 }
 const userName = (id: string) => {
 const u = userMap.get(id)
 return u ? `${u.first_name} ${u.last_name}`.trim() : 'Unknown'
 }
 const teams: Team[] = Array.from(teamsByManager.entries()).map(([mgr, members]) => ({
 manager_id: mgr,
 manager_name: userName(mgr),
 member_ids: members,
 }))

 // Reverse lookup: userId -> manager (team lead)
 const userTeamMap = new Map<string, string>()
 for (const t of teams) {
 userTeamMap.set(t.manager_id, t.manager_id)
 for (const m of t.member_ids) userTeamMap.set(m, t.manager_id)
 }

 // Group comments by task, then count co-commenting pairs
 const taskAuthors = new Map<string, Set<string>>()
 for (const c of comments) {
 if (!userMap.has(c.author_id)) continue
 const set = taskAuthors.get(c.task_id) || new Set()
 set.add(c.author_id)
 taskAuthors.set(c.task_id, set)
 }
 const pk = (a: string, b: string) => (a < b ? `${a}|${b}` : `${b}|${a}`)
 const pairCounts = new Map<string, number>()
 for (const authors of taskAuthors.values()) {
 const arr = Array.from(authors)
 for (let i = 0; i < arr.length; i++)
 for (let j = i + 1; j < arr.length; j++)
 pairCounts.set(pk(arr[i], arr[j]), (pairCounts.get(pk(arr[i], arr[j])) || 0) + 1)
 }
 const now = new Date().toISOString()
 const edges: ParticipationEdge[] = Array.from(pairCounts.entries()).map(([k, count]) => {
 const [src, tgt] = k.split('|')
 return { source_user_id: src, target_user_id: tgt, interaction_count: count, avg_sentiment: 0, last_interaction_at: now }
 })

 // Build nodes from edge participants
 const conns = new Map<string, number>(), totals = new Map<string, number>()
 for (const e of edges) {
 for (const uid of [e.source_user_id, e.target_user_id]) {
 conns.set(uid, (conns.get(uid) || 0) + 1)
 totals.set(uid, (totals.get(uid) || 0) + e.interaction_count)
 }
 }
 const n = Math.max(conns.size - 1, 1)
 const nodes: ParticipationMetrics[] = Array.from(conns.keys()).map((uid) => ({
 user_id: uid, user_name: userName(uid), degree_centrality: (conns.get(uid) || 0) / n,
 betweenness_centrality: 0, closeness_centrality: 0, total_interactions: totals.get(uid) || 0,
 }))

 // Cross-team connectors
 const connectors: CrossTeamConnector[] = []
 for (const uid of conns.keys()) {
 const myTeam = userTeamMap.get(uid)
 const tc = new Set<string>()
 for (const e of edges) {
 const other = e.source_user_id === uid ? e.target_user_id : e.target_user_id === uid ? e.source_user_id : null
 if (!other) continue
 const ot = userTeamMap.get(other)
 if (ot && ot !== myTeam) tc.add(userName(ot))
 }
 if (tc.size > 0) connectors.push({ user_id: uid, user_name: userName(uid), teams_connected: Array.from(tc), bridge_score: tc.size / Math.max(teams.length - 1, 1) })
 }
 connectors.sort((a, b) => b.bridge_score - a.bridge_score)

 return {
 graph: { nodes, edges, computed_at: new Date().toISOString() },
 teams,
 connectors: connectors.slice(0, 10),
 }
}

type ViewMode = 'teams' | 'table' | 'graph'

export function AnalyticsNetworkPage() {
 const [days, setDays] = useState(90)
 const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null)
 const [viewMode, setViewMode] = useState<ViewMode>('teams')

 const token = useAuthStore((s) => s.token)
 const orgId = useOrgStore((s) => s.currentOrg?.id ?? '')
 const { postgrestUrl, apiKey } = detectApiUrls()

 // All data fetched directly
 const [resolvedGraph, setResolvedGraph] = useState<NetworkGraph | undefined>()
 const [resolvedTeams, setResolvedTeams] = useState<Team[] | undefined>()
 const [resolvedConnectors, setResolvedConnectors] = useState<CrossTeamConnector[] | undefined>()
 const [isLoading, setIsLoading] = useState(false)
 const [loaded, setLoaded] = useState(false)

 useEffect(() => {
 if (!orgId || !token || loaded) return
 let cancelled = false
 setIsLoading(true)

 const client = new PostgRestClient(postgrestUrl, apiKey)

 // Fetch analytics API + PostgREST in parallel
 Promise.all([
 analyticsApi.getNetworkGraph(orgId, days).catch(() => null),
 analyticsApi.getCrossTeamConnectors(orgId).catch(() => null),
 analyticsApi.getTeams(orgId).catch(() => [] as Team[]),
 client.get<PgUser>('users',
 new QueryBuilder().select('id,first_name,last_name,title,manager_id').eq('organization_id', orgId).limit(200).build(),
 token,
 ).catch(() => [] as PgUser[]),
 client.get<PgComment>('task_comments',
 new QueryBuilder().select('author_id,task_id').eq('organization_id', orgId).limit(10000).build(),
 token,
 ).catch(() => [] as PgComment[]),
 ]).then(([apiGraph, apiConnectors, apiTeams, pgUsers, pgComments]) => {
 if (cancelled) return

 // Build PostgREST network (always — it has real edges from co-commenting)
 const pgResult = pgUsers.length ? buildFromPostgREST(pgUsers, pgComments) : null

 // API graph has nodes but no edges — prefer PostgREST which computes real edges
 // Only use API graph if it has both nodes AND edges
 const useApiGraph = apiGraph?.nodes?.length && apiGraph?.edges?.length
 setResolvedGraph(useApiGraph ? apiGraph : pgResult?.graph)
 setResolvedTeams(apiTeams?.length ? apiTeams : pgResult?.teams)
 setResolvedConnectors(apiConnectors?.length ? apiConnectors : pgResult?.connectors)
 setLoaded(true)
 }).finally(() => { if (!cancelled) setIsLoading(false) })

 return () => { cancelled = true }
 }, [orgId, token, loaded, days, postgrestUrl, apiKey])

 // Build team membership map
 const teamMap = useMemo(() => {
 const map = new Map<string, string>()
 if (!resolvedTeams) return map
 for (const team of resolvedTeams) {
 map.set(team.manager_id, team.manager_name)
 for (const mid of team.member_ids) map.set(mid, team.manager_name)
 }
 return map
 }, [resolvedTeams])

 // Team colors
 const teamColors = useMemo(() => {
 const colors = ['hsl(221,83%,53%)', 'hsl(142,71%,45%)', 'hsl(25,95%,53%)', 'hsl(263,70%,50%)', 'hsl(0,84%,60%)', 'hsl(199,89%,48%)']
 const map = new Map<string, string>()
 if (!resolvedTeams) return map
 resolvedTeams.forEach((t, i) => map.set(t.manager_name, colors[i % colors.length]))
 return map
 }, [resolvedTeams])

 // Team summary data
 const teamSummaries = useMemo(() => {
 if (!resolvedGraph?.nodes || !resolvedTeams) return []
 return resolvedTeams.map((team) => {
 const memberIds = new Set([team.manager_id, ...team.member_ids])
 const teamNodes = resolvedGraph.nodes.filter((n) => memberIds.has(n.user_id))
 const teamEdges = resolvedGraph.edges?.filter((e) => memberIds.has(e.source_user_id) && memberIds.has(e.target_user_id)) ?? []
 const totalInteractions = teamNodes.reduce((s, n) => s + n.total_interactions, 0)
 const avgSentiment = teamEdges.length > 0
 ? teamEdges.reduce((s, e) => s + e.avg_sentiment, 0) / teamEdges.length
 : 0
 return {
 team,
 memberCount: teamNodes.length,
 totalInteractions,
 avgSentiment,
 connections: teamEdges.length,
 color: teamColors.get(team.manager_name) || 'hsl(220,9%,46%)',
 }
 })
 }, [resolvedGraph, resolvedTeams, teamColors])

 // Top interaction pairs
 const topPairs = useMemo(() => {
 if (!resolvedGraph?.edges || !resolvedGraph.nodes) return []
 const nodeMap = new Map(resolvedGraph.nodes.map((n) => [n.user_id, n.user_name || 'Unknown']))
 return [...resolvedGraph.edges]
 .sort((a, b) => b.interaction_count - a.interaction_count)
 .slice(0, 25)
 .map((e) => ({
 source: nodeMap.get(e.source_user_id) || 'Unknown',
 target: nodeMap.get(e.target_user_id) || 'Unknown',
 count: e.interaction_count,
 sentiment: e.avg_sentiment,
 sourceTeam: teamMap.get(e.source_user_id) || 'Unknown',
 targetTeam: teamMap.get(e.target_user_id) || 'Unknown',
 }))
 }, [resolvedGraph, teamMap])

 // Get edge color based on sentiment
 const getEdgeColor = (s: number) => s > 0.2 ? 'hsl(142,71%,45%)' : s < -0.2 ? 'hsl(0,84%,60%)' : 'hsl(220,9%,46%)'

 // SVG graph layout
 const graphLayout = useMemo(() => {
 if (!resolvedGraph?.nodes?.length) return { positions: {} as Record<string, { x: number; y: number }>, width: 1000, height: 600 }
 const nodes = resolvedGraph.nodes
 const width = 1000
 const height = 600
 const positions: Record<string, { x: number; y: number }> = {}
 const cx = width / 2
 const cy = height / 2
 const radius = Math.min(width, height) * 0.35
 nodes.forEach((node, i) => {
 const angle = (2 * Math.PI * i) / nodes.length - Math.PI / 2
 positions[node.user_id] = { x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle) }
 })
 return { positions, width, height }
 }, [resolvedGraph])

 const selectedNode = resolvedGraph?.nodes?.find((n) => n.user_id === selectedNodeId)
 const connectedEdges = resolvedGraph?.edges?.filter(
 (e) => e.source_user_id === selectedNodeId || e.target_user_id === selectedNodeId,
 ) || []

 const handleRefresh = () => {
 setLoaded(false)
 }

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <div>
 <h1 className="text-2xl font-bold">Team Interactions</h1>
 <p className="text-muted-foreground text-sm">
 Visualize communication patterns and sentiment across team members.
 </p>
 </div>
 <div className="flex items-center gap-2">
 <Tabs value={viewMode} onValueChange={(v) => setViewMode(v as ViewMode)}>
 <TabsList>
 <TabsTrigger value="teams" className="gap-1.5"><LayoutGrid className="h-4 w-4" />Teams</TabsTrigger>
 <TabsTrigger value="table" className="gap-1.5"><List className="h-4 w-4" />Top Pairs</TabsTrigger>
 <TabsTrigger value="graph" className="gap-1.5"><GitBranch className="h-4 w-4" />Graph</TabsTrigger>
 </TabsList>
 </Tabs>
 <Button variant="outline" size="icon" onClick={handleRefresh}>
 <RefreshCw className="h-4 w-4" />
 </Button>
 </div>
 </div>

 {/* Stats */}
 <div className="grid gap-4 md:grid-cols-4">
 {([
 { label: 'Teams', icon: <Users className="h-4 w-4 text-muted-foreground" />, value: resolvedTeams?.length ?? 0, sub: 'Active teams' },
 { label: 'Participants', icon: <Users className="h-4 w-4 text-muted-foreground" />, value: resolvedGraph?.nodes?.length || 0, sub: `In last ${days} days` },
 { label: 'Connections', icon: <Link2 className="h-4 w-4 text-muted-foreground" />, value: resolvedGraph?.edges?.length || 0, sub: 'Communication links' },
 { label: 'Bridge Connectors', icon: <Network className="h-4 w-4 text-muted-foreground" />, value: resolvedConnectors?.length || 0, sub: 'Cross-team bridges' },
 ] as const).map((s) => (
 <Card key={s.label}>
 <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
 <CardTitle className="text-sm font-medium">{s.label}</CardTitle>{s.icon}
 </CardHeader>
 <CardContent>
 {isLoading ? <Skeleton className="h-8 w-16" /> : <><div className="text-2xl font-bold">{s.value}</div><p className="text-xs text-muted-foreground">{s.sub}</p></>}
 </CardContent>
 </Card>
 ))}
 </div>

 {/* Time Range */}
 <Card>
 <CardContent className="py-4">
 <div className="flex items-center gap-4">
 <span className="text-sm font-medium">Time Range: {days} day{days !== 1 ? 's' : ''}</span>
 <Slider value={[days]} onValueChange={([v]) => { setDays(v); setLoaded(false) }} min={7} max={180} step={7} className="w-40" />
 </div>
 </CardContent>
 </Card>

 {/* Team Summary View */}
 {viewMode === 'teams' && (
 <Card>
 <CardHeader className="pb-2">
 <CardTitle>Team Overview</CardTitle>
 <CardDescription>Aggregate sentiment and activity metrics by team.</CardDescription>
 </CardHeader>
 <CardContent>
 {isLoading ? (
 <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
 {[1, 2, 3].map((i) => <Skeleton key={i} className="h-[200px] rounded-lg" />)}
 </div>
 ) : teamSummaries.length > 0 ? (
 <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
 {teamSummaries.map((ts) => (
 <div key={ts.team.manager_id} className="p-4 rounded-lg border bg-card space-y-3">
 <div className="flex items-center gap-2">
 <div className="w-3 h-3 rounded-full" style={{ backgroundColor: ts.color }} />
 <h4 className="font-medium">{ts.team.manager_name}&apos;s Team</h4>
 </div>
 <div className="grid grid-cols-2 gap-2 text-sm">
 <div><span className="text-muted-foreground">Members</span><p className="font-semibold">{ts.memberCount}</p></div>
 <div><span className="text-muted-foreground">Interactions</span><p className="font-semibold">{ts.totalInteractions}</p></div>
 <div><span className="text-muted-foreground">Connections</span><p className="font-semibold">{ts.connections}</p></div>
 <div>
 <span className="text-muted-foreground">Sentiment</span>
 <p className="font-semibold" style={{ color: getEdgeColor(ts.avgSentiment) }}>
 {(ts.avgSentiment * 100).toFixed(0)}%
 </p>
 </div>
 </div>
 </div>
 ))}
 </div>
 ) : <div className="h-[200px] flex items-center justify-center text-muted-foreground">No team data available</div>}
 </CardContent>
 </Card>
 )}

 {/* Top Pairs Table */}
 {viewMode === 'table' && (
 <Card>
 <CardHeader className="pb-2">
 <CardTitle>Top Interaction Pairs</CardTitle>
 <CardDescription>Ranked list of relationships by frequency.</CardDescription>
 </CardHeader>
 <CardContent>
 {isLoading ? <Skeleton className="h-[400px]" /> : topPairs.length > 0 ? (
 <div className="overflow-x-auto">
 <table className="w-full text-sm">
 <thead>
 <tr className="border-b">
 <th className="text-left px-3 py-2 font-medium text-muted-foreground">#</th>
 <th className="text-left px-3 py-2 font-medium text-muted-foreground">Person A</th>
 <th className="text-left px-3 py-2 font-medium text-muted-foreground">Person B</th>
 <th className="text-right px-3 py-2 font-medium text-muted-foreground">Interactions</th>
 <th className="text-right px-3 py-2 font-medium text-muted-foreground">Sentiment</th>
 </tr>
 </thead>
 <tbody>
 {topPairs.map((p, i) => (
 <tr key={i} className="border-b last:border-0">
 <td className="px-3 py-2 text-muted-foreground">{i + 1}</td>
 <td className="px-3 py-2"><span className="font-medium">{p.source}</span> <span className="text-xs text-muted-foreground">({p.sourceTeam})</span></td>
 <td className="px-3 py-2"><span className="font-medium">{p.target}</span> <span className="text-xs text-muted-foreground">({p.targetTeam})</span></td>
 <td className="px-3 py-2 text-right"><Badge variant="secondary">{p.count}</Badge></td>
 <td className="px-3 py-2 text-right" style={{ color: getEdgeColor(p.sentiment) }}>{(p.sentiment * 100).toFixed(0)}%</td>
 </tr>
 ))}
 </tbody>
 </table>
 </div>
 ) : <div className="h-[400px] flex items-center justify-center text-muted-foreground">No interaction data available</div>}
 </CardContent>
 </Card>
 )}

 {/* Network Graph View */}
 {viewMode === 'graph' && (
 <>
 <Card>
 <CardHeader className="pb-2">
 <CardTitle>Network Graph</CardTitle>
 <CardDescription>Click a node to view details. Node size = activity level.</CardDescription>
 </CardHeader>
 <CardContent className="h-[600px]">
 {isLoading ? <Skeleton className="h-full" /> : resolvedGraph?.nodes?.length ? (
 <svg width="100%" height="100%" viewBox={`0 0 ${graphLayout.width} ${graphLayout.height}`} preserveAspectRatio="xMidYMid meet" className="bg-muted/20 rounded-lg">
 {/* Edges */}
 {(resolvedGraph.edges ?? []).map((edge, i) => {
 const s = graphLayout.positions[edge.source_user_id]
 const t = graphLayout.positions[edge.target_user_id]
 if (!s || !t) return null
 const isConnected = selectedNodeId && (edge.source_user_id === selectedNodeId || edge.target_user_id === selectedNodeId)
 const maxCount = Math.max(...resolvedGraph.edges.map((e) => e.interaction_count))
 return (
 <line key={i} x1={s.x} y1={s.y} x2={t.x} y2={t.y}
 stroke={getEdgeColor(edge.avg_sentiment)}
 strokeWidth={1 + (edge.interaction_count / maxCount) * 3}
 opacity={selectedNodeId ? (isConnected ? 0.8 : 0.1) : 0.4} />
 )
 })}
 {/* Nodes */}
 {resolvedGraph.nodes.map((node) => {
 const pos = graphLayout.positions[node.user_id]
 if (!pos) return null
 const isConnected = !selectedNodeId || selectedNodeId === node.user_id || connectedEdges.some((e) => e.source_user_id === node.user_id || e.target_user_id === node.user_id)
 const maxI = Math.max(...resolvedGraph.nodes.map((n) => n.total_interactions))
 const r = 8 + (node.total_interactions / Math.max(maxI, 1)) * 12
 const team = teamMap.get(node.user_id) || ''
 const color = teamColors.get(team) || 'hsl(220,9%,46%)'
 return (
 <g key={node.user_id} onClick={() => setSelectedNodeId(selectedNodeId === node.user_id ? null : node.user_id)} className="cursor-pointer">
 <circle cx={pos.x} cy={pos.y} r={r} fill={color} opacity={isConnected ? 1 : 0.3}
 stroke={selectedNodeId === node.user_id ? '#fff' : 'none'} strokeWidth={2} />
 <text x={pos.x} y={pos.y + r + 14} textAnchor="middle" fontSize={10}
 fill="currentColor" opacity={isConnected ? 1 : 0.3}>
 {(node.user_name || 'Unknown').slice(0, 15)}
 </text>
 </g>
 )
 })}
 </svg>
 ) : <div className="h-full flex items-center justify-center text-muted-foreground">No network data available</div>}
 </CardContent>
 </Card>

 {/* Details */}
 <div className="grid gap-6 md:grid-cols-2">
 <Card>
 <CardHeader>
 <CardTitle className="text-sm">{selectedNode ? (selectedNode.user_name || 'Unknown') : 'Node Details'}</CardTitle>
 <CardDescription>{selectedNode ? `Team: ${teamMap.get(selectedNode.user_id) || 'Unknown'}` : 'Click a node to view details'}</CardDescription>
 </CardHeader>
 <CardContent>
 {selectedNode ? (
 <div className="space-y-3">
 <div className="flex justify-between"><span className="text-sm text-muted-foreground">Total Interactions</span><span className="font-medium">{selectedNode.total_interactions}</span></div>
 <div className="flex justify-between"><span className="text-sm text-muted-foreground">Degree Centrality</span><span className="font-medium">{(selectedNode.degree_centrality * 100).toFixed(1)}%</span></div>
 <div className="flex justify-between"><span className="text-sm text-muted-foreground">Betweenness</span><span className="font-medium">{(selectedNode.betweenness_centrality * 100).toFixed(1)}%</span></div>
 <div className="flex justify-between"><span className="text-sm text-muted-foreground">Direct Connections</span><span className="font-medium">{connectedEdges.length}</span></div>
 <Button variant="ghost" size="sm" className="w-full mt-2" onClick={() => setSelectedNodeId(null)}>Clear Selection</Button>
 </div>
 ) : <p className="text-sm text-muted-foreground text-center py-4">Select a node from the graph</p>}
 </CardContent>
 </Card>

 <Card>
 <CardHeader>
 <CardTitle className="text-sm flex items-center gap-2"><Star className="h-4 w-4 text-amber-500" />Bridge Connectors</CardTitle>
 <CardDescription>Key people connecting different teams</CardDescription>
 </CardHeader>
 <CardContent>
 {isLoading ? <Skeleton className="h-20" /> : resolvedConnectors?.length ? (
 <div className="space-y-3">
 {resolvedConnectors.map((c) => (
 <div key={c.user_id} className="flex items-center justify-between p-2 rounded-lg bg-muted/50 hover:bg-muted cursor-pointer"
 onClick={() => setSelectedNodeId(c.user_id)}>
 <div>
 <p className="font-medium text-sm">{c.user_name || 'Unknown'}</p>
 <p className="text-xs text-muted-foreground">{c.teams_connected.length} teams connected</p>
 </div>
 <Badge variant="secondary">{(c.bridge_score * 100).toFixed(0)}%</Badge>
 </div>
 ))}
 </div>
 ) : <p className="text-sm text-muted-foreground text-center py-4">No bridge connectors identified</p>}
 </CardContent>
 </Card>
 </div>
 </>
 )}
 </div>
 )
}
