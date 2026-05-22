import { ChevronLeft, ChevronRight } from 'lucide-react'
import { Button } from '@/components/ui/button'

interface PaginationControlsProps {
 page: number
 perPage: number
 total: number
 onPageChange: (page: number) => void
}

export function PaginationControls({ page, perPage, total, onPageChange }: PaginationControlsProps) {
 const totalPages = Math.ceil(total / perPage)
 if (totalPages <= 1) return null

 return (
 <div className="flex items-center justify-between pt-4">
 <span className="text-sm text-gray-500">
 Showing {(page - 1) * perPage + 1}–{Math.min(page * perPage, total)} of {total}
 </span>
 <div className="flex items-center gap-2">
 <Button
 variant="outline"
 size="sm"
 disabled={page <= 1}
 onClick={() => onPageChange(page - 1)}
 >
 <ChevronLeft className="w-4 h-4" />
 </Button>
 <span className="text-sm text-gray-600">
 Page {page} of {totalPages}
 </span>
 <Button
 variant="outline"
 size="sm"
 disabled={page >= totalPages}
 onClick={() => onPageChange(page + 1)}
 >
 <ChevronRight className="w-4 h-4" />
 </Button>
 </div>
 </div>
 )
}
