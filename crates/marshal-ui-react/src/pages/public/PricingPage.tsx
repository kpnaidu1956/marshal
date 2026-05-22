import { useState } from 'react'
import { Link } from 'react-router-dom'
import { Check, ChevronDown, Shield, Clock, Lock, CheckCircle2 } from 'lucide-react'
import { PublicNav } from '@/components/PublicNav'
import { PublicFooter } from '@/components/PublicFooter'

interface Plan {
 name: string
 price: string
 period: string
 description: string
 features: string[]
 cta: string
 ctaLink: string
 highlighted?: boolean
}

const plans: Plan[] = [
 {
 name: 'Starter',
 price: 'Free',
 period: '',
 description: 'For individuals and small teams getting started.',
 features: [
 'Up to 5 team members',
 '100 tasks & goals',
 '50 document uploads',
 'Basic RAG search',
 'Dashboard analytics',
 'Email support',
 ],
 cta: 'Get Started',
 ctaLink: '/login',
 },
 {
 name: 'Professional',
 price: '$29',
 period: '/user/mo',
 description: 'For growing teams that need full analytics and AI.',
 features: [
 'Unlimited team members',
 'Unlimited tasks & goals',
 '500 document uploads',
 'Advanced RAG with citations',
 'Team network analytics',
 'Performance dashboards',
 'Priority support',
 'API access',
 ],
 cta: 'Start Free Trial',
 ctaLink: '/login',
 highlighted: true,
 },
 {
 name: 'Enterprise',
 price: 'Custom',
 period: '',
 description: 'For organizations needing security and scale.',
 features: [
 'Everything in Professional',
 'Unlimited documents',
 'SSO / SAML authentication',
 'Custom embeddings model',
 'Dedicated infrastructure',
 'SLA guarantee',
 'Onboarding & training',
 'Audit logs',
 ],
 cta: 'Contact Sales',
 ctaLink: '/contact',
 },
]

const faqs = [
 {
 q: 'Can I try Marshal before committing?',
 a: 'Yes! The Starter plan is completely free with no credit card required. You can also start a 14-day free trial of the Professional plan.',
 },
 {
 q: 'What document formats are supported?',
 a: 'Marshal supports PDF, DOCX, TXT, Markdown, and more. Documents are automatically chunked and embedded for semantic search.',
 },
 {
 q: 'How does the AI search work?',
 a: 'Marshal uses retrieval-augmented generation (RAG) to find relevant chunks from your documents and synthesize answers with source citations.',
 },
 {
 q: 'Is my data secure?',
 a: 'All data is encrypted in transit and at rest. Enterprise plans include SSO, audit logs, and dedicated infrastructure options.',
 },
 {
 q: 'Can I switch plans later?',
 a: 'Absolutely. You can upgrade or downgrade at any time. When upgrading, you only pay the prorated difference. Downgrades take effect at the next billing cycle.',
 },
 {
 q: 'What kind of support do you offer?',
 a: 'Starter plans include email support. Professional gets priority support with faster response times. Enterprise includes a dedicated success manager and custom SLA.',
 },
]

export function PricingPage() {
 const [openFaq, setOpenFaq] = useState<number | null>(null)
 const [billingPeriod, setBillingPeriod] = useState<'monthly' | 'annual'>('monthly')

 return (
 <div className="min-h-screen bg-white">
 <PublicNav />

 {/* Hero */}
 <section className="relative overflow-hidden">
 <div className="absolute inset-0 bg-gradient-to-b from-indigo-50/50 via-white to-white" />
 <div className="relative max-w-4xl mx-auto px-6 pt-20 pb-12 text-center">
 <h1 className="text-4xl sm:text-5xl font-bold text-gray-900 tracking-tight">
 Simple, transparent pricing
 </h1>
 <p className="mt-4 text-lg text-gray-600 max-w-2xl mx-auto">
 Start free, scale as you grow. No hidden fees, no surprises.
 </p>

 {/* Billing toggle */}
 <div className="mt-8 inline-flex items-center gap-3 bg-gray-100 rounded-full p-1">
 <button
 onClick={() => setBillingPeriod('monthly')}
 className={`px-4 py-2 rounded-full text-sm font-medium transition-all ${
 billingPeriod === 'monthly'
 ? 'bg-white text-gray-900 shadow-sm'
 : 'text-gray-500 hover:text-gray-700'
 }`}
 >
 Monthly
 </button>
 <button
 onClick={() => setBillingPeriod('annual')}
 className={`px-4 py-2 rounded-full text-sm font-medium transition-all flex items-center gap-2 ${
 billingPeriod === 'annual'
 ? 'bg-white text-gray-900 shadow-sm'
 : 'text-gray-500 hover:text-gray-700'
 }`}
 >
 Annual
 <span className="text-xs px-1.5 py-0.5 rounded-full bg-emerald-100 text-emerald-700 font-semibold">-20%</span>
 </button>
 </div>
 </div>
 </section>

 {/* Pricing cards */}
 <section className="max-w-5xl mx-auto px-6 pb-20">
 <div className="grid grid-cols-1 md:grid-cols-3 gap-6 items-start">
 {plans.map((plan) => (
 <div
 key={plan.name}
 className={`rounded-2xl p-6 sm:p-8 border transition-all ${
 plan.highlighted
 ? 'bg-white border-transparent shadow-2xl shadow-indigo-200/50 ring-2 ring-indigo-500 relative md:scale-105 md:-my-4'
 : 'bg-white border-gray-200 hover:shadow-lg hover:border-gray-300'
 }`}
 >
 {plan.highlighted && (
 <div className="absolute -top-3.5 left-1/2 -translate-x-1/2 px-4 py-1 rounded-full bg-gradient-to-r from-indigo-600 to-purple-600 text-white text-xs font-semibold shadow-lg shadow-indigo-500/30">
 Most Popular
 </div>
 )}
 <h3 className="text-lg font-semibold text-gray-900">{plan.name}</h3>
 <p className="text-sm text-gray-500 mt-1 min-h-[40px]">{plan.description}</p>
 <div className="mt-5 mb-6">
 <span className="text-4xl font-bold text-gray-900">
 {plan.price === '$29' && billingPeriod === 'annual' ? '$23' : plan.price}
 </span>
 {plan.period && (
 <span className="text-sm text-gray-500 ml-1">{plan.period}</span>
 )}
 {plan.price === '$29' && billingPeriod === 'annual' && (
 <span className="ml-2 text-sm text-gray-500 line-through">$29</span>
 )}
 </div>
 <Link
 to={plan.ctaLink}
 className={`block text-center py-3 rounded-xl text-sm font-semibold transition-all ${
 plan.highlighted
 ? 'bg-gradient-to-r from-indigo-600 to-indigo-700 text-white hover:from-indigo-700 hover:to-indigo-800 shadow-md shadow-indigo-500/20 hover:shadow-indigo-500/30'
 : 'bg-gray-100 text-gray-900 hover:bg-gray-200'
 }`}
 >
 {plan.cta}
 </Link>
 <ul className="mt-6 space-y-3">
 {plan.features.map((f) => (
 <li key={f} className="flex items-start gap-2.5 text-sm text-gray-600">
 <Check className={`w-4 h-4 mt-0.5 flex-shrink-0 ${plan.highlighted ? 'text-indigo-500' : 'text-emerald-500'}`} />
 {f}
 </li>
 ))}
 </ul>
 </div>
 ))}
 </div>
 </section>

 {/* Trust badges */}
 <section className="max-w-4xl mx-auto px-6 pb-20">
 <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
 {[
 { icon: Shield, label: 'SOC 2 Compliant' },
 { icon: Clock, label: '99.9% Uptime' },
 { icon: CheckCircle2, label: 'GDPR Ready' },
 { icon: Lock, label: '256-bit Encryption' },
 ].map((badge) => (
 <div key={badge.label} className="flex items-center justify-center gap-2.5 py-4 px-3 rounded-xl bg-gray-50 border border-gray-200">
 <badge.icon className="w-4 h-4 text-gray-500" />
 <span className="text-xs font-medium text-gray-500">{badge.label}</span>
 </div>
 ))}
 </div>
 </section>

 {/* FAQ */}
 <section className="bg-gray-50 border-y border-gray-200/60">
 <div className="max-w-3xl mx-auto px-6 py-20">
 <h2 className="text-3xl font-bold text-gray-900 text-center mb-12 tracking-tight">Frequently asked questions</h2>
 <div className="space-y-3">
 {faqs.map((item, i) => (
 <div
 key={item.q}
 className="bg-white border border-gray-200 rounded-xl overflow-hidden transition-all"
 >
 <button
 onClick={() => setOpenFaq(openFaq === i ? null : i)}
 className="w-full flex items-center justify-between px-6 py-4 text-left"
 >
 <h3 className="font-medium text-gray-900 text-sm sm:text-base pr-4">{item.q}</h3>
 <ChevronDown className={`w-4 h-4 text-gray-400 flex-shrink-0 transition-transform duration-200 ${openFaq === i ? 'rotate-180' : ''}`} />
 </button>
 {openFaq === i && (
 <div className="px-6 pb-4">
 <p className="text-sm text-gray-500 leading-relaxed">{item.a}</p>
 </div>
 )}
 </div>
 ))}
 </div>
 </div>
 </section>

 <PublicFooter />
 </div>
 )
}
