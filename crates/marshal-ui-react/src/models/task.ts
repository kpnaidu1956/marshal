export interface Task {
 id: string
 organization_id: string
 title: string
 description: string | null
 status: string
 priority: string | null
 assigned_to: string | null
 goal_id: string | null
 due_date: string | null
 task_number: string | null
 team_assignment_id: string | null
 created_by: string | null
 needs_reassignment: boolean | null
 is_deleted: boolean | null
 created_at: string | null
 updated_at: string | null
}

export interface NewTask {
 organization_id: string
 title: string
 description?: string | null
 status?: string | null
 priority?: string | null
 assigned_to?: string | null
 goal_id?: string | null
 due_date?: string | null
}

export interface TaskUpdate {
 title?: string
 description?: string
 status?: string
 priority?: string
 assigned_to?: string | null
 goal_id?: string | null
 due_date?: string | null
}

export interface TaskComment {
 id: string
 task_id: string
 organization_id: string
 author_id: string
 content: string
 is_private: boolean | null
 created_at: string | null
 updated_at: string | null
}
