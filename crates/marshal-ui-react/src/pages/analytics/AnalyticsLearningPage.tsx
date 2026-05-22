import { useState, useEffect, useMemo } from 'react'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Brain, CheckCircle, TrendingUp, Target, RefreshCw, Zap } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { Skeleton } from '@/components/ui/skeleton'
import { Badge } from '@/components/ui/badge'
import { useApplyLearningAdjustments } from '@/hooks/useAnalytics'
import { analyticsApi, formatInteractionType } from '@/lib/analyticsApi'
import { useOrgStore } from '@/stores/org'
import { useAuthStore } from '@/stores/auth'
import type { LearningEffectiveness, EfficiencyRecommendation } from '@/types/analytics'
import {
 BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
 RadialBarChart, RadialBar, Legend,
} from 'recharts'

export function AnalyticsLearningPage() {
 const orgId = useOrgStore((s) => s.currentOrg?.id ?? '')
 const token = useAuthStore((s) => s.token)
 const applyAdjustments = useApplyLearningAdjustments()

 const [effectiveness, setEffectiveness] = useState<LearningEffectiveness[] | null>(null)
 const [recommendations, setRecommendations] = useState<EfficiencyRecommendation[] | null>(null)
 const [effectivenessLoading, setEffectivenessLoading] = useState(false)
 const [recsLoading, setRecsLoading] = useState(false)
 const [loaded, setLoaded] = useState(false)

 useEffect(() => {
 if (!orgId || !token || loaded) return
 setEffectivenessLoading(true)
 setRecsLoading(true)
 Promise.all([
 analyticsApi.getLearningEffectiveness(orgId).catch(() => [] as LearningEffectiveness[]),
 analyticsApi.getOrganizationRecommendations(orgId).catch(() => [] as EfficiencyRecommendation[]),
 ]).then(([eff, recs]) => {
 setEffectiveness(eff)
 setRecommendations(recs)
 setLoaded(true)
 }).finally(() => { setEffectivenessLoading(false); setRecsLoading(false) })
 }, [orgId, token, loaded])

 const refetch = () => setLoaded(false)

 const effectivenessData = useMemo(() => {
 if (!effectiveness?.length) return []
 return effectiveness.map((e: LearningEffectiveness) => ({
 type: formatInteractionType(e.intervention_type),
 shortType: e.intervention_type.split('_').map((w) => w[0].toUpperCase()).join(''),
 total: e.total_interventions,
 successful: e.successful_interventions,
 successRate: Math.round(e.success_rate * 100),
 improvement: Math.round(e.avg_improvement * 100),
 fill: e.success_rate > 0.6 ? 'hsl(142, 71%, 45%)' : e.success_rate > 0.4 ? 'hsl(48, 96%, 53%)' : 'hsl(0, 84%, 60%)',
 }))
 }, [effectiveness])

 const radialData = useMemo(() => {
 const colors = ['hsl(221, 83%, 53%)', 'hsl(142, 71%, 45%)', 'hsl(25, 95%, 53%)', 'hsl(263, 70%, 50%)']
 return effectivenessData.map((e, i) => ({ name: e.type, value: e.successRate, fill: colors[i % 4] }))
 }, [effectivenessData])

 const overallStats = useMemo(() => {
 if (!effectiveness?.length) return { total: 0, successful: 0, rate: 0, avgImprovement: 0 }
 const total = effectiveness.reduce((s: number, e: LearningEffectiveness) => s + e.total_interventions, 0)
 const successful = effectiveness.reduce((s: number, e: LearningEffectiveness) => s + e.successful_interventions, 0)
 const avgImprovement = effectiveness.reduce((s: number, e: LearningEffectiveness) => s + e.avg_improvement, 0) / effectiveness.length
 return { total, successful, rate: total > 0 ? Math.round((successful / total) * 100) : 0, avgImprovement: Math.round(avgImprovement * 100) }
 }, [effectiveness])

 const pendingRecs = Array.isArray(recommendations)
 ? recommendations.filter((r: EfficiencyRecommendation) => r.status === 'pending').length
 : 0

 const handleApplyAdjustments = () => {
 applyAdjustments.mutate(undefined, { onSuccess: () => setLoaded(false) })
 }

 return (
 <div className="space-y-6">
 <div className="flex items-center justify-between">
 <div>
 <h1 className="text-2xl font-bold">Learning & Effectiveness</h1>
 <p className="text-muted-foreground text-sm">
 Track intervention outcomes and recommendation effectiveness over time.
 </p>
 </div>
 <div className="flex items-center gap-2">
 <Button variant="outline" onClick={handleApplyAdjustments} disabled={applyAdjustments.isPending}>
 <Zap className={`h-4 w-4 mr-2 ${applyAdjustments.isPending ? 'animate-pulse' : ''}`} />
 Apply Learning
 </Button>
 <Button variant="outline" size="icon" onClick={() => setLoaded(false)}>
 <RefreshCw className="h-4 w-4" />
 </Button>
 </div>
 </div>

 {/* Stats Cards */}
 <div className="grid gap-6 md:grid-cols-4">
 <Card>
 <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
 <CardTitle className="text-sm font-medium">Total Interventions</CardTitle>
 <Target className="h-4 w-4 text-muted-foreground" />
 </CardHeader>
 <CardContent>
 {effectivenessLoading ? <Skeleton className="h-8 w-16" /> : (
 <><div className="text-2xl font-bold">{overallStats.total}</div><p className="text-xs text-muted-foreground">Actions taken</p></>
 )}
 </CardContent>
 </Card>
 <Card>
 <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
 <CardTitle className="text-sm font-medium">Success Rate</CardTitle>
 <CheckCircle className="h-4 w-4 text-muted-foreground" />
 </CardHeader>
 <CardContent>
 {effectivenessLoading ? <Skeleton className="h-8 w-20" /> : (
 <><div className="flex items-center gap-2">
 <span className="text-2xl font-bold">{overallStats.rate}%</span>
 <Badge variant={overallStats.rate > 60 ? 'default' : overallStats.rate > 40 ? 'secondary' : 'destructive'}>
 {overallStats.rate > 60 ? 'Good' : overallStats.rate > 40 ? 'Fair' : 'Needs Work'}
 </Badge>
 </div><p className="text-xs text-muted-foreground">Positive outcomes</p></>
 )}
 </CardContent>
 </Card>
 <Card>
 <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
 <CardTitle className="text-sm font-medium">Avg. Improvement</CardTitle>
 <TrendingUp className="h-4 w-4 text-muted-foreground" />
 </CardHeader>
 <CardContent>
 {effectivenessLoading ? <Skeleton className="h-8 w-16" /> : (
 <><div className="text-2xl font-bold">+{overallStats.avgImprovement}%</div><p className="text-xs text-muted-foreground">Metric improvement</p></>
 )}
 </CardContent>
 </Card>
 <Card>
 <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
 <CardTitle className="text-sm font-medium">Pending Actions</CardTitle>
 <Brain className="h-4 w-4 text-muted-foreground" />
 </CardHeader>
 <CardContent>
 {recsLoading ? <Skeleton className="h-8 w-16" /> : (
 <><div className="text-2xl font-bold">{pendingRecs}</div><p className="text-xs text-muted-foreground">Recommendations to review</p></>
 )}
 </CardContent>
 </Card>
 </div>

 <div className="grid gap-6 md:grid-cols-2">
 {/* Intervention Success */}
 <Card>
 <CardHeader>
 <CardTitle>Intervention Success by Type</CardTitle>
 <CardDescription>Comparison of successful vs total interventions</CardDescription>
 </CardHeader>
 <CardContent className="h-[350px]">
 {effectivenessLoading ? <Skeleton className="h-[280px] w-full" /> :
 effectivenessData.length > 0 ? (
 <ResponsiveContainer width="100%" height="100%">
 <BarChart data={effectivenessData} layout="vertical">
 <CartesianGrid strokeDasharray="3 3" />
 <XAxis type="number" tick={{ fontSize: 12 }} />
 <YAxis type="category" dataKey="type" width={180} tick={{ fontSize: 12 }} />
 <Tooltip formatter={(v, name) => [v, name === 'total' ? 'Total' : 'Successful']} />
 <Bar dataKey="total" fill="hsl(220, 14%, 80%)" name="total" radius={[0, 4, 4, 0]} />
 <Bar dataKey="successful" fill="hsl(142, 71%, 45%)" name="successful" radius={[0, 4, 4, 0]} />
 <Legend />
 </BarChart>
 </ResponsiveContainer>
 ) : <div className="h-full flex items-center justify-center text-muted-foreground">No intervention data available</div>}
 </CardContent>
 </Card>

 {/* Success Rate Radial */}
 <Card>
 <CardHeader>
 <CardTitle>Success Rates Overview</CardTitle>
 <CardDescription>Success rate by intervention type</CardDescription>
 </CardHeader>
 <CardContent className="h-[350px]">
 {effectivenessLoading ? <Skeleton className="h-[280px] w-[280px] rounded-full mx-auto" /> :
 radialData.length > 0 ? (
 <ResponsiveContainer width="100%" height="100%">
 <RadialBarChart cx="50%" cy="50%" innerRadius="20%" outerRadius="90%" data={radialData} startAngle={180} endAngle={0}>
 <RadialBar dataKey="value" cornerRadius={4} label={{ position: 'insideStart', fill: '#fff', fontSize: 12 }} />
 <Legend iconType="circle" layout="horizontal" verticalAlign="bottom" wrapperStyle={{ fontSize: '11px' }} />
 <Tooltip formatter={(v) => [`${v}%`, 'Success Rate']} />
 </RadialBarChart>
 </ResponsiveContainer>
 ) : <div className="h-full flex items-center justify-center text-muted-foreground">No data available</div>}
 </CardContent>
 </Card>
 </div>

 {/* Detailed Breakdown */}
 <Card>
 <CardHeader>
 <CardTitle>Intervention Type Breakdown</CardTitle>
 <CardDescription>Detailed metrics for each type of intervention</CardDescription>
 </CardHeader>
 <CardContent>
 {effectivenessLoading ? (
 <div className="space-y-4">{[1, 2, 3, 4].map((i) => <Skeleton key={i} className="h-20 w-full" />)}</div>
 ) : effectivenessData.length > 0 ? (
 <div className="space-y-4">
 {effectivenessData.map((item) => (
 <div key={item.type} className="p-4 rounded-lg bg-muted/30 space-y-3">
 <div className="flex items-center justify-between">
 <div className="flex items-center gap-3">
 <div className="w-3 h-3 rounded-full" style={{ backgroundColor: item.fill }} />
 <span className="font-medium">{item.type}</span>
 </div>
 <Badge variant={item.successRate > 60 ? 'default' : 'secondary'}>{item.successRate}% success</Badge>
 </div>
 <Progress value={item.successRate} className="h-2" />
 <div className="flex gap-6 text-sm text-muted-foreground">
 <span>Total: <strong className="text-foreground">{item.total}</strong></span>
 <span>Successful: <strong className="text-foreground">{item.successful}</strong></span>
 <span>Avg Improvement: <strong className="text-foreground">+{item.improvement}%</strong></span>
 </div>
 </div>
 ))}
 </div>
 ) : (
 <div className="py-8 text-center text-muted-foreground">
 No intervention data available. Interventions are recorded when recommendations are acted upon.
 </div>
 )}
 </CardContent>
 </Card>

 {/* Pending Recommendations */}
 {pendingRecs > 0 && (
 <Card>
 <CardHeader>
 <CardTitle className="flex items-center gap-2">
 <Brain className="h-5 w-5 text-primary" />
 Pending Recommendations ({pendingRecs})
 </CardTitle>
 <CardDescription>Review and act on these to improve learning data</CardDescription>
 </CardHeader>
 <CardContent>
 <div className="grid gap-4 md:grid-cols-2">
 {(Array.isArray(recommendations) ? recommendations : [])
 .filter((r: EfficiencyRecommendation) => r.status === 'pending')
 .slice(0, 4)
 .map((rec: EfficiencyRecommendation) => (
 <div key={rec.id} className="p-4 rounded-lg border bg-card">
 <div className="flex items-start justify-between gap-2 mb-2">
 <h4 className="font-medium text-sm">{rec.title}</h4>
 <Badge variant="outline" className="shrink-0">{rec.priority}</Badge>
 </div>
 <p className="text-xs text-muted-foreground mb-3">{rec.description}</p>
 {rec.estimated_time_savings_hours && (
 <p className="text-xs text-primary">Potential savings: {rec.estimated_time_savings_hours}h</p>
 )}
 </div>
 ))}
 </div>
 </CardContent>
 </Card>
 )}
 </div>
 )
}
