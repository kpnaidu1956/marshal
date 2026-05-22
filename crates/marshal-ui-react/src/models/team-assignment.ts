export interface TeamAssignment {
 id: string
 organization_id: string
 title: string
 description: string | null
 status: string | null
 user_id: string | null
 assignment_date: string | null
 needs_reassignment: boolean | null
 created_at: string | null
 updated_at: string | null
}
