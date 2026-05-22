import type { ApiError } from '@/api/error'
import { apiErrorMessage } from '@/api/error'

function formatError(error: unknown): string {
 if (!error) return 'An unknown error occurred.'
 if (typeof error === 'string') return error
 if (typeof error === 'object' && 'type' in error) {
 return apiErrorMessage(error as ApiError)
 }
 if (error instanceof Error) return error.message
 return String(error)
}

export function ErrorAlert({ error }: { error: unknown }) {
 return (
 <div className="p-3 text-sm text-red-700 bg-red-50 border border-red-200 rounded-lg">
 {formatError(error)}
 </div>
 )
}
