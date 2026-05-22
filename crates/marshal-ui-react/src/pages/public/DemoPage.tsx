import { Link } from 'react-router-dom'
import {
 Brain, Target, Users, FileSearch, BarChart3, MessageSquare,
 Zap, ArrowRight, Search, CheckCircle2, Upload, Sparkles,
 Quote, Shield, Lock, Clock
} from 'lucide-react'
import { PublicNav } from '@/components/PublicNav'
import { PublicFooter } from '@/components/PublicFooter'

const features = [
 {
 icon: Brain,
 title: 'AI-Powered RAG Search',
 description: 'Ask natural language questions across all your documents. Get answers with source citations powered by retrieval-augmented generation.',
 },
 {
 icon: Target,
 title: 'Goal & Task Management',
 description: 'Create hierarchical goals with linked tasks, track progress with real-time dashboards, and assign work to team members.',
 },
 {
 icon: Users,
 title: 'Team Analytics',
 description: 'Visualize team workload, collaboration networks, and individual performance with interactive charts and leaderboards.',
 },
 {
 icon: FileSearch,
 title: 'Document Intelligence',
 description: 'Upload PDFs, Word docs, and more. Marshal chunks, embeds, and indexes your files for instant semantic search.',
 },
 {
 icon: BarChart3,
 title: 'Performance Dashboards',
 description: 'Track completion rates, task distribution by priority, and goal progress with real-time analytics and gauge charts.',
 },
 {
 icon: MessageSquare,
 title: 'Collaboration Hub',
 description: 'Comment on tasks, view activity timelines, and keep your team aligned with a centralized communication feed.',
 },
]

const steps = [
 {
 icon: Upload,
 title: 'Upload your documents',
 description: 'Drag and drop PDFs, Word docs, or Markdown files. Marshal automatically chunks and embeds them for search.',
 },
 {
 icon: Search,
 title: 'Ask anything',
 description: 'Type natural language questions. Our AI retrieves relevant passages and synthesizes answers with citations.',
 },
 {
 icon: CheckCircle2,
 title: 'Track and deliver',
 description: 'Create goals from insights, assign tasks, and monitor progress with real-time analytics dashboards.',
 },
]

export function DemoPage() {
 return (
 <div className="min-h-screen bg-white">
 <PublicNav />

 {/* Hero */}
 <section className="relative overflow-hidden">
 {/* Decorative background */}
 <div className="absolute inset-0 bg-gradient-to-b from-indigo-50/50 via-white to-white" />
 <div className="absolute top-0 left-1/2 -translate-x-1/2 w-[800px] h-[600px] bg-gradient-to-br from-indigo-200/30 to-purple-200/20 rounded-full blur-3xl" />

 <div className="relative max-w-5xl mx-auto px-6 pt-20 pb-16 text-center">
 <div className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-full bg-indigo-50 border border-indigo-100 text-xs font-medium text-indigo-600 mb-8">
 <Zap className="w-3 h-3" />
 Now with advanced RAG search
 </div>

 <h1 className="text-4xl sm:text-5xl lg:text-6xl font-bold text-gray-900 leading-[1.1] tracking-tight">
 The{' '}
 <span className="bg-gradient-to-r from-indigo-600 via-purple-600 to-indigo-600 bg-clip-text text-transparent">
 AI-powered
 </span>{' '}
 platform<br className="hidden sm:block" /> for modern teams
 </h1>

 <p className="mt-6 text-lg sm:text-xl text-gray-600 max-w-2xl mx-auto leading-relaxed">
 Marshal combines task management, goal tracking, and AI-powered document intelligence into a single platform that helps teams move faster.
 </p>

 <div className="mt-10 flex flex-col sm:flex-row items-center justify-center gap-4">
 <Link
 to="/login"
 className="w-full sm:w-auto inline-flex items-center justify-center gap-2 px-7 py-3.5 rounded-xl bg-gradient-to-r from-indigo-600 to-indigo-700 text-white font-semibold text-sm hover:from-indigo-700 hover:to-indigo-800 shadow-lg shadow-indigo-500/25 hover:shadow-indigo-500/40 transition-all"
 >
 Get Started Free
 <ArrowRight className="w-4 h-4" />
 </Link>
 <Link
 to="/pricing"
 className="w-full sm:w-auto inline-flex items-center justify-center px-7 py-3.5 rounded-xl border border-gray-300 text-gray-700 font-semibold text-sm hover:bg-gray-50 transition-colors"
 >
 View Pricing
 </Link>
 </div>
 </div>
 </section>

 {/* Stats bar */}
 <section className="border-y border-gray-200/60 bg-gray-50/50">
 <div className="max-w-5xl mx-auto px-6 py-8">
 <p className="text-center text-sm font-medium text-gray-500 mb-6">Trusted by teams building the future</p>
 <div className="grid grid-cols-2 md:grid-cols-4 gap-6 md:gap-8">
 {[
 { value: '2,400+', label: 'Teams' },
 { value: '1.2M', label: 'Tasks Managed' },
 { value: '500K', label: 'Documents Indexed' },
 { value: '99.9%', label: 'Uptime' },
 ].map((stat) => (
 <div key={stat.label} className="text-center">
 <p className="text-2xl sm:text-3xl font-bold text-gray-900">{stat.value}</p>
 <p className="text-xs sm:text-sm text-gray-500 mt-1">{stat.label}</p>
 </div>
 ))}
 </div>
 </div>
 </section>

 {/* Product preview */}
 <section className="max-w-5xl mx-auto px-6 py-20">
 <div className="rounded-2xl border border-gray-200 bg-white shadow-2xl shadow-gray-300/30 overflow-hidden">
 {/* Browser chrome */}
 <div className="flex items-center gap-2 px-4 py-3 border-b border-gray-200 bg-gray-50">
 <div className="flex items-center gap-1.5">
 <div className="w-3 h-3 rounded-full bg-red-400" />
 <div className="w-3 h-3 rounded-full bg-yellow-400" />
 <div className="w-3 h-3 rounded-full bg-green-400" />
 </div>
 <div className="flex-1 mx-4">
 <div className="max-w-md mx-auto flex items-center gap-2 px-3 py-1.5 rounded-lg bg-gray-100 border border-gray-200">
 <Lock className="w-3 h-3 text-gray-500" />
 <span className="text-xs text-gray-500 font-mono">your-domain.com/marshal</span>
 </div>
 </div>
 </div>

 {/* Mock dashboard */}
 <div className="p-6 sm:p-10">
 {/* Stat cards */}
 <div className="grid grid-cols-1 sm:grid-cols-3 gap-4 mb-8">
 <div className="bg-gradient-to-br from-indigo-50 to-indigo-100/50 rounded-xl p-5 border border-indigo-100">
 <div className="flex items-center justify-between mb-2">
 <span className="text-xs font-medium text-indigo-600 uppercase tracking-wider">Tasks</span>
 <Target className="w-4 h-4 text-indigo-400" />
 </div>
 <p className="text-3xl font-bold text-gray-900">247</p>
 <p className="text-xs text-indigo-600/70 mt-1">+12 this week</p>
 </div>
 <div className="bg-gradient-to-br from-emerald-50 to-emerald-100/50 rounded-xl p-5 border border-emerald-100">
 <div className="flex items-center justify-between mb-2">
 <span className="text-xs font-medium text-emerald-600 uppercase tracking-wider">Completion</span>
 <CheckCircle2 className="w-4 h-4 text-emerald-400" />
 </div>
 <p className="text-3xl font-bold text-gray-900">89%</p>
 <p className="text-xs text-emerald-600/70 mt-1">+3% from last month</p>
 </div>
 <div className="bg-gradient-to-br from-purple-50 to-purple-100/50 rounded-xl p-5 border border-purple-100">
 <div className="flex items-center justify-between mb-2">
 <span className="text-xs font-medium text-purple-600 uppercase tracking-wider">Documents</span>
 <FileSearch className="w-4 h-4 text-purple-400" />
 </div>
 <p className="text-3xl font-bold text-gray-900">1,248</p>
 <p className="text-xs text-purple-600/70 mt-1">Fully indexed</p>
 </div>
 </div>

 {/* RAG query mockup */}
 <div className="bg-gray-50 rounded-xl p-6 border border-gray-200">
 <div className="flex items-center gap-3 mb-4">
 <div className="w-8 h-8 rounded-lg bg-indigo-100 flex items-center justify-center">
 <Brain className="w-4 h-4 text-indigo-500" />
 </div>
 <div className="flex-1 flex items-center gap-2 px-3.5 py-2 rounded-lg bg-white border border-gray-200 text-sm text-gray-500">
 <Search className="w-3.5 h-3.5" />
 Ask anything about your documents...
 </div>
 </div>
 <div className="bg-white rounded-lg border border-gray-200 p-5">
 <p className="text-sm text-gray-800 leading-relaxed">
 <span className="text-indigo-600 font-semibold">Q:</span> What are the key deliverables for Q1?
 </p>
 <div className="mt-4 pt-4 border-t border-gray-100">
 <p className="text-sm text-gray-600 leading-relaxed">
 Based on the project charter and sprint plans, the Q1 deliverables include: API gateway deployment, user authentication module, and the analytics dashboard. These are tracked across 3 goals with 24 linked tasks.
 </p>
 <div className="mt-3 flex flex-wrap gap-2">
 <span className="text-xs px-2.5 py-1 rounded-md bg-blue-50 text-blue-600 border border-blue-100 font-medium">project-charter.pdf p.3</span>
 <span className="text-xs px-2.5 py-1 rounded-md bg-blue-50 text-blue-600 border border-blue-100 font-medium">sprint-plan-q1.docx p.1</span>
 </div>
 </div>
 </div>
 </div>
 </div>
 </div>
 </section>

 {/* Feature grid */}
 <section className="max-w-5xl mx-auto px-6 pb-20">
 <div className="text-center mb-12">
 <h2 className="text-3xl sm:text-4xl font-bold text-gray-900 tracking-tight">Everything you need</h2>
 <p className="text-gray-500 mt-3 text-lg">One platform for goals, tasks, documents, and analytics.</p>
 </div>
 <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-5">
 {features.map((f) => (
 <div
 key={f.title}
 className="group bg-white border border-gray-200 rounded-xl p-6 hover:shadow-xl hover:shadow-indigo-100/40 hover:border-indigo-200 transition-all duration-300 hover:-translate-y-0.5"
 >
 <div className="w-10 h-10 rounded-lg bg-indigo-50 flex items-center justify-center mb-4 group-hover:bg-indigo-100 transition-colors">
 <f.icon className="w-5 h-5 text-indigo-500" />
 </div>
 <h3 className="font-semibold text-gray-900 mb-2">{f.title}</h3>
 <p className="text-sm text-gray-500 leading-relaxed">{f.description}</p>
 </div>
 ))}
 </div>
 </section>

 {/* How it works */}
 <section className="bg-gray-50 border-y border-gray-200/60">
 <div className="max-w-5xl mx-auto px-6 py-20">
 <div className="text-center mb-14">
 <h2 className="text-3xl sm:text-4xl font-bold text-gray-900 tracking-tight">How it works</h2>
 <p className="text-gray-500 mt-3 text-lg">Get started in three simple steps.</p>
 </div>
 <div className="grid grid-cols-1 md:grid-cols-3 gap-8 md:gap-6">
 {steps.map((step, i) => (
 <div key={step.title} className="relative text-center">
 <div className="inline-flex items-center justify-center w-14 h-14 rounded-2xl bg-white border border-gray-200 shadow-sm mb-5">
 <step.icon className="w-6 h-6 text-indigo-500" />
 </div>
 <div className="absolute -top-2 -right-2 w-7 h-7 rounded-full bg-indigo-600 text-white text-xs font-bold flex items-center justify-center shadow-md shadow-indigo-500/30 md:left-1/2 md:ml-5 md:right-auto">
 {i + 1}
 </div>
 <h3 className="font-semibold text-gray-900 mb-2">{step.title}</h3>
 <p className="text-sm text-gray-500 leading-relaxed max-w-xs mx-auto">{step.description}</p>
 </div>
 ))}
 </div>
 </div>
 </section>

 {/* Testimonial */}
 <section className="max-w-4xl mx-auto px-6 py-20">
 <div className="relative bg-white border border-gray-200 rounded-2xl p-8 sm:p-12 text-center">
 <Quote className="w-10 h-10 text-indigo-200 mx-auto mb-6" />
 <blockquote className="text-xl sm:text-2xl font-medium text-gray-900 leading-relaxed max-w-2xl mx-auto">
 &ldquo;Marshal replaced three separate tools for us. The AI search across our 2,000+ documents is like having a team member who remembers everything.&rdquo;
 </blockquote>
 <div className="mt-8 flex items-center justify-center gap-4">
 <div className="w-10 h-10 rounded-full bg-gradient-to-br from-indigo-400 to-purple-500 flex items-center justify-center text-white font-bold text-sm">
 SJ
 </div>
 <div className="text-left">
 <p className="text-sm font-semibold text-gray-900">Sarah Johnson</p>
 <p className="text-xs text-gray-500">VP of Engineering, TechScale Inc.</p>
 </div>
 </div>
 </div>
 </section>

 {/* CTA banner */}
 <section className="max-w-5xl mx-auto px-6 pb-20">
 <div className="relative overflow-hidden rounded-2xl bg-gradient-to-r from-indigo-600 via-indigo-700 to-purple-700 p-10 sm:p-14 text-center">
 <div className="absolute top-0 right-0 w-[300px] h-[300px] bg-white/5 rounded-full blur-3xl -translate-y-1/2 translate-x-1/4" />
 <div className="absolute bottom-0 left-0 w-[200px] h-[200px] bg-purple-400/10 rounded-full blur-2xl translate-y-1/2 -translate-x-1/4" />
 <div className="relative z-10">
 <Sparkles className="w-8 h-8 text-indigo-200 mx-auto mb-4" />
 <h2 className="text-2xl sm:text-3xl font-bold text-white mb-3">Ready to get started?</h2>
 <p className="text-indigo-200 mb-8 max-w-md mx-auto">Join teams using Marshal to streamline their work and unlock insights from their documents.</p>
 <div className="flex flex-col sm:flex-row items-center justify-center gap-4">
 <Link
 to="/login"
 className="w-full sm:w-auto inline-flex items-center justify-center gap-2 px-8 py-3.5 rounded-xl bg-white text-indigo-600 font-semibold text-sm hover:bg-indigo-50 transition-colors shadow-lg"
 >
 Sign in now
 <ArrowRight className="w-4 h-4" />
 </Link>
 <Link
 to="/contact"
 className="w-full sm:w-auto inline-flex items-center justify-center px-8 py-3.5 rounded-xl border border-white/30 text-white font-semibold text-sm hover:bg-white/10 transition-colors"
 >
 Contact sales
 </Link>
 </div>
 </div>
 </div>
 </section>

 {/* Trust badges */}
 <section className="max-w-5xl mx-auto px-6 pb-20">
 <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
 {[
 { icon: Shield, label: 'SOC 2 Compliant' },
 { icon: Clock, label: '99.9% Uptime' },
 { icon: Lock, label: '256-bit Encryption' },
 { icon: CheckCircle2, label: 'GDPR Ready' },
 ].map((badge) => (
 <div key={badge.label} className="flex items-center justify-center gap-2.5 py-4 px-3 rounded-xl bg-gray-50 border border-gray-200">
 <badge.icon className="w-4 h-4 text-gray-500" />
 <span className="text-xs font-medium text-gray-500">{badge.label}</span>
 </div>
 ))}
 </div>
 </section>

 <PublicFooter />
 </div>
 )
}
