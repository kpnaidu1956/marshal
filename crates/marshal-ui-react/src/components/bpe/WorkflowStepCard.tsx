import { memo } from 'react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Checkbox } from '@/components/ui/checkbox'
import { Switch } from '@/components/ui/switch'
import {
 Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select'
import {
 Popover, PopoverContent, PopoverTrigger,
} from '@/components/ui/popover'
import {
 Hand, ShieldCheck, Zap, Plug, Brain, GitBranch, Bot,
 ChevronDown, ChevronUp, ArrowUp, ArrowDown, Trash2,
 Copy, OctagonX,
} from 'lucide-react'
import type { BuilderStep } from './useWorkflowBuilder'

const STEP_TYPES = [
 { value: 'manual', label: 'Manual', icon: Hand },
 { value: 'approval', label: 'Approval', icon: ShieldCheck },
 { value: 'automated', label: 'Automated', icon: Zap },
 { value: 'integration', label: 'Integration', icon: Plug },
 { value: 'llm_action', label: 'LLM Action', icon: Brain },
 { value: 'sub_workflow', label: 'Sub-Workflow', icon: GitBranch },
 { value: 'ruflo_agent', label: 'AI Agent', icon: Bot },
] as const

/** Infer which Ruflo agent type the backend will auto-select, for display purposes.
 * Mirrors the logic in bpe-core/src/integration/ruflo.rs `infer_agent_type()`. */
function inferAgentHint(name: string, description: string, prompt: string, category: string, deps: number[], allSteps: BuilderStep[]): string {
 const combined = `${name} ${description} ${prompt}`.toLowerCase()
 const hasApprovalParent = deps.some((d) => allSteps[d]?.step_type === 'approval')

 // Reviewer: follows approval steps, or mentions review/validate/audit/compliance/check/verify/inspect/quality
 if (hasApprovalParent || /review|validate|audit|compliance|check|verify|inspect|quality/.test(combined)) return 'Reviewer'
 // Coder: mentions code/implement/build/develop/fix/bug/deploy/script/program/refactor
 if (/code|implement|build|develop|fix|bug|deploy|script|program|refactor/.test(combined)) return 'Coder'
 // Tester: mentions test/qa/regression/coverage/acceptance
 if (/\btest|qa\b|regression|coverage|acceptance/.test(combined)) return 'Tester'
 // Planner: mentions plan/design/architect/organize/schedule/decompos/breakdown
 if (/plan|design|architect|organize|schedule|decompos|breakdown/.test(combined)) return 'Planner'
 // Analyzer: mentions analyze/data/metrics/report/statistics/trend/dashboard/insight
 if (/analyz|data|metric|report|statistic|trend|dashboard|insight/.test(combined)) return 'Analyzer'
 // Researcher: mentions research/gather/find/search/investigate/summarize/discover/learn/explore
 if (/research|gather|find|search|investigat|summariz|discover|learn|explore/.test(combined)) return 'Researcher'
 // Category-based fallback (case-insensitive)
 const cat = (category || '').toLowerCase()
 if (cat === 'compliance') return 'Reviewer'
 if (cat === 'it') return 'Coder'
 if (cat === 'finance') return 'Analyzer'
 return 'Researcher'
}

const CONDITION_OPTIONS = [
 { value: 'none', label: 'Always (no condition)' },
 { value: 'approved', label: 'If approved' },
 { value: 'rejected', label: 'If rejected' },
 { value: 'custom', label: 'Custom condition' },
]

interface WorkflowStepCardProps {
 step: BuilderStep
 index: number
 totalSteps: number
 allSteps: BuilderStep[]
 roleSuggestions: string[]
 error?: string
 compact?: boolean
 workflowCategory?: string
 onUpdate: (index: number, partial: Partial<BuilderStep>) => void
 onRemove: (index: number) => void
 onMoveUp: (index: number) => void
 onMoveDown: (index: number) => void
 onToggle: (index: number) => void
 onAddParallel?: (index: number) => void
}

function WorkflowStepCardInner({
 step, index, totalSteps, allSteps, roleSuggestions, error, compact, workflowCategory,
 onUpdate, onRemove, onMoveUp, onMoveDown, onToggle, onAddParallel,
}: WorkflowStepCardProps) {
 const typeInfo = STEP_TYPES.find((t) => t.value === step.step_type) || STEP_TYPES[0]
 const TypeIcon = typeInfo.icon

 // Check if any dependency is an approval step
 const hasApprovalParent = step.dependencies.some(
 (d) => allSteps[d]?.step_type === 'approval',
 )

 // Collapsed view
 if (!step.expanded) {
 return (
 <div
 className={`border rounded-lg p-2.5 bg-white transition-colors ${
 step.is_terminal
 ? 'border-red-300 bg-red-50/30'
 : error ? 'border-red-300' : 'border-gray-200'
 } ${compact ? 'min-w-[180px]' : ''}`}
 >
 <div className="flex items-center gap-2">
 <span className="text-xs font-mono text-gray-400 w-4 text-right flex-shrink-0">{index + 1}</span>
 <TypeIcon className="w-3.5 h-3.5 text-gray-500 flex-shrink-0" />
 <span className="font-medium text-xs text-gray-900 truncate flex-1">
 {step.name || <span className="text-gray-400 italic">Unnamed</span>}
 </span>
 {step.step_type === 'approval' && (
 <Badge variant="secondary" className="text-[10px] px-1 py-0 bg-amber-100 text-amber-700">
 Decision
 </Badge>
 )}
 {step.is_terminal && (
 <Badge variant="secondary" className="text-[10px] px-1 py-0 bg-red-100 text-red-700">
 End
 </Badge>
 )}
 {step.condition && (
 <Badge variant="outline" className={`text-[10px] px-1 py-0 ${
 step.condition === 'approved' ? 'text-green-600 border-green-300' :
 step.condition === 'rejected' ? 'text-red-600 border-red-300' :
 'text-gray-500'
 }`}>
 {step.condition}
 </Badge>
 )}
 {step.assigned_role && !compact && (
 <Badge variant="outline" className="text-[10px] px-1 py-0">{step.assigned_role}</Badge>
 )}
 </div>
 <div className="flex items-center gap-0.5 mt-1.5 justify-end">
 {onAddParallel && (
 <Button variant="ghost" size="sm" className="h-6 px-1.5 text-[10px] text-blue-600 hover:text-blue-700" onClick={() => onAddParallel(index)} title="Add parallel step">
 <Copy className="w-3 h-3 mr-0.5" />Parallel
 </Button>
 )}
 <Button variant="ghost" size="sm" className="h-6 w-6 p-0" onClick={() => onMoveUp(index)} disabled={index === 0}>
 <ArrowUp className="w-3 h-3" />
 </Button>
 <Button variant="ghost" size="sm" className="h-6 w-6 p-0" onClick={() => onMoveDown(index)} disabled={index === totalSteps - 1}>
 <ArrowDown className="w-3 h-3" />
 </Button>
 <Button variant="ghost" size="sm" className="h-6 w-6 p-0 text-red-500 hover:text-red-700" onClick={() => onRemove(index)} disabled={totalSteps <= 1}>
 <Trash2 className="w-3 h-3" />
 </Button>
 <Button variant="ghost" size="sm" className="h-6 w-6 p-0" onClick={() => onToggle(index)}>
 <ChevronDown className="w-3.5 h-3.5" />
 </Button>
 </div>
 {error && <p className="text-[10px] text-red-500 mt-1">{error}</p>}
 </div>
 )
 }

 // Expanded view
 return (
 <div className={`border rounded-lg bg-white transition-colors ${
 step.is_terminal
 ? 'border-red-300'
 : error ? 'border-red-300' : 'border-gray-200'
 } ${compact ? 'min-w-[260px]' : ''}`}>
 {/* Header */}
 <div className="flex items-center gap-2 p-2.5 border-b border-gray-100">
 <span className="text-xs font-mono text-gray-400 w-4 text-right flex-shrink-0">{index + 1}</span>
 <TypeIcon className="w-3.5 h-3.5 text-gray-500" />
 <span className="font-medium text-xs text-gray-900 flex-1 truncate">
 {step.name || 'New Step'}
 </span>
 {step.step_type === 'approval' && (
 <Badge variant="secondary" className="text-[10px] px-1 py-0 bg-amber-100 text-amber-700">
 Decision
 </Badge>
 )}
 {step.is_terminal && (
 <Badge variant="secondary" className="text-[10px] px-1 py-0 bg-red-100 text-red-700">
 End
 </Badge>
 )}
 <div className="flex items-center gap-0.5">
 {onAddParallel && (
 <Button variant="ghost" size="sm" className="h-6 px-1.5 text-[10px] text-blue-600 hover:text-blue-700" onClick={() => onAddParallel(index)} title="Add parallel step">
 <Copy className="w-3 h-3 mr-0.5" />Parallel
 </Button>
 )}
 <Button variant="ghost" size="sm" className="h-6 w-6 p-0" onClick={() => onMoveUp(index)} disabled={index === 0}>
 <ArrowUp className="w-3 h-3" />
 </Button>
 <Button variant="ghost" size="sm" className="h-6 w-6 p-0" onClick={() => onMoveDown(index)} disabled={index === totalSteps - 1}>
 <ArrowDown className="w-3 h-3" />
 </Button>
 <Button variant="ghost" size="sm" className="h-6 w-6 p-0 text-red-500 hover:text-red-700" onClick={() => onRemove(index)} disabled={totalSteps <= 1}>
 <Trash2 className="w-3 h-3" />
 </Button>
 <Button variant="ghost" size="sm" className="h-6 w-6 p-0" onClick={() => onToggle(index)}>
 <ChevronUp className="w-3.5 h-3.5" />
 </Button>
 </div>
 </div>

 {/* Form */}
 <div className="p-2.5 space-y-2.5">
 <div className="grid grid-cols-2 gap-2.5">
 <div className="space-y-1">
 <Label className="text-[11px]">Step Name *</Label>
 <Input
 placeholder="e.g. Submit Invoice"
 value={step.name}
 onChange={(e) => onUpdate(index, { name: e.target.value })}
 className={`h-8 text-sm ${error ? 'border-red-300' : ''}`}
 />
 </div>
 <div className="space-y-1">
 <Label className="text-[11px]">Step Type</Label>
 <Select value={step.step_type} onValueChange={(val) => onUpdate(index, { step_type: val })}>
 <SelectTrigger className="h-8 text-sm">
 <SelectValue />
 </SelectTrigger>
 <SelectContent>
 {STEP_TYPES.map((t) => (
 <SelectItem key={t.value} value={t.value}>
 <span className="flex items-center gap-2">
 <t.icon className="w-3.5 h-3.5" />
 {t.label}
 </span>
 </SelectItem>
 ))}
 </SelectContent>
 </Select>
 </div>
 </div>

 <div className="grid grid-cols-2 gap-2.5">
 <div className="space-y-1">
 <Label className="text-[11px]">Assigned Role</Label>
 <Popover>
 <PopoverTrigger asChild>
 <Input
 placeholder="e.g. Order Processor"
 value={step.assigned_role}
 onChange={(e) => onUpdate(index, { assigned_role: e.target.value })}
 className="h-8 text-sm"
 />
 </PopoverTrigger>
 {(() => {
 const filteredSuggestions = roleSuggestions.filter(
 (r) => r.toLowerCase().includes(step.assigned_role.toLowerCase()) && r !== step.assigned_role,
 )
 return filteredSuggestions.length > 0 && step.assigned_role.length > 0 ? (
 <PopoverContent className="w-[200px] p-1" align="start" side="bottom">
 {filteredSuggestions.slice(0, 6).map((r) => (
 <button
 key={r}
 className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-gray-100"
 onClick={() => onUpdate(index, { assigned_role: r })}
 >
 {r}
 </button>
 ))}
 </PopoverContent>
 ) : null})()}
 </Popover>
 </div>
 <div className="space-y-1">
 <Label className="text-[11px]">Est. Duration (min)</Label>
 <Input
 type="number"
 min={0}
 placeholder="Optional"
 value={step.estimated_duration_minutes ?? ''}
 onChange={(e) =>
 onUpdate(index, {
 estimated_duration_minutes: e.target.value ? Number(e.target.value) : null,
 })
 }
 className="h-8 text-sm"
 />
 </div>
 </div>

 <div className="space-y-1">
 <Label className="text-[11px]">Description</Label>
 <Textarea
 rows={2}
 placeholder="What happens in this step?"
 value={step.description}
 onChange={(e) => onUpdate(index, { description: e.target.value })}
 className="text-sm"
 />
 </div>

 {/* Dependencies */}
 {index > 0 && (
 <div className="space-y-1">
 <Label className="text-[11px]">Dependencies (runs after)</Label>
 <div className="flex flex-wrap gap-x-3 gap-y-1">
 {allSteps.slice(0, index).map((prev, di) => (
 <label key={di} className="flex items-center gap-1.5 text-xs cursor-pointer">
 <Checkbox
 checked={step.dependencies.includes(di)}
 onCheckedChange={(checked) => {
 const deps = checked
 ? [...step.dependencies, di]
 : step.dependencies.filter((d) => d !== di)
 onUpdate(index, { dependencies: deps })
 }}
 />
 <span className="text-gray-700">
 {prev.name || `Step ${di + 1}`}
 </span>
 </label>
 ))}
 </div>
 </div>
 )}

 {/* Execution rule — only when multiple dependencies */}
 {step.dependencies.length > 1 && (
 <div className="space-y-1">
 <Label className="text-[11px]">Join Rule</Label>
 <Select value={step.execution_rule} onValueChange={(val: 'all' | 'any') => onUpdate(index, { execution_rule: val })}>
 <SelectTrigger className="h-8 text-sm">
 <SelectValue />
 </SelectTrigger>
 <SelectContent>
 <SelectItem value="all">Wait for ALL dependencies</SelectItem>
 <SelectItem value="any">Wait for ANY dependency</SelectItem>
 </SelectContent>
 </Select>
 </div>
 )}

 {/* Condition — when depending on an approval step */}
 {hasApprovalParent && (
 <div className="space-y-1">
 <Label className="text-[11px]">Branch Condition</Label>
 <Select
 value={
 step.condition === 'approved' || step.condition === 'rejected'
 ? step.condition
 : !step.condition ? 'none' : 'custom'
 }
 onValueChange={(val) => {
 onUpdate(index, { condition: val === 'none' ? '' : val === 'custom' ? step.condition || 'custom' : val })
 }}
 >
 <SelectTrigger className="h-8 text-sm">
 <SelectValue placeholder="Always (no condition)" />
 </SelectTrigger>
 <SelectContent>
 {CONDITION_OPTIONS.map((opt) => (
 <SelectItem key={opt.value} value={opt.value}>{opt.label}</SelectItem>
 ))}
 </SelectContent>
 </Select>
 {step.condition && step.condition !== 'approved' && step.condition !== 'rejected' && (
 <Input
 placeholder="Custom condition expression"
 value={step.condition}
 onChange={(e) => onUpdate(index, { condition: e.target.value })}
 className="h-8 text-sm mt-1"
 />
 )}
 </div>
 )}

 {/* Terminal step toggle */}
 <div className="flex items-center justify-between pt-1 border-t border-gray-100">
 <div className="flex items-center gap-2">
 <OctagonX className="w-3.5 h-3.5 text-red-500" />
 <Label className="text-[11px] cursor-pointer" htmlFor={`terminal-${index}`}>
 Terminal step (workflow ends here)
 </Label>
 </div>
 <Switch
 id={`terminal-${index}`}
 checked={step.is_terminal}
 onCheckedChange={(checked) => onUpdate(index, { is_terminal: !!checked })}
 />
 </div>

 {/* Integration-specific fields */}
 {step.step_type === 'integration' && (
 <div className="grid grid-cols-2 gap-2.5">
 <div className="space-y-1">
 <Label className="text-[11px]">Integration Type</Label>
 <Select value={step.integration_type} onValueChange={(val) => onUpdate(index, { integration_type: val })}>
 <SelectTrigger className="h-8 text-sm">
 <SelectValue placeholder="Select type" />
 </SelectTrigger>
 <SelectContent>
 {['email', 'slack', 'webhook', 'api', 'database'].map((t) => (
 <SelectItem key={t} value={t}>{t.charAt(0).toUpperCase() + t.slice(1)}</SelectItem>
 ))}
 </SelectContent>
 </Select>
 </div>
 <div className="space-y-1">
 <Label className="text-[11px]">Config (JSON)</Label>
 <Textarea
 rows={2}
 className="font-mono text-xs"
 placeholder="{}"
 value={step.integration_config}
 onChange={(e) => onUpdate(index, { integration_config: e.target.value })}
 />
 </div>
 </div>
 )}

 {/* LLM Action config */}
 {step.step_type === 'llm_action' && (
 <div className="space-y-1">
 <Label className="text-[11px]">Prompt / Config (JSON)</Label>
 <Textarea
 rows={3}
 className="font-mono text-xs"
 placeholder='{"prompt": "..."}'
 value={step.integration_config}
 onChange={(e) => onUpdate(index, { integration_config: e.target.value })}
 />
 </div>
 )}

 {/* Ruflo AI Agent config */}
 {step.step_type === 'ruflo_agent' && (() => {
 let parsedConfig: { prompt?: string; tools?: string[] } = {}
 try { parsedConfig = JSON.parse(step.integration_config || '{}') } catch { /* use defaults */ }
 const agentPrompt = parsedConfig.prompt || ''
 const agentTools = (parsedConfig.tools || []).join(', ')
 const hint = inferAgentHint(step.name, step.description, agentPrompt, workflowCategory || '', step.dependencies, allSteps)

 const updateConfig = (patch: Record<string, unknown>) => {
 try {
 const c = JSON.parse(step.integration_config || '{}')
 onUpdate(index, { integration_config: JSON.stringify({ ...c, ...patch }), integration_type: 'ruflo_agent' })
 } catch {
 onUpdate(index, { integration_config: JSON.stringify(patch), integration_type: 'ruflo_agent' })
 }
 }

 return (
 <div className="space-y-2.5 p-2 bg-violet-50/50 rounded-md border border-violet-200">
 <div className="flex items-center justify-between mb-1">
 <div className="flex items-center gap-1.5">
 <Bot className="w-3.5 h-3.5 text-violet-600" />
 <span className="text-[11px] font-medium text-violet-700">AI Agent</span>
 </div>
 <Badge variant="outline" className="text-[10px] px-1.5 py-0 border-violet-300 text-violet-600">
 Auto: {hint}
 </Badge>
 </div>
 <div className="space-y-1">
 <Label className="text-[11px]">What should the AI do? *</Label>
 <Textarea
 rows={3}
 placeholder="Describe the task — the system will automatically pick the right AI agent type..."
 value={agentPrompt}
 onChange={(e) => updateConfig({ prompt: e.target.value })}
 className="text-sm"
 />
 <p className="text-[10px] text-gray-400">
 Agent type is auto-selected from step name, description, and workflow context.
 </p>
 </div>
 <div className="space-y-1">
 <Label className="text-[11px]">Tools (optional)</Label>
 <Input
 placeholder="e.g. search, file_read — leave empty for auto"
 value={agentTools}
 onChange={(e) => {
 const tools = e.target.value.split(',').map((s: string) => s.trim()).filter(Boolean)
 updateConfig({ tools })
 }}
 className="h-8 text-sm"
 />
 </div>
 </div>
 )
 })()}
 </div>
 </div>
 )
}

export const WorkflowStepCard = memo(WorkflowStepCardInner)
