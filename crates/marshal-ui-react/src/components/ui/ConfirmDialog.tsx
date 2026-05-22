import { useState } from 'react'
import {
 AlertDialog,
 AlertDialogContent,
 AlertDialogHeader,
 AlertDialogTitle,
 AlertDialogDescription,
 AlertDialogFooter,
 AlertDialogCancel,
 AlertDialogAction,
} from '@/components/ui/alert-dialog'
import { cn } from '@/lib/utils'

interface ConfirmDialogProps {
 open: boolean
 onOpenChange: (open: boolean) => void
 title: string
 description: string
 confirmLabel?: string
 cancelLabel?: string
 variant?: 'danger' | 'warning' | 'default'
 onConfirm: () => void | Promise<void>
}

export function ConfirmDialog({
 open,
 onOpenChange,
 title,
 description,
 confirmLabel = 'Confirm',
 cancelLabel = 'Cancel',
 variant = 'default',
 onConfirm,
}: ConfirmDialogProps) {
 const [loading, setLoading] = useState(false)

 const handleConfirm = async () => {
 setLoading(true)
 try {
 await onConfirm()
 onOpenChange(false)
 } finally {
 setLoading(false)
 }
 }

 return (
 <AlertDialog open={open} onOpenChange={onOpenChange}>
 <AlertDialogContent>
 <AlertDialogHeader>
 <AlertDialogTitle>{title}</AlertDialogTitle>
 <AlertDialogDescription>{description}</AlertDialogDescription>
 </AlertDialogHeader>
 <AlertDialogFooter>
 <AlertDialogCancel disabled={loading}>{cancelLabel}</AlertDialogCancel>
 <AlertDialogAction
 onClick={handleConfirm}
 disabled={loading}
 className={cn(
 variant === 'danger' && 'bg-red-600 hover:bg-red-700 text-white',
 variant === 'warning' && 'bg-amber-600 hover:bg-amber-700 text-white',
 )}
 >
 {loading ? 'Processing...' : confirmLabel}
 </AlertDialogAction>
 </AlertDialogFooter>
 </AlertDialogContent>
 </AlertDialog>
 )
}
