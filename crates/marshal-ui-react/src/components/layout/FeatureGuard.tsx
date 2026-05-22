import { Navigate, Outlet } from 'react-router-dom'
import { useAuthStore } from '@/stores/auth'
import { usePermissions } from '@/hooks/usePermissions'

interface FeatureGuardProps {
 feature: string
 action?: 'read' | 'write' | 'delete' | 'admin'
 children?: React.ReactNode
}

export function FeatureGuard({ feature, action = 'read', children }: FeatureGuardProps) {
 const isAdmin = useAuthStore((s) => s.user?.is_platform_admin ?? false)
 const { canRead, canWrite, canDelete, canAdmin, hasPermissions } = usePermissions()

 // Platform admins bypass all checks
 if (isAdmin) return children ? <>{children}</> : <Outlet />

 // If permissions haven't been loaded yet (null = legacy login without RBAC),
 // allow access for backward compatibility
 if (!hasPermissions()) return children ? <>{children}</> : <Outlet />

 let allowed = false
 switch (action) {
 case 'read': allowed = canRead(feature); break
 case 'write': allowed = canWrite(feature); break
 case 'delete': allowed = canDelete(feature); break
 case 'admin': allowed = canAdmin(feature); break
 }

 if (!allowed) {
 return <Navigate to="/" replace />
 }

 return children ? <>{children}</> : <Outlet />
}
