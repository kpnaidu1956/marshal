import type { BuilderStep } from './useWorkflowBuilder'

export interface GraphLevel {
 stepIndices: number[]
}

/** Compute the depth of each step (longest path from any root). */
function computeDepths(steps: BuilderStep[]): number[] {
 const depths = new Array<number>(steps.length).fill(-1)

 function dfs(i: number, visiting: Set<number>): number {
 if (depths[i] >= 0) return depths[i]
 if (visiting.has(i)) return 0 // cycle — break it
 visiting.add(i)
 let maxParent = -1
 for (const dep of steps[i].dependencies) {
 if (dep >= 0 && dep < steps.length && dep !== i) {
 maxParent = Math.max(maxParent, dfs(dep, visiting))
 }
 }
 visiting.delete(i)
 depths[i] = maxParent + 1
 return depths[i]
 }

 for (let i = 0; i < steps.length; i++) {
 if (depths[i] < 0) dfs(i, new Set())
 }
 return depths
}

/** Group steps into levels by their graph depth. */
export function computeLevels(steps: BuilderStep[]): GraphLevel[] {
 if (steps.length === 0) return []
 const depths = computeDepths(steps)
 const maxDepth = Math.max(...depths)
 const levels: GraphLevel[] = []
 for (let d = 0; d <= maxDepth; d++) {
 const indices = depths
 .map((depth, idx) => (depth === d ? idx : -1))
 .filter((idx) => idx >= 0)
 if (indices.length > 0) {
 levels.push({ stepIndices: indices })
 }
 }
 return levels
}

/** Build a set of connections between two adjacent levels for drawing connectors. */
export interface LevelEdge {
 parentCol: number
 childCol: number
 parentIdx: number
 childIdx: number
 condition?: string
 isApprovalParent: boolean
}

export function computeEdges(
 parentLevel: GraphLevel,
 childLevel: GraphLevel,
 steps: BuilderStep[],
): LevelEdge[] {
 const parentColMap = new Map<number, number>()
 parentLevel.stepIndices.forEach((idx, col) => parentColMap.set(idx, col))

 const edges: LevelEdge[] = []
 childLevel.stepIndices.forEach((childIdx, childCol) => {
 const step = steps[childIdx]
 for (const dep of step.dependencies) {
 const parentCol = parentColMap.get(dep)
 if (parentCol !== undefined) {
 edges.push({
 parentCol,
 childCol,
 parentIdx: dep,
 childIdx,
 condition: step.condition || undefined,
 isApprovalParent: steps[dep]?.step_type === 'approval',
 })
 }
 }
 })
 return edges
}

/** Check for cycles in the dependency graph. */
export function hasCycle(steps: BuilderStep[]): boolean {
 const visited = new Set<number>()
 const recStack = new Set<number>()

 function dfs(i: number): boolean {
 visited.add(i)
 recStack.add(i)
 for (const dep of steps[i].dependencies) {
 if (dep < 0 || dep >= steps.length) continue
 if (!visited.has(dep)) {
 if (dfs(dep)) return true
 } else if (recStack.has(dep)) {
 return true
 }
 }
 recStack.delete(i)
 return false
 }

 for (let i = 0; i < steps.length; i++) {
 if (!visited.has(i) && dfs(i)) return true
 }
 return false
}
