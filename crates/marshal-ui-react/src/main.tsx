import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { Toaster } from 'sonner'
import { App } from './App'
import './index.css'

// Ensure light mode only (remove any stale dark class)
document.documentElement.classList.remove('dark')

const queryClient = new QueryClient({
 defaultOptions: {
 queries: {
 staleTime: 30_000,
 retry: 1,
 },
 },
})

createRoot(document.getElementById('root')!).render(
 <StrictMode>
 <BrowserRouter basename="/marshal">
 <QueryClientProvider client={queryClient}>
 <App />
 <Toaster position="bottom-right" richColors />
 </QueryClientProvider>
 </BrowserRouter>
 </StrictMode>,
)
// cache-bust
