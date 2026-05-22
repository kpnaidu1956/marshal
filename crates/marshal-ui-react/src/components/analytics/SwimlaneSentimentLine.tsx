interface SwimlaneSentimentLineProps {
 x1: number;
 y1: number;
 x2: number;
 y2: number;
 animated?: boolean;
 rightAngle?: boolean;
}

export function SwimlaneSentimentLine({
 x1,
 y1,
 x2,
 y2,
 animated = false,
 rightAngle = false,
}: SwimlaneSentimentLineProps) {
 let pathD: string;

 if (rightAngle) {
 const midX = (x1 + x2) / 2;
 if (y1 === y2) {
 pathD = `M ${x1} ${y1} L ${x2} ${y2}`;
 } else {
 pathD = `M ${x1} ${y1} L ${midX} ${y1} L ${midX} ${y2} L ${x2} ${y2}`;
 }
 } else {
 const midX = (x1 + x2) / 2;
 const midY = (y1 + y2) / 2;
 const dy = y2 - y1;
 const curveOffset = Math.min(Math.abs(dy) * 0.3, 30);
 const controlX = midX + (dy > 0 ? curveOffset : -curveOffset);
 pathD = `M ${x1} ${y1} Q ${controlX} ${midY} ${x2} ${y2}`;
 }

 return (
 <g>
 <path
 d={pathD}
 fill="none"
 stroke="hsl(var(--foreground))"
 strokeWidth={1.5}
 strokeLinecap="round"
 strokeLinejoin="round"
 opacity={0.6}
 className={animated ? 'animate-pulse' : ''}
 />
 <circle
 cx={x2}
 cy={y2}
 r={3}
 fill="hsl(var(--foreground))"
 opacity={0.7}
 />
 </g>
 );
}
