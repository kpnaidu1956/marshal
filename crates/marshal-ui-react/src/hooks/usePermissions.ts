import { useAuthStore } from '@/stores/auth'

export type PermissionMap = Record<string, string[]>

export function usePermissions() {
 const permissions: PermissionMap | null = useAuthStore((s) => s.permissions)

 return {
  permissions,
  canRead: (feature: string) => {
   if (!permissions) return false
   const actions = permissions[feature]
   return actions ? actions.includes('read') || actions.includes('admin') : false
  },
  canWrite: (feature: string) => {
   if (!permissions) return false
   const actions = permissions[feature]
   return actions ? actions.includes('write') || actions.includes('admin') : false
  },
  canDelete: (feature: string) => {
   if (!permissions) return false
   const actions = permissions[feature]
   return actions ? actions.includes('delete') || actions.includes('admin') : false
  },
  canAdmin: (feature: string) => {
   if (!permissions) return false
   const actions = permissions[feature]
   return actions ? actions.includes('admin') : false
  },
  hasPermissions: () => permissions !== null,
 }
}
