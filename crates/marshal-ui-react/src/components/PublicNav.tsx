import { useState } from 'react'
import { Link, useLocation } from 'react-router-dom'
import { Shield, Menu, X } from 'lucide-react'

const navLinks = [
 { to: '/demo', label: 'Demo' },
 { to: '/pricing', label: 'Pricing' },
 { to: '/contact', label: 'Contact' },
]

export function PublicNav() {
 const [mobileOpen, setMobileOpen] = useState(false)
 const location = useLocation()

 return (
 <nav className="sticky top-0 z-50 border-b border-gray-200/60 bg-white/80 backdrop-blur-xl">
 <div className="max-w-6xl mx-auto flex items-center justify-between px-6 py-3.5">
 {/* Logo */}
 <Link to="/demo" className="flex items-center gap-2.5 group">
 <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-indigo-500 to-indigo-700 flex items-center justify-center shadow-md shadow-indigo-500/20 group-hover:shadow-indigo-500/40 transition-shadow">
 <Shield className="w-4 h-4 text-white" />
 </div>
 <span className="text-lg font-bold tracking-tight text-gray-900">MARSHAL</span>
 </Link>

 {/* Desktop links */}
 <div className="hidden md:flex items-center gap-1">
 {navLinks.map((link) => (
 <Link
 key={link.to}
 to={link.to}
 className={`px-3.5 py-2 rounded-lg text-sm font-medium transition-colors ${
 location.pathname === link.to
 ? 'text-indigo-600 bg-indigo-50'
 : 'text-gray-600 hover:text-gray-900 hover:bg-gray-50'
 }`}
 >
 {link.label}
 </Link>
 ))}
 </div>

 {/* Desktop CTA */}
 <div className="hidden md:flex items-center gap-3">
 <Link
 to="/login"
 className="px-4 py-2 rounded-lg text-sm font-semibold bg-gradient-to-r from-indigo-600 to-indigo-700 text-white hover:from-indigo-700 hover:to-indigo-800 shadow-md shadow-indigo-500/20 hover:shadow-indigo-500/30 transition-all"
 >
 Sign in
 </Link>
 </div>

 {/* Mobile hamburger */}
 <button
 onClick={() => setMobileOpen(!mobileOpen)}
 className="md:hidden p-2 rounded-lg text-gray-600 hover:bg-gray-100 transition-colors"
 aria-label="Toggle menu"
 >
 {mobileOpen ? <X className="w-5 h-5" /> : <Menu className="w-5 h-5" />}
 </button>
 </div>

 {/* Mobile menu */}
 {mobileOpen && (
 <div className="md:hidden border-t border-gray-200/60 bg-white/95 backdrop-blur-xl">
 <div className="px-6 py-4 space-y-1">
 {navLinks.map((link) => (
 <Link
 key={link.to}
 to={link.to}
 onClick={() => setMobileOpen(false)}
 className={`block px-3.5 py-2.5 rounded-lg text-sm font-medium transition-colors ${
 location.pathname === link.to
 ? 'text-indigo-600 bg-indigo-50'
 : 'text-gray-600 hover:text-gray-900 hover:bg-gray-50'
 }`}
 >
 {link.label}
 </Link>
 ))}
 <div className="pt-3 border-t border-gray-200">
 <Link
 to="/login"
 onClick={() => setMobileOpen(false)}
 className="block text-center px-4 py-2.5 rounded-lg text-sm font-semibold bg-indigo-600 text-white hover:bg-indigo-700 transition-colors"
 >
 Sign in
 </Link>
 </div>
 </div>
 </div>
 )}
 </nav>
 )
}
