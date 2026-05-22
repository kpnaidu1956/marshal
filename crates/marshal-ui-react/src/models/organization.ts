export interface Organization {
 id: string
 name: string
 display_name: string | null
 description: string | null
 logo_url: string | null
 created_by: string | null
 created_at: string | null
 updated_at: string | null
}

/** Convert an organization name to its slug form. */
export function orgNameToSlug(name: string): string {
 return name.toLowerCase().replace(/ /g, '-')
}
