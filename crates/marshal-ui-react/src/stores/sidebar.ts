import { create } from 'zustand'

interface SidebarState {
 collapsed: boolean
 mobileOpen: boolean
 toggleCollapsed: () => void
 toggleMobile: () => void
 closeMobile: () => void
}

export const useSidebarStore = create<SidebarState>()((set) => ({
 collapsed: false,
 mobileOpen: false,
 toggleCollapsed: () => set((s) => ({ collapsed: !s.collapsed })),
 toggleMobile: () => set((s) => ({ mobileOpen: !s.mobileOpen })),
 closeMobile: () => set({ mobileOpen: false }),
}))
