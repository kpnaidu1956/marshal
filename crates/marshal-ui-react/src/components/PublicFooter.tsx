import { Link } from 'react-router-dom'
import { Shield } from 'lucide-react'

export function PublicFooter() {
 return (
 <footer className="border-t border-gray-200/60 bg-white">
 <div className="max-w-6xl mx-auto px-6 py-12">
 <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-8">
 {/* Brand */}
 <div className="sm:col-span-2 lg:col-span-1">
 <Link to="/demo" className="flex items-center gap-2.5 mb-4">
 <div className="w-7 h-7 rounded-lg bg-gradient-to-br from-indigo-500 to-indigo-700 flex items-center justify-center">
 <Shield className="w-3.5 h-3.5 text-white" />
 </div>
 <span className="text-base font-bold tracking-tight text-gray-900">MARSHAL</span>
 </Link>
 <p className="text-sm text-gray-500 leading-relaxed max-w-xs">
 AI-powered management platform for modern teams. Goals, tasks, documents, and analytics in one place.
 </p>
 </div>

 {/* Product */}
 <div>
 <h4 className="text-xs font-semibold uppercase tracking-wider text-gray-500 mb-4">Product</h4>
 <ul className="space-y-2.5">
 {[
 { to: '/demo', label: 'Demo' },
 { to: '/pricing', label: 'Pricing' },
 { to: '/login', label: 'Sign In' },
 ].map((link) => (
 <li key={link.to}>
 <Link to={link.to} className="text-sm text-gray-600 hover:text-indigo-600 transition-colors">
 {link.label}
 </Link>
 </li>
 ))}
 </ul>
 </div>

 {/* Company */}
 <div>
 <h4 className="text-xs font-semibold uppercase tracking-wider text-gray-500 mb-4">Company</h4>
 <ul className="space-y-2.5">
 {[
 { to: '/contact', label: 'Contact' },
 { to: '/pricing', label: 'Enterprise' },
 ].map((link) => (
 <li key={link.to}>
 <Link to={link.to} className="text-sm text-gray-600 hover:text-indigo-600 transition-colors">
 {link.label}
 </Link>
 </li>
 ))}
 </ul>
 </div>

 {/* Legal placeholder */}
 <div>
 <h4 className="text-xs font-semibold uppercase tracking-wider text-gray-500 mb-4">Legal</h4>
 <ul className="space-y-2.5">
 <li><span className="text-sm text-gray-500">Privacy Policy</span></li>
 <li><span className="text-sm text-gray-500">Terms of Service</span></li>
 </ul>
 </div>
 </div>

 {/* Bottom bar */}
 <div className="mt-10 pt-6 border-t border-gray-200/60 flex flex-col sm:flex-row items-center justify-between gap-3">
 <p className="text-xs text-gray-500">
 &copy; {new Date().getFullYear()} Marshal. All rights reserved.
 </p>
 <p className="text-xs text-gray-500">
 Marshal AI Management System
 </p>
 </div>
 </div>
 </footer>
 )
}
