import { useMemo, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { ScrollArea, ScrollBar } from '@/components/ui/scroll-area';
import { TooltipProvider } from '@/components/ui/tooltip';
import { format, parseISO, differenceInMinutes } from 'date-fns';
import type { InteractionClassification, WorkflowTimeline, InteractionType } from '@/types/analytics';
import { SwimlaneInteractionBox } from './SwimlaneInteractionBox';
import { SwimlaneSentimentLine } from './SwimlaneSentimentLine';
import { formatDuration } from '@/lib/analyticsApi';

interface SwimlaneTimelineProps {
 interactions: InteractionClassification[];
 workflowTimeline?: WorkflowTimeline | null;
 totalDurationHours?: number | null;
 participantNames?: Record<string, string>;
}

interface ParticipantLane {
 userId: string;
 userName: string;
 y: number;
}

interface UnifiedEvent {
 id: string;
 type: 'interaction' | 'phase_change';
 senderId: string;
 senderName: string;
 content: string;
 interactionType: InteractionType | 'phase_change';
 sentiment: number;
 timestamp: Date;
 phaseName?: string;
}

interface PositionedEvent {
 event: UnifiedEvent;
 x: number;
 laneIndex: number;
 elapsedFromPrev?: string;
}

const LANE_HEIGHT = 72;
const LANE_PADDING = 12;
const BOX_WIDTH = 140;
const BOX_HEIGHT = 50;
const BOX_GAP = 40;
const LABEL_WIDTH = 120;
const MIN_TIMELINE_WIDTH = 1200;

function toDate(value: unknown): Date | null {
 if (typeof value === 'string') {
 const trimmed = value.trim();
 if (!trimmed) return null;
 const d = parseISO(trimmed);
 return Number.isNaN(d.getTime()) ? null : d;
 }
 if (value instanceof Date) {
 return Number.isNaN(value.getTime()) ? null : value;
 }
 return null;
}

function getInitials(name: string): string {
 return name
 .split(' ')
 .map(n => n[0])
 .join('')
 .toUpperCase()
 .slice(0, 2);
}

export function SwimlaneTimeline({
 interactions,
 workflowTimeline,
 totalDurationHours,
 participantNames = {},
}: SwimlaneTimelineProps) {
 const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);

 const safeInteractions = Array.isArray(interactions) ? interactions : [];

 const { lanes, positionedEvents, timelineWidth, startDate, endDate } = useMemo(() => {
 const allEvents: UnifiedEvent[] = [];

 safeInteractions.forEach(i => {
 const timestamp = toDate(i.original_created_at);
 if (!timestamp) return;

 const name = participantNames[i.sender_id] || 'Unknown';
 allEvents.push({
 id: i.id,
 type: 'interaction',
 senderId: i.sender_id,
 senderName: name,
 content: i.content,
 interactionType: i.interaction_type,
 sentiment: i.sentiment,
 timestamp,
 });
 });

 /* Phase change events hidden — uncomment to restore
 if (workflowTimeline?.phases) {
 workflowTimeline.phases.forEach((phase, index) => {
 const timestamp = toDate(phase.start);
 if (!timestamp) return;

 const actorId = phase.participants?.[0] || 'system';
 const actorName =
 actorId === 'system'
 ? 'System'
 : participantNames[actorId] || 'Unknown';

 allEvents.push({
 id: `phase-${index}-${phase.name}`,
 type: 'phase_change',
 senderId: actorId,
 senderName: actorName,
 content: `Phase: ${phase.name.replace('_', ' ')}`,
 interactionType: 'phase_change',
 sentiment: 0,
 timestamp,
 phaseName: phase.name,
 });
 });
 }
 */

 if (allEvents.length === 0) {
 return { lanes: [], positionedEvents: [], timelineWidth: MIN_TIMELINE_WIDTH, startDate: null, endDate: null };
 }

 const sorted = allEvents.sort((a, b) => a.timestamp.getTime() - b.timestamp.getTime());

 const participantMap = new Map<string, string>();
 sorted.forEach(e => {
 if (!participantMap.has(e.senderId)) {
 participantMap.set(e.senderId, e.senderName);
 }
 });

 const lanesList: ParticipantLane[] = Array.from(participantMap.entries()).map(
 ([userId, userName], index) => ({
 userId,
 userName,
 y: index * LANE_HEIGHT + LANE_PADDING,
 })
 );

 const laneIndexMap = new Map<string, number>();
 lanesList.forEach((lane, index) => {
 laneIndexMap.set(lane.userId, index);
 });

 const start = sorted[0].timestamp;
 const end = sorted[sorted.length - 1].timestamp;

 const positioned: PositionedEvent[] = sorted.map((event, index) => {
 const x = LABEL_WIDTH + index * (BOX_WIDTH + BOX_GAP);
 const laneIndex = laneIndexMap.get(event.senderId) ?? 0;

 let elapsedFromPrev: string | undefined;
 if (index > 0) {
 const prevTimestamp = sorted[index - 1].timestamp;
 const minutesDiff = differenceInMinutes(event.timestamp, prevTimestamp);
 if (minutesDiff < 60) {
 elapsedFromPrev = `${minutesDiff}m`;
 } else if (minutesDiff < 1440) {
 const hours = Math.floor(minutesDiff / 60);
 const mins = minutesDiff % 60;
 elapsedFromPrev = mins > 0 ? `${hours}h ${mins}m` : `${hours}h`;
 } else {
 const days = Math.floor(minutesDiff / 1440);
 const hours = Math.floor((minutesDiff % 1440) / 60);
 elapsedFromPrev = hours > 0 ? `${days}d ${hours}h` : `${days}d`;
 }
 }

 return {
 event,
 x,
 laneIndex,
 elapsedFromPrev,
 };
 });

 const calculatedWidth = Math.max(
 LABEL_WIDTH + positioned.length * (BOX_WIDTH + BOX_GAP) + 50,
 MIN_TIMELINE_WIDTH
 );

 return {
 lanes: lanesList,
 positionedEvents: positioned,
 timelineWidth: calculatedWidth,
 startDate: start,
 endDate: end,
 };
 }, [safeInteractions, workflowTimeline, participantNames]);

 if (lanes.length === 0) {
 return (
 <Card>
 <CardHeader>
 <CardTitle className="text-base">Workflow Timeline</CardTitle>
 </CardHeader>
 <CardContent className="flex items-center justify-center h-[200px] text-muted-foreground">
 No timeline data available
 </CardContent>
 </Card>
 );
 }

 const svgHeight = lanes.length * LANE_HEIGHT + LANE_PADDING * 2;

 return (
 <TooltipProvider>
 <Card>
 <CardHeader className="pb-2">
 <div className="flex items-center justify-between">
 <CardTitle className="text-base">Workflow Timeline</CardTitle>
 <div className="flex items-center gap-4 text-sm text-muted-foreground">
 <span>{positionedEvents.length} events</span>
 <span>{lanes.length} participants</span>
 {totalDurationHours && (
 <span>{formatDuration(totalDurationHours)} total</span>
 )}
 </div>
 </div>
 <div className="flex items-center gap-4 mt-2">
 <span className="text-xs text-muted-foreground">Interaction:</span>
 <div className="flex items-center gap-1">
 <div className="w-3 h-3 rounded-sm" style={{ backgroundColor: 'hsl(142, 71%, 45%)' }} />
 <span className="text-xs">Kudos</span>
 </div>
 <div className="flex items-center gap-1">
 <div className="w-3 h-3 rounded-sm" style={{ backgroundColor: 'hsl(48, 96%, 53%)' }} />
 <span className="text-xs">Neutral</span>
 </div>
 <div className="flex items-center gap-1">
 <div className="w-3 h-3 rounded-sm" style={{ backgroundColor: 'hsl(0, 84%, 60%)' }} />
 <span className="text-xs">Issue</span>
 </div>
 </div>
 </CardHeader>
 <CardContent>
 <ScrollArea className="w-full">
 <div style={{ width: timelineWidth, minWidth: '100%' }}>
 {startDate && endDate && (
 <div className="flex items-center mb-2 text-xs text-muted-foreground" style={{ paddingLeft: LABEL_WIDTH }}>
 <span>{format(startDate, 'MMM d, HH:mm')}</span>
 <div className="flex-1" />
 <span>{format(endDate, 'MMM d, HH:mm')}</span>
 </div>
 )}

 <svg width={timelineWidth} height={svgHeight} className="overflow-visible">
 {lanes.map((lane, index) => (
 <g key={lane.userId}>
 <rect
 x={0}
 y={lane.y}
 width={timelineWidth}
 height={LANE_HEIGHT - 8}
 fill={index % 2 === 0 ? 'hsl(var(--muted) / 0.3)' : 'transparent'}
 rx={4}
 />
 <foreignObject x={8} y={lane.y + 8} width={LABEL_WIDTH - 16} height={LANE_HEIGHT - 16}>
 <div className="flex items-center gap-2 h-full">
 <div className="w-8 h-8 rounded-full bg-primary/20 flex items-center justify-center text-xs font-bold text-primary">
 {getInitials(lane.userName)}
 </div>
 <div className="flex-1 min-w-0">
 <p className="text-sm font-medium truncate">{lane.userName}</p>
 </div>
 </div>
 </foreignObject>
 </g>
 ))}

 {positionedEvents.slice(1).map((pe, index) => {
 const prevPe = positionedEvents[index];
 const prevY = lanes[prevPe.laneIndex].y + LANE_HEIGHT / 2;
 const currY = lanes[pe.laneIndex].y + LANE_HEIGHT / 2;

 return (
 <SwimlaneSentimentLine
 key={`line-${index}`}
 x1={prevPe.x + BOX_WIDTH}
 y1={prevY}
 x2={pe.x}
 y2={currY}
 animated={hoveredIndex === index || hoveredIndex === index + 1}
 rightAngle
 />
 );
 })}

 {positionedEvents.map((pe, index) => {
 if (!pe.elapsedFromPrev) return null;
 const boxY = lanes[pe.laneIndex].y + (LANE_HEIGHT - BOX_HEIGHT) / 2;
 return (
 <text
 key={`elapsed-${index}`}
 x={pe.x + BOX_WIDTH / 2}
 y={boxY + BOX_HEIGHT + 14}
 textAnchor="middle"
 className="text-[10px] fill-muted-foreground font-medium"
 >
 +{pe.elapsedFromPrev}
 </text>
 );
 })}

 {positionedEvents.map((pe, index) => (
 <SwimlaneInteractionBox
 key={pe.event.id}
 x={pe.x}
 y={lanes[pe.laneIndex].y + (LANE_HEIGHT - BOX_HEIGHT) / 2}
 width={BOX_WIDTH}
 height={BOX_HEIGHT}
 content={pe.event.content}
 interactionType={pe.event.interactionType}
 sentiment={pe.event.sentiment}
 timestamp={pe.event.timestamp}
 isPhaseChange={pe.event.type === 'phase_change'}
 phaseName={pe.event.phaseName}
 onHover={(hovered) => setHoveredIndex(hovered ? index : null)}
 />
 ))}
 </svg>
 </div>
 <ScrollBar orientation="horizontal" />
 </ScrollArea>
 </CardContent>
 </Card>
 </TooltipProvider>
 );
}
