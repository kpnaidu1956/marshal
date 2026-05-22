import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { type Organization, orgNameToSlug } from '@/models/organization'

interface OrgState {
 currentOrg: Organization | null
 currentOrgSlug: string | null
 availableOrgs: Organization[]
 setCurrentOrg: (org: Organization | null) => void
 setAvailableOrgs: (orgs: Organization[]) => void
 currentOrgId: () => string | null
 clear: () => void
}

export const useOrgStore = create<OrgState>()(
 persist(
 (set, get) => ({
 currentOrg: null,
 currentOrgSlug: null,
 availableOrgs: [],

 setCurrentOrg: (org) => set({ currentOrg: org, currentOrgSlug: org ? orgNameToSlug(org.name) : null }),
 setAvailableOrgs: (orgs) => set({ availableOrgs: orgs }),

 currentOrgId: () => get().currentOrg?.id ?? null,

 clear: () => set({ currentOrg: null, currentOrgSlug: null, availableOrgs: [] }),
 }),
 {
 name: 'marshal_org',
 partialize: (state) => ({ currentOrg: state.currentOrg, currentOrgSlug: state.currentOrgSlug, availableOrgs: state.availableOrgs }),
 },
 ),
)
