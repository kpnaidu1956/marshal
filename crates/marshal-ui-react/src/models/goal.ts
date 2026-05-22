export interface Goal {
 id: string
 organization_id: string
 title: string
 description: string | null
 status: string
 progress: number | null
 parent_goal_id: string | null
 target_date: string | null
 created_by: string | null
 created_at: string | null
 updated_at: string | null
}

export interface NewGoal {
 organization_id: string
 title: string
 description?: string | null
 status?: string | null
 progress?: number | null
 parent_goal_id?: string | null
 target_date?: string | null
 created_by?: string | null
}

export interface GoalUpdate {
 title?: string
 description?: string
 status?: string
 progress?: number | null
 parent_goal_id?: string | null
 target_date?: string | null
}
