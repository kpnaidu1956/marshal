import { Outlet, useLocation } from 'react-router-dom'
import { Navigation } from './Navigation'
import { TrialBanner } from './TrialBanner'
import { Sidebar } from './Sidebar'
import { Footer } from '../Footer'
import { CommandPalette } from '../CommandPalette'

export function AppShell() {
 const location = useLocation()
 const showSidebar = false // Sidebar disabled — all navigation via top dropdowns

 return (
 <div className="min-h-screen flex flex-col bg-background">
 <TrialBanner />
 <Navigation />
 {showSidebar ? (
 <div className="flex flex-1">
 <Sidebar />
 <main className="flex-1 min-w-0">
 <div className="px-4 py-4 md:px-6 md:py-6">
 <Outlet />
 </div>
 </main>
 </div>
 ) : (
 <main className="flex-1">
 <div className="container mx-auto px-4 py-4 md:py-6">
 <Outlet />
 </div>
 </main>
 )}
 <Footer />
 <CommandPalette />
 </div>
 )
}
