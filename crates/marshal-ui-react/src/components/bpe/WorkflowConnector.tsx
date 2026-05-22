import { memo } from 'react'
import { Diamond } from 'lucide-react'
import type { LevelEdge } from './workflowLayout'

/** Simple vertical connector for 1:1 connections or the "add step" stem. */
export const SimpleConnector = memo(function SimpleConnector({ isApproval }: { isApproval?: boolean }) {
 return (
 <div className="flex flex-col items-center py-1">
 <div className="w-0.5 h-4 bg-gray-300" />
 {isApproval && (
 <Diamond className="w-4 h-4 text-amber-500 -my-0.5" />
 )}
 <div className="w-0.5 h-4 bg-gray-300" />
 <div className="w-0 h-0 border-l-[5px] border-l-transparent border-r-[5px] border-r-transparent border-t-[6px] border-t-gray-300" />
 </div>
 )
})

interface LevelConnectorProps {
 edges: LevelEdge[]
 parentCount: number
 childCount: number
}

/**
 * SVG connector between two graph levels.
 * Draws lines from parent column centers to child column centers.
 */
export const LevelConnector = memo(function LevelConnector({ edges, parentCount, childCount }: LevelConnectorProps) {
 if (edges.length === 0) return <div className="h-6" />

 const svgHeight = 48
 const padding = 8

 // Compute X positions as percentages
 function parentX(col: number) {
 return ((col + 0.5) / parentCount) * 100
 }
 function childX(col: number) {
 return ((col + 0.5) / childCount) * 100
 }

 // Check if this is a simple 1:1 pass-through
 const isSimple = edges.length === 1 && parentCount === 1 && childCount === 1

 return (
 <div className="relative w-full" style={{ height: svgHeight }}>
 <svg
 viewBox={`0 0 100 ${svgHeight}`}
 preserveAspectRatio="none"
 className="w-full h-full"
 style={{ overflow: 'visible' }}
 >
 {edges.map((edge, i) => {
 const x1 = parentX(edge.parentCol)
 const x2 = childX(edge.childCol)
 const y1 = padding
 const y2 = svgHeight - padding
 const midY = svgHeight / 2

 // Color based on condition
 let strokeColor = 'rgb(209 213 219)' // gray-300
 if (edge.condition === 'approved') strokeColor = 'rgb(34 197 94)' // green-500
 else if (edge.condition === 'rejected') strokeColor = 'rgb(239 68 68)' // red-500
 else if (edge.isApprovalParent) strokeColor = 'rgb(245 158 11)' // amber-500

 const strokeWidth = isSimple ? 0.8 : 0.6

 return (
 <g key={i}>
 {/* Path from parent to child */}
 {Math.abs(x1 - x2) < 0.5 ? (
 // Straight vertical line
 <line
 x1={x1}
 y1={y1}
 x2={x2}
 y2={y2}
 stroke={strokeColor}
 strokeWidth={strokeWidth}
 vectorEffect="non-scaling-stroke"
 />
 ) : (
 // Curved path for diagonal connections
 <path
 d={`M ${x1} ${y1} C ${x1} ${midY}, ${x2} ${midY}, ${x2} ${y2}`}
 fill="none"
 stroke={strokeColor}
 strokeWidth={strokeWidth}
 vectorEffect="non-scaling-stroke"
 />
 )}
 {/* Arrow at child end */}
 <polygon
 points={`${x2 - 1.2},${y2 - 3} ${x2 + 1.2},${y2 - 3} ${x2},${y2}`}
 fill={strokeColor}
 />
 </g>
 )
 })}
 </svg>

 {/* Condition labels */}
 {edges
 .filter((e) => e.condition && e.isApprovalParent)
 .map((edge, i) => {
 const x1 = parentX(edge.parentCol)
 const x2 = childX(edge.childCol)
 const labelX = (x1 + x2) / 2
 const isGreen = edge.condition === 'approved'
 const isRed = edge.condition === 'rejected'
 return (
 <div
 key={`label-${i}`}
 className="absolute text-[10px] font-medium -translate-x-1/2 whitespace-nowrap pointer-events-none"
 style={{
 left: `${labelX}%`,
 top: svgHeight / 2 - 8,
 color: isGreen ? 'rgb(22 163 74)' : isRed ? 'rgb(220 38 38)' : 'rgb(156 163 175)',
 }}
 >
 {edge.condition}
 </div>
 )
 })}

 {/* Approval diamond on fork */}
 {edges.length > 1 && edges.some((e) => e.isApprovalParent) && (
 <div
 className="absolute -translate-x-1/2 -translate-y-1/2 pointer-events-none"
 style={{
 left: `${parentX(edges[0].parentCol)}%`,
 top: padding + 2,
 }}
 >
 <Diamond className="w-3.5 h-3.5 text-amber-500" />
 </div>
 )}
 </div>
 )
})
