export interface User {
 id: string
 organization_id: string
 first_name: string | null
 last_name: string | null
 username: string | null
 email: string | null
 mobile_phone: string | null
 avatar_url: string | null
 badge_number: string | null
 title: string | null
 level: string | null
 manager_id: string | null
 is_deleted: boolean | null
 deleted_at: string | null
 created_at: string | null
 updated_at: string | null
 is_platform_admin: boolean
}
