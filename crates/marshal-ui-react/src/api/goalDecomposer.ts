/**
 * AI Goal Decomposer — recursively breaks a goal into sub-goals and tasks,
 * grounded in the organization's knowledge base and BPE workflow patterns.
 */

import type { RagClient } from './rag'
import type { BpeClient } from './bpe'
import type { Goal } from '@/models/goal'
import {
  queryKnowledgeBase,
  queryBpePatterns,
  queryBpeDefinitions,
  type RagContextResult,
  type BpeSuggestion,
  type BpeDefinition,
} from './taskGenerator'

export interface DecomposedTask {
  title: string
  description: string
  priority: 'Low' | 'Medium' | 'High'
  sequence_order: number
  selected: boolean
  source: 'knowledge_base' | 'workflow' | 'ai'
}

export interface DecomposedGoal {
  title: string
  description: string
  selected: boolean
  sub_goals: DecomposedGoal[]
  tasks: DecomposedTask[]
}

export interface DecompositionResult {
  tree: DecomposedGoal
  citations: { filename: string; snippet: string }[]
  ragConfidence: number
  bpePatternCount: number
}

/**
 * Decompose a goal into a recursive hierarchy of sub-goals and tasks.
 */
export async function decomposeGoal(
  goal: Goal,
  orgSlug: string,
  ragClient: RagClient,
  bpeClient: BpeClient,
): Promise<DecompositionResult> {
  // Step 1: Parallel context gathering (reuses taskGenerator helpers)
  const [ragContext, bpeSuggestions, bpeDefinitions] = await Promise.all([
    queryKnowledgeBase(ragClient, goal, orgSlug),
    queryBpePatterns(bpeClient, goal, orgSlug),
    queryBpeDefinitions(bpeClient, orgSlug),
  ])

  // Step 2: Build decomposition prompt
  const prompt = buildDecompositionPrompt(goal, ragContext, bpeSuggestions, bpeDefinitions)

  // Step 3: Call RAG with prompt
  let tree: DecomposedGoal = {
    title: goal.title,
    description: goal.description ?? '',
    selected: true,
    sub_goals: [],
    tasks: [],
  }

  try {
    const response = await ragClient.queryV2({
      question: prompt,
      organization_id: orgSlug,
      top_k: 10,
    })
    const parsed = parseTreeFromLlmResponse(response.answer)
    if (parsed) {
      tree.sub_goals = parsed.sub_goals
      tree.tasks = parsed.tasks
    }
  } catch (err) {
    console.warn('LLM decomposition failed:', err)
  }

  // Step 4: If LLM returned nothing, build fallback from BPE patterns
  if (tree.sub_goals.length === 0 && tree.tasks.length === 0) {
    tree = buildFallbackTree(goal)
  }

  // Mark all items as selected by default
  markAllSelected(tree)

  return {
    tree,
    citations: ragContext.citations,
    ragConfidence: ragContext.confidence,
    bpePatternCount: bpeSuggestions.length,
  }
}

/** Count total sub-goals and tasks in the tree */
export function countTree(tree: DecomposedGoal): { goals: number; tasks: number; levels: number } {
  let goals = 0
  let tasks = tree.tasks.filter((t) => t.selected).length
  let maxDepth = 0

  function walk(node: DecomposedGoal, depth: number) {
    if (depth > maxDepth) maxDepth = depth
    for (const sg of node.sub_goals) {
      if (sg.selected) {
        goals++
        tasks += sg.tasks.filter((t) => t.selected).length
        walk(sg, depth + 1)
      }
    }
  }
  walk(tree, 0)
  return { goals, tasks, levels: maxDepth }
}

// ── Internal ──

function markAllSelected(node: DecomposedGoal) {
  node.selected = true
  for (const t of node.tasks) t.selected = true
  for (const sg of node.sub_goals) markAllSelected(sg)
}

function buildDecompositionPrompt(
  goal: Goal,
  ragContext: RagContextResult,
  bpeSuggestions: BpeSuggestion[],
  bpeDefinitions: BpeDefinition[],
): string {
  let prompt = `You are a strategic goal decomposition assistant. Break down the following goal into a hierarchical structure of sub-goals and tasks.

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
${relevant.map((d) => `- ${d.name}: ${d.step_templates.map((s) => s.name).join(' -> ')}`).join('\n')}

`
  }

  prompt += `Rules:
- Each sub-goal should be a meaningful milestone toward the parent goal
- Tasks are concrete, actionable work items assigned under a goal
- Sub-goals can have their own sub-goals for complex areas (recursive decomposition)
- All items must align with the organizational guidelines above
- Generate 2-5 sub-goals, each with 2-5 tasks
- Sub-goals may have nested sub-goals if the scope warrants it (keep to 2-3 levels max)

Respond with ONLY a valid JSON object (no markdown, no explanation). Format:
{"sub_goals":[{"title":"...","description":"...","sub_goals":[],"tasks":[{"title":"...","description":"...","priority":"High","sequence_order":1}]}],"tasks":[{"title":"...","description":"...","priority":"Medium","sequence_order":1}]}

JSON:`

  return prompt
}

function parseTreeFromLlmResponse(answer: string): { sub_goals: DecomposedGoal[]; tasks: DecomposedTask[] } | null {
  if (!answer) return null

  let jsonStr = answer.trim()
  jsonStr = jsonStr.replace(/```json\s*/g, '').replace(/```\s*/g, '')

  // Find the JSON object
  const objMatch = jsonStr.match(/\{[\s\S]*\}/)
  if (!objMatch) {
    console.warn('Could not find JSON object in LLM response:', answer.slice(0, 200))
    return null
  }

  try {
    const parsed = JSON.parse(objMatch[0])
    return {
      sub_goals: parseSubGoals(parsed.sub_goals),
      tasks: parseTasks(parsed.tasks),
    }
  } catch (err) {
    console.warn('Failed to parse LLM decomposition JSON:', err)
    return null
  }
}

function parseSubGoals(arr: unknown): DecomposedGoal[] {
  if (!Array.isArray(arr)) return []
  return arr.map((item: Record<string, unknown>) => ({
    title: String(item.title || 'Sub-goal'),
    description: String(item.description || ''),
    selected: true,
    sub_goals: parseSubGoals(item.sub_goals),
    tasks: parseTasks(item.tasks),
  }))
}

function parseTasks(arr: unknown): DecomposedTask[] {
  if (!Array.isArray(arr)) return []
  return arr.map((item: Record<string, unknown>, idx: number) => ({
    title: String(item.title || `Task ${idx + 1}`),
    description: String(item.description || ''),
    priority: (['Low', 'Medium', 'High'].includes(String(item.priority)) ? String(item.priority) : 'Medium') as 'Low' | 'Medium' | 'High',
    sequence_order: typeof item.sequence_order === 'number' ? item.sequence_order : idx + 1,
    selected: true,
    source: 'ai' as const,
  }))
}

function buildFallbackTree(goal: Goal): DecomposedGoal {
  return {
    title: goal.title,
    description: goal.description ?? '',
    selected: true,
    sub_goals: [
      {
        title: `Planning: ${goal.title}`,
        description: 'Define requirements, constraints, and approach.',
        selected: true,
        sub_goals: [],
        tasks: [
          { title: 'Gather requirements', description: 'Document all requirements and constraints.', priority: 'High', sequence_order: 1, selected: true, source: 'ai' },
          { title: 'Identify stakeholders', description: 'List all stakeholders and their roles.', priority: 'High', sequence_order: 2, selected: true, source: 'ai' },
          { title: 'Define success criteria', description: 'Establish measurable success criteria.', priority: 'Medium', sequence_order: 3, selected: true, source: 'ai' },
        ],
      },
      {
        title: `Execution: ${goal.title}`,
        description: 'Carry out the planned work.',
        selected: true,
        sub_goals: [],
        tasks: [
          { title: 'Execute primary deliverables', description: 'Complete the main work items.', priority: 'High', sequence_order: 1, selected: true, source: 'ai' },
          { title: 'Track progress', description: 'Monitor and report on progress.', priority: 'Medium', sequence_order: 2, selected: true, source: 'ai' },
        ],
      },
      {
        title: `Review: ${goal.title}`,
        description: 'Verify and close out the goal.',
        selected: true,
        sub_goals: [],
        tasks: [
          { title: 'Review deliverables', description: 'Verify all deliverables meet requirements.', priority: 'High', sequence_order: 1, selected: true, source: 'ai' },
          { title: 'Document lessons learned', description: 'Record what worked and what to improve.', priority: 'Low', sequence_order: 2, selected: true, source: 'ai' },
        ],
      },
    ],
    tasks: [],
  }
}
