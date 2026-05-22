import { useEffect } from 'react'
import { Navigate, Outlet, useNavigate } from 'react-router-dom'
import { useAuthStore } from '@/stores/auth'

function isTokenExpired(token: string): boolean {
 try {
 const payload = JSON.parse(atob(token.split('.')[1]))
 return typeof payload.exp === 'number' && payload.exp * 1000 < Date.now()
 } catch {
 return true
 }
}

export function AuthGuard() {
 const token = useAuthStore((s) => s.token)
 const logout = useAuthStore((s) => s.logout)
 const navigate = useNavigate()

 // Periodic token expiry check (every 60 seconds)
 useEffect(() => {
 if (!token) return
 const interval = setInterval(() => {
 if (isTokenExpired(token)) {
 logout()
 navigate('/login', { replace: true })
 }
 }, 60_000)
 return () => clearInterval(interval)
 }, [token, logout, navigate])

 if (!token || isTokenExpired(token)) {
 // Clear stale token if expired
 if (token) logout()
 return <Navigate to="/login" replace />
 }

 return <Outlet />
}
