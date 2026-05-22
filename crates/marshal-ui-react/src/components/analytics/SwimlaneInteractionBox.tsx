import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { Badge } from '@/components/ui/badge';
import { format } from 'date-fns';
import type { InteractionType } from '@/types/analytics';

interface SwimlaneInteractionBoxProps {
 x: number;
 y: number;
 width: number;
 height: number;
 content: string;
 interactionType: InteractionType | 'phase_change';
 sentiment: number;
 timestamp: Date;
 isPhaseChange?: boolean;
 phaseName?: string;
 onHover?: (hovered: boolean) => void;
}

function truncateText(text: string, maxLength: number): string {
 if (text.length <= maxLength) return text;
 return text.slice(0, maxLength - 3) + '...';
}

function formatInteractionType(type: string): string {
 return type
 .split('_')
 .map(word => word.charAt(0).toUpperCase() + word.slice(1))
 .join(' ');
}

function getSentimentColor(sentiment: number): string {
 if (sentiment >= 0.3) return 'hsl(142, 71%, 45%)';
 if (sentiment <= -0.3) return 'hsl(0, 84%, 60%)';
 return 'hsl(48, 96%, 53%)';
}

function getSentimentLabel(sentiment: number): string {
 if (sentiment >= 0.3) return 'Kudos';
 if (sentiment <= -0.3) return 'Issue';
 return 'Neutral';
}

export function SwimlaneInteractionBox({
 x,
 y,
 width,
 height,
 content,
 interactionType,
 sentiment,
 timestamp,
 isPhaseChange = false,
 phaseName,
 onHover,
}: SwimlaneInteractionBoxProps) {
 const sentimentColor = getSentimentColor(sentiment);
 const displayText = truncateText(content, 40);
 const sentimentBandHeight = 4;

 return (
 <Tooltip>
 <TooltipTrigger asChild>
 <g
 onMouseEnter={() => onHover?.(true)}
 onMouseLeave={() => onHover?.(false)}
 className="cursor-pointer"
 >
 <rect
 x={x}
 y={y}
 width={width}
 height={height - sentimentBandHeight}
 rx={4}
 ry={4}
 fill="hsl(var(--primary))"
 opacity={0.85}
 className="transition-all hover:opacity-100"
 />
 <rect
 x={x}
 y={y + height - sentimentBandHeight}
 width={width}
 height={sentimentBandHeight}
 rx={0}
 ry={0}
 fill={sentimentColor}
 />
 <rect
 x={x}
 y={y + height - sentimentBandHeight - 2}
 width={4}
 height={sentimentBandHeight + 2}
 fill={sentimentColor}
 />
 <rect
 x={x + width - 4}
 y={y + height - sentimentBandHeight - 2}
 width={4}
 height={sentimentBandHeight + 2}
 fill={sentimentColor}
 />
 <foreignObject x={x + 6} y={y + 6} width={width - 12} height={height - sentimentBandHeight - 10}>
 <div
 className="text-xs text-primary-foreground font-medium leading-tight overflow-hidden"
 style={{
 display: '-webkit-box',
 WebkitLineClamp: 2,
 WebkitBoxOrient: 'vertical',
 }}
 >
 {isPhaseChange && phaseName && (
 <span className="opacity-70 mr-1">▸</span>
 )}
 {displayText}
 </div>
 </foreignObject>
 </g>
 </TooltipTrigger>
 <TooltipContent className="max-w-sm">
 <div className="space-y-2">
 <div className="flex items-center gap-2">
 <Badge variant="outline" className="text-xs">
 {isPhaseChange ? `Phase: ${phaseName}` : formatInteractionType(interactionType)}
 </Badge>
 <span
 className="text-xs font-medium"
 style={{ color: sentimentColor }}
 >
 {getSentimentLabel(sentiment)}
 </span>
 </div>
 <p className="text-sm">{content}</p>
 <p className="text-xs text-muted-foreground">
 {format(timestamp, 'MMM d, yyyy HH:mm')}
 </p>
 </div>
 </TooltipContent>
 </Tooltip>
 );
}
