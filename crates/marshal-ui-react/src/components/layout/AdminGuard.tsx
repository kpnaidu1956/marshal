import { Navigate, Outlet } from 'react-router-dom'
import { useAuthStore } from '@/stores/auth'

export function AdminGuard() {
 const isAdmin = useAuthStore((s) => s.user?.is_platform_admin ?? false)

 if (!isAdmin) {
 return <Navigate to="/" replace />
 }

 return <Outlet />
}
