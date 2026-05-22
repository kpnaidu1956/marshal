export type ApiError =
 | { type: 'network'; message: string }
 | { type: 'unauthorized' }
 | { type: 'forbidden'; message: string }
 | { type: 'not_found' }
 | { type: 'bad_request'; message: string }
 | { type: 'server_error'; message: string }
 | { type: 'parse_error'; message: string }

export function statusToError(status: number, body: string): ApiError {
 switch (status) {
 case 401:
 return { type: 'unauthorized' }
 case 403:
 return { type: 'forbidden', message: body }
 case 404:
 return { type: 'not_found' }
 case 400:
 return { type: 'bad_request', message: body }
 default:
 if (status >= 500) return { type: 'server_error', message: body }
 return { type: 'network', message: `HTTP ${status}: ${body}` }
 }
}

export function apiErrorMessage(err: ApiError): string {
 switch (err.type) {
 case 'unauthorized':
 return 'Unauthorized — please log in again.'
 case 'forbidden':
 return 'You do not have permission to perform this action.'
 case 'not_found':
 return 'Not found.'
 case 'bad_request':
 return `Bad request: ${err.message}`
 case 'server_error':
 return `Server error: ${err.message}`
 case 'parse_error':
 return `Failed to parse response: ${err.message}`
 case 'network':
 return err.message
 }
}
