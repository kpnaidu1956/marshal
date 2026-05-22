/**
 * AI Task Generator — orchestrates RAG knowledge base + BPE learned workflows
 * to generate tasks for achieving a goal.
 */

import type { RagClient, Citation } from './rag'
import type { BpeClient } from './bpe'
import type { Goal } from '@/models/goal'

export interface GeneratedTask {
 title: string
 description: string
 priority: 'Low' | 'Medium' | 'High'
 estimated_minutes: number | null
 sequence_order: number
 source: 'knowledge_base' | 'workflow' | 'ai'
 due_date?: string
 sequence_id?: string // BPE learned_sequence ID for feedback tracking
}

export interface GenerationResult {
 tasks: GeneratedTask[]
 citations: { filename: string; snippet: string }[]
 ragConfidence: number
 bpePatternCount: number
}

/**
 * Generate tasks for a goal using RAG (knowledge base) + BPE (workflow patterns).
 *
 * Flow:
 * 1. Parallel: query RAG for rules/guidelines + BPE for learned patterns + BPE definitions
 * 2. Build combined prompt with all context
 * 3. Call RAG with generation prompt → LLM returns structured task list
 * 4. Parse JSON, merge with BPE suggestions, return
 */
export async function generateTasksForGoal(
 goal: Goal,
 orgSlug: string,
 ragClient: RagClient,
 bpeClient: BpeClient,
): Promise<GenerationResult> {
 // Step 1: Parallel context gathering
 const [ragContext, bpeSuggestions, bpeDefinitions] = await Promise.all([
 queryKnowledgeBase(ragClient, goal, orgSlug),
 queryBpePatterns(bpeClient, goal, orgSlug),
 queryBpeDefinitions(bpeClient, orgSlug),
 ])

 // Step 2: Build generation prompt
 const prompt = buildGenerationPrompt(goal, ragContext, bpeSuggestions, bpeDefinitions)

 // Step 3: Call RAG with generation prompt
 let llmTasks: GeneratedTask[] = []
 try {
 const genResponse = await ragClient.queryV2({
 question: prompt,
 organization_id: orgSlug,
 top_k: 10,
 })

 llmTasks = parseTasksFromLlmResponse(genResponse.answer)
 } catch (err) {
 console.warn('LLM task generation failed, using BPE patterns only:', err)
 }

 // Step 4: Merge BPE steps as fallback/supplement
 const bpeTasks = bpeSuggestions.flatMap((s, si) =>
 (s.steps || []).map((step: { name: string; description?: string; estimated_duration_minutes?: number }, idx: number) => ({
 title: step.name,
 description: step.description || '',
 priority: 'Medium' as const,
 estimated_minutes: step.estimated_duration_minutes ?? null,
 sequence_order: si * 100 + idx + 1,
 source: 'workflow' as const,
 sequence_id: s.id, // Track which BPE sequence this came from
 }))
 )

 // Merge: LLM tasks first, then BPE tasks not already covered
 const allTasks = [...llmTasks]
 const existingTitles = new Set(allTasks.map((t) => t.title.toLowerCase()))
 for (const bt of bpeTasks) {
 if (!existingTitles.has(bt.title.toLowerCase())) {
 allTasks.push(bt)
 }
 }

 // If no tasks generated at all, create basic fallback tasks
 if (allTasks.length === 0) {
 allTasks.push(
 { title: `Define requirements for: ${goal.title}`, description: 'Gather and document all requirements and constraints.', priority: 'High', estimated_minutes: 60, sequence_order: 1, source: 'ai' },
 { title: `Plan approach for: ${goal.title}`, description: 'Create a detailed plan of action.', priority: 'High', estimated_minutes: 60, sequence_order: 2, source: 'ai' },
 { title: `Execute: ${goal.title}`, description: 'Carry out the planned actions.', priority: 'Medium', estimated_minutes: 120, sequence_order: 3, source: 'ai' },
 { title: `Review and verify: ${goal.title}`, description: 'Verify that the goal has been achieved and meets requirements.', priority: 'Medium', estimated_minutes: 30, sequence_order: 4, source: 'ai' },
 )
 }

 // Ensure sequence ordering
 allTasks.forEach((t, i) => { t.sequence_order = i + 1 })

 // Extract citations from RAG context
 const citations = ragContext.citations.map((c) => ({
 filename: c.filename,
 snippet: c.snippet,
 }))

 return {
 tasks: allTasks,
 citations,
 ragConfidence: ragContext.confidence,
 bpePatternCount: bpeSuggestions.length,
 }
}

/**
 * Auto-spread due dates between now and goal target date.
 */
export function spreadDueDates(tasks: GeneratedTask[], goalTargetDate: string | null): GeneratedTask[] {
 if (!goalTargetDate || tasks.length === 0) return tasks

 const now = new Date()
 const target = new Date(goalTargetDate)
 if (target <= now) return tasks

 const totalMs = target.getTime() - now.getTime()
 const stepMs = totalMs / tasks.length

 return tasks.map((t, i) => ({
 ...t,
 due_date: new Date(now.getTime() + stepMs * (i + 1)).toISOString().slice(0, 10),
 }))
}

// ── Exported helpers (reused by goalDecomposer) ──

export interface RagContextResult {
 answer: string
 citations: { filename: string; snippet: string }[]
 confidence: number
}

export async function queryKnowledgeBase(
 ragClient: RagClient,
 goal: Goal,
 orgSlug: string,
): Promise<RagContextResult> {
 try {
 const response = await ragClient.queryV2({
 question: `What rules, guidelines, policies, procedures, and best practices apply to the following goal? Goal: "${goal.title}". ${goal.description || ''}. List any relevant requirements, constraints, approval processes, or standards.`,
 organization_id: orgSlug,
 top_k: 15,
 })
 return {
 answer: response.answer || '',
 citations: (response.citations || []).map((c: Citation) => ({
 filename: c.source?.filename ?? 'Unknown',
 snippet: c.snippet?.text ?? '',
 })),
 confidence: (response as Record<string, unknown>).confidence as number ?? 0,
 }
 } catch {
 return { answer: '', citations: [], confidence: 0 }
 }
}

export interface BpeSuggestion {
 id: string
 task_category: string
 steps: { name: string; description?: string; step_type?: string; estimated_duration_minutes?: number }[]
 acceptance_rate?: number
}

export async function queryBpePatterns(
 bpeClient: BpeClient,
 goal: Goal,
 orgSlug: string,
): Promise<BpeSuggestion[]> {
 try {
 // Try to infer a task category from the goal title
 const category = inferCategory(goal.title)
 const response = await bpeClient.suggestSequence({
 organization_id: orgSlug,
 prompt: goal.title,
 task_category: category,
 limit: 5,
 })
 return (response.data || []) as BpeSuggestion[]
 } catch {
 return []
 }
}

export interface BpeDefinition {
 name: string
 description?: string
 category: string
 step_templates: { name: string; description?: string; step_type?: string; estimated_duration_minutes?: number }[]
}

export async function queryBpeDefinitions(
 bpeClient: BpeClient,
 orgSlug: string,
): Promise<BpeDefinition[]> {
 try {
 const response = await bpeClient.listDefinitions(orgSlug, 1, 20)
 return (response.data || []) as BpeDefinition[]
 } catch {
 return []
 }
}

export function inferCategory(title: string): string {
 const lower = title.toLowerCase()
 if (lower.includes('onboard')) return 'onboarding'
 if (lower.includes('hire') || lower.includes('recruit')) return 'hiring'
 if (lower.includes('train')) return 'training'
 if (lower.includes('review') || lower.includes('audit')) return 'review'
 if (lower.includes('deploy') || lower.includes('launch')) return 'deployment'
 if (lower.includes('fix') || lower.includes('bug')) return 'bug_fix'
 if (lower.includes('improve') || lower.includes('optimize')) return 'improvement'
 if (lower.includes('compliance') || lower.includes('policy')) return 'compliance'
 if (lower.includes('safety') || lower.includes('incident')) return 'safety'
 return 'general'
}

function buildGenerationPrompt(
 goal: Goal,
 ragContext: RagContextResult,
 bpeSuggestions: BpeSuggestion[],
 bpeDefinitions: BpeDefinition[],
): string {
 let prompt = `You are a task planning assistant. Generate a structured list of actionable tasks to achieve the following goal.

GOAL: ${goal.title}
${goal.description ? `DESCRIPTION: ${goal.description}` : ''}
${goal.target_date ? `TARGET DATE: ${goal.target_date}` : ''}

`

 if (ragContext.answer) {
 prompt += `ORGANIZATIONAL GUIDELINES AND RULES (from knowledge base):
${ragContext.answer}

`
 }

 if (bpeSuggestions.length > 0) {
 prompt += `LEARNED WORKFLOW PATTERNS (from past successful workflows):
${bpeSuggestions.map((s) =>
 s.steps.map((step) => `- ${step.name}${step.description ? ': ' + step.description : ''}`).join('\n')
).join('\n')}

`
 }

 if (bpeDefinitions.length > 0) {
 const relevant = bpeDefinitions.slice(0, 3)
 prompt += `AVAILABLE WORKFLOW TEMPLATES:
${relevant.map((d) => `- ${d.name}: ${d.step_templates.map((s) => s.name).join(' → ')}`).join('\n')}

`
 }

 prompt += `Generate 3-8 tasks. Each task must align with the organizational guidelines above.
Respond with ONLY a JSON array, no markdown, no explanation. Each element must have:
- "title": concise task title
- "description": 1-2 sentence description
- "priority": "Low", "Medium", or "High"
- "estimated_minutes": estimated time in minutes (integer or null)
- "sequence_order": integer starting from 1

Example: [{"title":"Review requirements","description":"Gather and review all requirements.","priority":"High","estimated_minutes":60,"sequence_order":1}]

JSON array:`

 return prompt
}

function parseTasksFromLlmResponse(answer: string): GeneratedTask[] {
 if (!answer) return []

 // Try to extract JSON array from the response
 // The LLM may wrap it in markdown code blocks or add extra text
 let jsonStr = answer.trim()

 // Remove markdown code fences
 jsonStr = jsonStr.replace(/```json\s*/g, '').replace(/```\s*/g, '')

 // Find the JSON array
 const arrayMatch = jsonStr.match(/\[[\s\S]*\]/)
 if (!arrayMatch) {
 console.warn('Could not find JSON array in LLM response:', answer.slice(0, 200))
 return []
 }

 try {
 const parsed = JSON.parse(arrayMatch[0])
 if (!Array.isArray(parsed)) return []

 return parsed.map((item: Record<string, unknown>, idx: number) => ({
 title: String(item.title || `Task ${idx + 1}`),
 description: String(item.description || ''),
 priority: (['Low', 'Medium', 'High'].includes(String(item.priority)) ? String(item.priority) : 'Medium') as 'Low' | 'Medium' | 'High',
 estimated_minutes: typeof item.estimated_minutes === 'number' ? item.estimated_minutes : null,
 sequence_order: typeof item.sequence_order === 'number' ? item.sequence_order : idx + 1,
 source: 'ai' as const,
 }))
 } catch (err) {
 console.warn('Failed to parse LLM JSON response:', err)
 return []
 }
}
