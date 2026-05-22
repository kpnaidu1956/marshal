import { useEffect, useCallback, useMemo } from 'react'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Button } from '@/components/ui/button'
import {
 Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select'
import { Plus } from 'lucide-react'
import { useWorkflowBuilder } from './useWorkflowBuilder'
import { WorkflowStepCard } from './WorkflowStepCard'
import { LevelConnector, SimpleConnector } from './WorkflowConnector'
import { computeLevels, computeEdges } from './workflowLayout'
import { BpeClient } from '@/api/bpe'
import { useAuthStore } from '@/stores/auth'
import { useOrgStore } from '@/stores/org'
import { useState } from 'react'
import type { WorkflowDefinition, StepTemplate } from '@/models/bpe'
import type { BuilderErrors } from './useWorkflowBuilder'

const CATEGORIES = [
 'general', 'onboarding', 'offboarding', 'procurement',
 'compliance', 'hr', 'finance', 'it', 'custom',
] as const

type Category = typeof CATEGORIES[number]

interface WorkflowBuilderProps {
 mode: 'create' | 'edit'
 definition?: WorkflowDefinition | null
 onSubmit: (data: {
 name: string
 description: string | null
 category: string
 step_templates: StepTemplate[]
 }) => void
 loading?: boolean
}

export function WorkflowBuilder({ mode, definition, onSubmit, loading }: WorkflowBuilderProps) {
 const token = useAuthStore((s) => s.token)
 const orgSlug = useOrgStore((s) => s.currentOrgSlug)

 const [name, setName] = useState(definition?.name || '')
 const [description, setDescription] = useState(definition?.description || '')
 const [category, setCategory] = useState<Category>((definition?.category || 'general') as Category)
 const [roleSuggestions, setRoleSuggestions] = useState<string[]>([])
 const [errors, setErrors] = useState<BuilderErrors>({})

 const builder = useWorkflowBuilder()

 // Compute graph layout
 const levels = useMemo(() => computeLevels(builder.steps), [builder.steps])

 // Load definition on edit
 useEffect(() => {
 if (mode === 'edit' && definition) {
 setName(definition.name)
 setDescription(definition.description || '')
 setCategory((definition.category || 'general') as Category)
 builder.fromDefinition(definition)
 }
 // eslint-disable-next-line react-hooks/exhaustive-deps
 }, [mode, definition])

 const client = useMemo(() => token ? new BpeClient(token) : null, [token])

 // Fetch entity types for role suggestions
 useEffect(() => {
 if (!client || !orgSlug) return
 client.listEntityTypes(orgSlug).then((res) => {
 setRoleSuggestions(res.data.map((et) => et.display_name))
 }).catch(() => {})
 }, [client, orgSlug])

 const handleSubmit = useCallback(() => {
 const validationErrors = builder.validate(name)
 if (Object.keys(validationErrors).length > 0) {
 setErrors(validationErrors)
 return
 }
 setErrors({})
 onSubmit({
 name: name.trim(),
 description: description.trim() || null,
 category,
 step_templates: builder.toStepTemplates(),
 })
 }, [name, description, category, builder, onSubmit])

 // Memoize edge computations
 const allEdges = useMemo(() => {
 const edgeMap = new Map<number, ReturnType<typeof computeEdges>>()
 for (let li = 1; li < levels.length; li++) {
 edgeMap.set(li, computeEdges(levels[li - 1], levels[li], builder.steps))
 }
 return edgeMap
 }, [levels, builder.steps])

 // Check if layout is purely linear (each level has 1 step)
 const isLinear = useMemo(() => levels.every((l) => l.stepIndices.length === 1), [levels])

 return (
 <div className="space-y-4">
 {/* Header fields */}
 <div className="space-y-3">
 <div className="space-y-1.5">
 <Label htmlFor="wb-name">Workflow Name *</Label>
 <Input
 id="wb-name"
 placeholder="e.g. Invoice Approval Process"
 value={name}
 onChange={(e) => { setName(e.target.value); setErrors((prev) => ({ ...prev, name: undefined })) }}
 className={errors.name ? 'border-red-300' : ''}
 />
 {errors.name && <p className="text-xs text-red-500">{errors.name}</p>}
 </div>
 <div className="grid grid-cols-2 gap-3">
 <div className="space-y-1.5">
 <Label htmlFor="wb-desc">Description</Label>
 <Textarea
 id="wb-desc"
 placeholder="What does this workflow do?"
 rows={2}
 value={description}
 onChange={(e) => setDescription(e.target.value)}
 />
 </div>
 <div className="space-y-1.5">
 <Label htmlFor="wb-cat">Category</Label>
 <Select value={category} onValueChange={(val) => setCategory(val as Category)}>
 <SelectTrigger id="wb-cat">
 <SelectValue />
 </SelectTrigger>
 <SelectContent>
 {CATEGORIES.map((cat) => (
 <SelectItem key={cat} value={cat}>
 {cat.charAt(0).toUpperCase() + cat.slice(1)}
 </SelectItem>
 ))}
 </SelectContent>
 </Select>
 </div>
 </div>
 </div>

 {/* Divider */}
 <div className="border-t border-gray-200 pt-3">
 <div className="flex items-center justify-between">
 <Label className="text-sm font-medium">Steps</Label>
 <span className="text-[10px] text-gray-400">
 {builder.steps.length} step{builder.steps.length !== 1 ? 's' : ''}
 {!isLinear && ` across ${levels.length} level${levels.length !== 1 ? 's' : ''}`}
 </span>
 </div>
 {errors.steps && <p className="text-xs text-red-500 mt-1">{errors.steps}</p>}
 </div>

 {/* Graph-based step rendering */}
 <div className="flex flex-col items-stretch">
 {levels.map((level, levelIdx) => {
 const isParallel = level.stepIndices.length > 1

 return (
 <div key={levelIdx}>
 {/* Connector from previous level */}
 {levelIdx > 0 && (
 (() => {
 const parentLevel = levels[levelIdx - 1]
 const edges = allEdges.get(levelIdx) || []
 if (edges.length === 0 && parentLevel.stepIndices.length === 1 && level.stepIndices.length === 1) {
 // Simple 1:1 fallback
 return <SimpleConnector isApproval={builder.steps[parentLevel.stepIndices[0]]?.step_type === 'approval'} />
 }
 return (
 <LevelConnector
 edges={edges}
 parentCount={parentLevel.stepIndices.length}
 childCount={level.stepIndices.length}
 />
 )
 })()
 )}

 {/* Level row */}
 {isParallel ? (
 <div className="relative">
 {/* Parallel indicator */}
 <div className="absolute -left-1 top-0 bottom-0 w-0.5 bg-blue-300 rounded-full" />
 <div className="flex gap-2 pl-2">
 {level.stepIndices.map((stepIdx) => (
 <div key={stepIdx} className="flex-1 min-w-0">
 <WorkflowStepCard
 step={builder.steps[stepIdx]}
 index={stepIdx}
 totalSteps={builder.steps.length}
 allSteps={builder.steps}
 roleSuggestions={roleSuggestions}
 error={errors.stepErrors?.[stepIdx]}
 compact
 workflowCategory={category}
 onUpdate={builder.updateStep}
 onRemove={builder.removeStep}
 onMoveUp={builder.moveStepUp}
 onMoveDown={builder.moveStepDown}
 onToggle={builder.toggleExpanded}
 onAddParallel={builder.addParallelStep}
 />
 </div>
 ))}
 </div>
 </div>
 ) : (
 <WorkflowStepCard
 step={builder.steps[level.stepIndices[0]]}
 index={level.stepIndices[0]}
 totalSteps={builder.steps.length}
 allSteps={builder.steps}
 roleSuggestions={roleSuggestions}
 error={errors.stepErrors?.[level.stepIndices[0]]}
 workflowCategory={category}
 onUpdate={builder.updateStep}
 onRemove={builder.removeStep}
 onMoveUp={builder.moveStepUp}
 onMoveDown={builder.moveStepDown}
 onToggle={builder.toggleExpanded}
 onAddParallel={builder.addParallelStep}
 />
 )}
 </div>
 )
 })}
 </div>

 {/* Add step button */}
 <div className="flex flex-col items-center">
 <div className="w-0.5 h-3 bg-gray-300" />
 <Button
 variant="outline"
 size="sm"
 onClick={builder.addStep}
 className="mt-1"
 >
 <Plus className="w-4 h-4 mr-1" />Add Step
 </Button>
 </div>

 {/* Submit */}
 <div className="flex justify-end pt-2 border-t border-gray-200">
 <Button onClick={handleSubmit} disabled={loading}>
 {loading ? 'Saving...' : mode === 'edit' ? 'Save Changes' : 'Create Workflow'}
 </Button>
 </div>
 </div>
 )
}
