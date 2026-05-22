import { useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { Mail, MessageSquare, MapPin, Clock, CheckCircle2, ArrowRight } from 'lucide-react'
import { PublicNav } from '@/components/PublicNav'
import { PublicFooter } from '@/components/PublicFooter'

const contactCards = [
 {
 icon: Mail,
 title: 'Email',
 detail: 'support@example.com',
 subtitle: 'We reply within 24 hours',
 },
 {
 icon: MessageSquare,
 title: 'Live Chat',
 detail: 'Chat with our team',
 subtitle: 'Mon-Fri, 9am-5pm PST',
 },
 {
 icon: MapPin,
 title: 'Office',
 detail: 'San Francisco, CA',
 subtitle: 'United States',
 },
 {
 icon: Clock,
 title: 'Response Time',
 detail: 'Under 4 hours',
 subtitle: 'For priority support',
 },
]

const subjectOptions = [
 'General Inquiry',
 'Sales',
 'Support',
 'Partnership',
]

export function ContactPage() {
 const [name, setName] = useState('')
 const [email, setEmail] = useState('')
 const [subject, setSubject] = useState(subjectOptions[0])
 const [message, setMessage] = useState('')
 const [submitted, setSubmitted] = useState(false)

 const handleSubmit = (e: FormEvent) => {
 e.preventDefault()
 setSubmitted(true)
 }

 return (
 <div className="min-h-screen bg-white">
 <PublicNav />

 {/* Hero */}
 <section className="relative overflow-hidden">
 <div className="absolute inset-0 bg-gradient-to-b from-indigo-50/50 via-white to-white" />
 <div className="relative max-w-4xl mx-auto px-6 pt-20 pb-12 text-center">
 <h1 className="text-4xl sm:text-5xl font-bold text-gray-900 tracking-tight">
 Let&rsquo;s talk
 </h1>
 <p className="mt-4 text-lg text-gray-600 max-w-2xl mx-auto">
 Have questions about Marshal? We&rsquo;d love to hear from you. Our team is ready to help you get started.
 </p>
 </div>
 </section>

 {/* Contact info cards */}
 <section className="max-w-5xl mx-auto px-6 pb-12">
 <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
 {contactCards.map((card) => (
 <div
 key={card.title}
 className="bg-white border border-gray-200 rounded-xl p-5 hover:shadow-lg hover:border-indigo-200 transition-all duration-300"
 >
 <div className="w-10 h-10 rounded-lg bg-indigo-50 flex items-center justify-center mb-3">
 <card.icon className="w-5 h-5 text-indigo-500" />
 </div>
 <h3 className="font-semibold text-gray-900 text-sm">{card.title}</h3>
 <p className="text-sm text-gray-700 mt-1">{card.detail}</p>
 <p className="text-xs text-gray-500 mt-0.5">{card.subtitle}</p>
 </div>
 ))}
 </div>
 </section>

 {/* Form section */}
 <section className="max-w-5xl mx-auto px-6 pb-20">
 <div className="grid grid-cols-1 lg:grid-cols-5 gap-10">
 {/* Left: context */}
 <div className="lg:col-span-2 flex flex-col justify-center">
 <h2 className="text-2xl font-bold text-gray-900 mb-4">Send us a message</h2>
 <p className="text-gray-600 leading-relaxed mb-6">
 Whether you have a question about features, pricing, need a demo, or anything else, our team is ready to answer all your questions.
 </p>
 <div className="space-y-4">
 {[
 'Personalized onboarding for your team',
 'Custom pricing for enterprise needs',
 'Technical integration support',
 ].map((item) => (
 <div key={item} className="flex items-center gap-3">
 <CheckCircle2 className="w-4 h-4 text-indigo-500 flex-shrink-0" />
 <span className="text-sm text-gray-600">{item}</span>
 </div>
 ))}
 </div>
 </div>

 {/* Right: form */}
 <div className="lg:col-span-3">
 <div className="bg-white border border-gray-200 rounded-2xl p-6 sm:p-8 shadow-xl shadow-gray-200/40">
 {submitted ? (
 <div className="text-center py-14">
 <div className="relative inline-flex items-center justify-center mb-6">
 <div className="absolute inset-0 w-16 h-16 rounded-full bg-emerald-100 animate-ping opacity-20" />
 <div className="relative w-16 h-16 rounded-full bg-emerald-100 flex items-center justify-center">
 <CheckCircle2 className="w-8 h-8 text-emerald-600" />
 </div>
 </div>
 <h2 className="text-2xl font-bold text-gray-900 mb-3">Message sent!</h2>
 <p className="text-gray-500 mb-6 max-w-sm mx-auto">
 Thanks for reaching out. We&rsquo;ll get back to you within 24 hours.
 </p>
 <button
 onClick={() => {
 setSubmitted(false)
 setName('')
 setEmail('')
 setSubject(subjectOptions[0])
 setMessage('')
 }}
 className="text-sm text-indigo-600 font-medium hover:underline"
 >
 Send another message
 </button>
 </div>
 ) : (
 <form onSubmit={handleSubmit} className="space-y-5">
 <div className="grid grid-cols-1 sm:grid-cols-2 gap-5">
 <div>
 <label htmlFor="contact-name" className="block text-sm font-medium text-gray-700 mb-1.5">
 Name
 </label>
 <input
 id="contact-name"
 type="text"
 value={name}
 onChange={(e) => setName(e.target.value)}
 required
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 bg-white text-gray-900 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent placeholder-gray-400 transition"
 placeholder="Your name"
 />
 </div>
 <div>
 <label htmlFor="contact-email" className="block text-sm font-medium text-gray-700 mb-1.5">
 Email
 </label>
 <input
 id="contact-email"
 type="email"
 value={email}
 onChange={(e) => setEmail(e.target.value)}
 required
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 bg-white text-gray-900 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent placeholder-gray-400 transition"
 placeholder="you@example.com"
 />
 </div>
 </div>

 <div>
 <label htmlFor="contact-subject" className="block text-sm font-medium text-gray-700 mb-1.5">
 Subject
 </label>
 <select
 id="contact-subject"
 value={subject}
 onChange={(e) => setSubject(e.target.value)}
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 bg-white text-gray-900 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent transition appearance-none"
 >
 {subjectOptions.map((opt) => (
 <option key={opt} value={opt}>{opt}</option>
 ))}
 </select>
 </div>

 <div>
 <label htmlFor="contact-message" className="block text-sm font-medium text-gray-700 mb-1.5">
 Message
 </label>
 <textarea
 id="contact-message"
 value={message}
 onChange={(e) => setMessage(e.target.value)}
 required
 rows={5}
 className="w-full px-3.5 py-2.5 rounded-lg border border-gray-300 bg-white text-gray-900 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent placeholder-gray-400 transition resize-none"
 placeholder="Tell us how we can help..."
 />
 </div>

 <button
 type="submit"
 className="w-full py-3 rounded-xl bg-gradient-to-r from-indigo-600 to-indigo-700 text-white text-sm font-semibold hover:from-indigo-700 hover:to-indigo-800 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 shadow-md shadow-indigo-500/20 transition-all flex items-center justify-center gap-2"
 >
 Send Message
 <ArrowRight className="w-4 h-4" />
 </button>
 </form>
 )}
 </div>
 </div>
 </div>
 </section>

 {/* Direct email CTA */}
 <section className="max-w-3xl mx-auto px-6 pb-20 text-center">
 <div className="bg-gray-50 border border-gray-200/60 rounded-2xl p-8">
 <Mail className="w-8 h-8 text-gray-500 mx-auto mb-4" />
 <h3 className="text-lg font-semibold text-gray-900 mb-2">Prefer to email directly?</h3>
 <p className="text-sm text-gray-500 mb-4">
 Drop us a line at{' '}
 <a href="mailto:support@example.com" className="text-indigo-600 font-medium hover:underline">
 support@example.com
 </a>
 </p>
 <Link
 to="/demo"
 className="inline-flex items-center gap-2 text-sm text-indigo-600 font-medium hover:underline"
 >
 Or explore the product first
 <ArrowRight className="w-3.5 h-3.5" />
 </Link>
 </div>
 </section>

 <PublicFooter />
 </div>
 )
}
