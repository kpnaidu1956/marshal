import { ChevronLeft, ChevronRight, Calendar } from 'lucide-react'
import { format, addMonths, subMonths } from 'date-fns'

interface MonthFilterProps {
 selectedMonth: Date
 onMonthChange: (date: Date) => void
 compact?: boolean
}

export function MonthFilter({ selectedMonth, onMonthChange, compact = false }: MonthFilterProps) {
 const handlePrev = (e: React.MouseEvent) => {
 e.stopPropagation()
 onMonthChange(subMonths(selectedMonth, 1))
 }
 const handleNext = (e: React.MouseEvent) => {
 e.stopPropagation()
 onMonthChange(addMonths(selectedMonth, 1))
 }
 const handleToday = (e: React.MouseEvent) => {
 e.stopPropagation()
 onMonthChange(new Date())
 }

 const isCurrent = format(selectedMonth, 'yyyy-MM') === format(new Date(), 'yyyy-MM')

 if (compact) {
 return (
 <div className="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
 <button onClick={handlePrev} className="p-1 rounded hover:bg-muted transition-colors">
 <ChevronLeft className="h-3 w-3" />
 </button>
 <span className="text-xs font-medium min-w-[80px] text-center">
 {format(selectedMonth, 'MMM yyyy')}
 </span>
 <button onClick={handleNext} className="p-1 rounded hover:bg-muted transition-colors">
 <ChevronRight className="h-3 w-3" />
 </button>
 {!isCurrent && (
 <button onClick={handleToday} className="p-1 rounded hover:bg-muted transition-colors" title="Go to current month">
 <Calendar className="h-3 w-3" />
 </button>
 )}
 </div>
 )
 }

 return (
 <div className="flex items-center gap-2 bg-muted/50 rounded-lg px-2 py-1">
 <button onClick={handlePrev} className="p-1.5 rounded hover:bg-muted transition-colors">
 <ChevronLeft className="h-4 w-4" />
 </button>
 <span className="text-sm font-medium min-w-[100px] text-center">
 {format(selectedMonth, 'MMMM yyyy')}
 </span>
 <button onClick={handleNext} className="p-1.5 rounded hover:bg-muted transition-colors">
 <ChevronRight className="h-4 w-4" />
 </button>
 {!isCurrent && (
 <button onClick={handleToday} className="px-2 py-1 rounded text-xs hover:bg-muted transition-colors">
 Today
 </button>
 )}
 </div>
 )
}
