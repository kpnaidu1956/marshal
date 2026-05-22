import { useState, useCallback } from 'react'
import type { StepTemplate, WorkflowDefinition } from '@/models/bpe'
import { hasCycle } from './workflowLayout'

export interface BuilderStep {
 name: string
 description: string
 step_type: string
 assigned_role: string
 dependencies: number[]
 estimated_duration_minutes: number | null
 integration_type: string
 integration_config: string
 execution_rule: 'all' | 'any'
 condition: string
 is_terminal: boolean
 expanded: boolean
}

const EMPTY_STEP: BuilderStep = {
 name: '',
 description: '',
 step_type: 'manual',
 assigned_role: '',
 dependencies: [],
 estimated_duration_minutes: null,
 integration_type: '',
 integration_config: '',
 execution_rule: 'all',
 condition: '',
 is_terminal: false,
 expanded: true,
}

export interface BuilderErrors {
 name?: string
 steps?: string
 stepErrors?: Record<number, string>
}

export function useWorkflowBuilder() {
 const [steps, setSteps] = useState<BuilderStep[]>([{ ...EMPTY_STEP }])

 const addStep = useCallback(() => {
 setSteps((prev) => {
 const newStep: BuilderStep = {
 ...EMPTY_STEP,
 dependencies: prev.length > 0 ? [prev.length - 1] : [],
 }
 return [...prev.map((s) => ({ ...s, expanded: false })), newStep]
 })
 }, [])

 /** Add a parallel sibling — same dependencies as the given step. */
 const addParallelStep = useCallback((siblingIndex: number) => {
 setSteps((prev) => {
 const sibling = prev[siblingIndex]
 if (!sibling) return prev
 const newStep: BuilderStep = {
 ...EMPTY_STEP,
 dependencies: [...sibling.dependencies],
 }
 // Insert right after the sibling
 const insertAt = siblingIndex + 1
 const next = [...prev]
 next.splice(insertAt, 0, newStep)
 // Fix dependency indices for steps after the insertion point
 return next.map((s, i) => ({
 ...s,
 expanded: i === insertAt ? true : false,
 dependencies: s.dependencies.map((d) =>
 i !== insertAt && d >= insertAt ? d + 1 : d,
 ),
 }))
 })
 }, [])

 const removeStep = useCallback((index: number) => {
 setSteps((prev) => {
 const next = prev.filter((_, i) => i !== index)
 return next.map((s) => ({
 ...s,
 dependencies: s.dependencies
 .filter((d) => d !== index)
 .map((d) => (d > index ? d - 1 : d)),
 }))
 })
 }, [])

 const updateStep = useCallback((index: number, partial: Partial<BuilderStep>) => {
 setSteps((prev) => prev.map((s, i) => (i === index ? { ...s, ...partial } : s)))
 }, [])

 const moveStepUp = useCallback((index: number) => {
 if (index <= 0) return
 setSteps((prev) => {
 const next = [...prev]
 ;[next[index - 1], next[index]] = [next[index], next[index - 1]]
 return next.map((s) => ({
 ...s,
 dependencies: s.dependencies.map((d) => {
 if (d === index) return index - 1
 if (d === index - 1) return index
 return d
 }),
 }))
 })
 }, [])

 const moveStepDown = useCallback((index: number) => {
 setSteps((prev) => {
 if (index >= prev.length - 1) return prev
 const next = [...prev]
 ;[next[index], next[index + 1]] = [next[index + 1], next[index]]
 return next.map((s) => ({
 ...s,
 dependencies: s.dependencies.map((d) => {
 if (d === index) return index + 1
 if (d === index + 1) return index
 return d
 }),
 }))
 })
 }, [])

 const toggleExpanded = useCallback((index: number) => {
 setSteps((prev) => prev.map((s, i) => (i === index ? { ...s, expanded: !s.expanded } : s)))
 }, [])

 const toStepTemplates = useCallback((): StepTemplate[] => {
 return steps.map((s) => {
 const tpl: StepTemplate = {
 name: s.name,
 description: s.description || null,
 step_type: s.step_type,
 dependencies: s.dependencies,
 estimated_duration_minutes: s.estimated_duration_minutes,
 assigned_role: s.assigned_role || null,
 execution_rule: s.dependencies.length > 1 ? s.execution_rule : null,
 condition: s.condition || null,
 is_terminal: s.is_terminal || undefined,
 }
 if (s.step_type === 'integration' && s.integration_type) {
 tpl.integration_type = s.integration_type
 if (s.integration_config) {
 try {
 tpl.integration_config = JSON.parse(s.integration_config)
 } catch {
 tpl.integration_config = null
 }
 }
 }
 if (s.step_type === 'llm_action' && s.integration_config) {
 try {
 tpl.integration_config = JSON.parse(s.integration_config)
 } catch {
 tpl.integration_config = null
 }
 }
 if (s.step_type === 'ruflo_agent') {
 tpl.integration_type = 'ruflo_agent'
 if (s.integration_config) {
 try {
 tpl.integration_config = JSON.parse(s.integration_config)
 } catch {
 tpl.integration_config = null
 }
 }
 }
 return tpl
 })
 }, [steps])

 const fromDefinition = useCallback((def: WorkflowDefinition) => {
 const templates = def.step_templates
 if (!Array.isArray(templates) || templates.length === 0) {
 setSteps([{ ...EMPTY_STEP }])
 return
 }
 setSteps(
 templates.map((t: StepTemplate, i: number) => ({
 name: t.name || '',
 description: t.description || '',
 step_type: t.step_type || 'manual',
 assigned_role: t.assigned_role || '',
 dependencies: Array.isArray(t.dependencies) ? t.dependencies : i > 0 ? [i - 1] : [],
 estimated_duration_minutes: t.estimated_duration_minutes ?? null,
 integration_type: t.integration_type || '',
 integration_config: t.integration_config ? JSON.stringify(t.integration_config, null, 2) : '',
 execution_rule: t.execution_rule || 'all',
 condition: t.condition || '',
 is_terminal: t.is_terminal || false,
 expanded: false,
 })),
 )
 }, [])

 const validate = useCallback(
 (name: string): BuilderErrors => {
 const errors: BuilderErrors = {}
 if (!name.trim()) errors.name = 'Name is required'
 if (steps.length === 0) errors.steps = 'At least one step is required'

 const stepErrors: Record<number, string> = {}
 steps.forEach((s, i) => {
 if (!s.name.trim()) stepErrors[i] = 'Step name is required'
 })

 if (hasCycle(steps)) {
 errors.steps = (errors.steps ? errors.steps + '. ' : '') + 'Dependency cycle detected'
 }

 if (Object.keys(stepErrors).length > 0) errors.stepErrors = stepErrors
 return errors
 },
 [steps],
 )

 return {
 steps,
 setSteps,
 addStep,
 addParallelStep,
 removeStep,
 updateStep,
 moveStepUp,
 moveStepDown,
 toggleExpanded,
 toStepTemplates,
 fromDefinition,
 validate,
 }
}
