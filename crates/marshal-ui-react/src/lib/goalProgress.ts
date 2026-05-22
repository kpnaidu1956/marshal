interface GoalLike {
 id: string
 status: string | null
 parent_goal_id: string | null
}

interface TaskLike {
 status: string
 goal_id: string | null
}

/**
 * Calculate progress % for a goal based on its child goals and linked tasks.
 * Completed items count as 100%, in-progress as 50%, others as 0%.
 */
export function calculateGoalProgress(
 goalId: string,
 allGoals: GoalLike[],
 allTasks: TaskLike[],
): number {
 const childGoals = allGoals.filter((g) => g.parent_goal_id === goalId)
 const goalTasks = allTasks.filter((t) => t.goal_id === goalId)

 const items: number[] = []

 for (const child of childGoals) {
 const childProgress = calculateGoalProgress(child.id, allGoals, allTasks)
 items.push(childProgress)
 }

 for (const task of goalTasks) {
 const s = (task.status || '').trim().toLowerCase()
 if (s === 'completed') items.push(100)
 else if (s === 'in-progress' || s === 'in progress' || s === 'in_progress') items.push(50)
 else items.push(0)
 }

 if (items.length === 0) return 0
 return Math.round(items.reduce((a, b) => a + b, 0) / items.length)
}

/**
 * Determine what the status of a parent goal should be based on its children.
 */
export function deriveGoalStatus(
 goalId: string,
 allGoals: GoalLike[],
 allTasks: TaskLike[],
): string {
 const progress = calculateGoalProgress(goalId, allGoals, allTasks)
 if (progress >= 100) return 'completed'
 if (progress > 0) return 'in_progress'
 return 'not_started'
}
