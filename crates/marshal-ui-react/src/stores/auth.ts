import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { User } from '@/models/user'
import { useOrgStore } from './org'

type PermissionMap = Record<string, string[]>

interface AuthState {
 token: string | null
 user: User | null
 permissions: PermissionMap | null
 login: (token: string, user: User, permissions?: PermissionMap | null) => void
 setPermissions: (permissions: PermissionMap) => void
 logout: () => void
 isAuthenticated: () => boolean
 isPlatformAdmin: () => boolean
}

function isTokenExpired(token: string): boolean {
 try {
 const payload = JSON.parse(atob(token.split('.')[1]))
 return typeof payload.exp === 'number' && payload.exp * 1000 < Date.now()
 } catch {
 return true
 }
}

export const useAuthStore = create<AuthState>()(
 persist(
 (set, get) => ({
 token: null,
 user: null,
 permissions: null,

 login: (token, user, permissions) => set({ token, user, permissions: permissions ?? null }),

 setPermissions: (permissions) => set({ permissions }),

 logout: () => {
 set({ token: null, user: null, permissions: null })
 useOrgStore.getState().clear()
 },

 isAuthenticated: () => {
 const token = get().token
 if (!token) return false
 return !isTokenExpired(token)
 },

 isPlatformAdmin: () => get().user?.is_platform_admin ?? false,
 }),
 {
 name: 'marshal_auth',
 partialize: (state) => ({ token: state.token, user: state.user, permissions: state.permissions }),
 },
 ),
)
