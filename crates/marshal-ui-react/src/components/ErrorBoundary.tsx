import { Component, type ErrorInfo, type ReactNode } from 'react'
import { AlertTriangle } from 'lucide-react'

interface Props {
 children: ReactNode
 fallback?: ReactNode
}

interface State {
 hasError: boolean
 error: Error | null
}

export class ErrorBoundary extends Component<Props, State> {
 constructor(props: Props) {
 super(props)
 this.state = { hasError: false, error: null }
 }

 static getDerivedStateFromError(error: Error): State {
 return { hasError: true, error }
 }

 componentDidCatch(error: Error, info: ErrorInfo) {
 console.error('ErrorBoundary caught:', error, info)
 }

 render() {
 if (this.state.hasError) {
 if (this.props.fallback) return this.props.fallback
 return (
 <div className="flex items-center justify-center h-64">
 <div className="text-center max-w-md">
 <AlertTriangle className="w-8 h-8 text-amber-500 mx-auto mb-3" />
 <h2 className="text-lg font-semibold text-gray-900 mb-1">Something went wrong</h2>
 <p className="text-sm text-gray-500 mb-4">
 {this.state.error?.message || 'An unexpected error occurred.'}
 </p>
 <button
 onClick={() => this.setState({ hasError: false, error: null })}
 className="px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors"
 >
 Try again
 </button>
 </div>
 </div>
 )
 }
 return this.props.children
 }
}
