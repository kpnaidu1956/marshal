import { Link } from 'react-router-dom'
import { Button } from '@/components/ui/button'
import { Home, ArrowLeft } from 'lucide-react'

export function NotFoundPage() {
 return (
 <div className="flex items-center justify-center h-full min-h-[60vh]">
 <div className="text-center max-w-md px-4">
 <p className="text-7xl font-bold text-primary mb-2">404</p>
 <h1 className="text-2xl font-bold text-foreground mb-2">Page not found</h1>
 <p className="text-sm text-muted-foreground mb-6">
 The page you're looking for doesn't exist or has been moved.
 </p>
 <div className="flex items-center justify-center gap-3">
 <Button variant="outline" onClick={() => window.history.back()}>
 <ArrowLeft className="w-4 h-4 mr-2" />
 Go back
 </Button>
 <Button asChild>
 <Link to="/">
 <Home className="w-4 h-4 mr-2" />
 Dashboard
 </Link>
 </Button>
 </div>
 </div>
 </div>
 )
}
