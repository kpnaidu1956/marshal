# Frontend Integration Guide

## Overview

This guide explains how to integrate the Analytics API with a frontend application and create visualizations for workflow intelligence data.

## TypeScript Interfaces

```typescript
// ========================================
// Enums
// ========================================

type InteractionType =
  | 'request_clarification' | 'request_resources' | 'direction'
  | 'suggestion' | 'request_approval' | 'status_update'
  | 'acknowledgment' | 'escalation' | 'blocker'
  | 'question' | 'answer' | 'assignment'
  | 'feedback' | 'recognition' | 'other';

type UrgencyLevel = 'low' | 'medium' | 'high' | 'critical';

type PatternType = 'success' | 'failure' | 'bottleneck' | 'efficiency';

type RecommendationType = 'process' | 'communication' | 'resource' | 'timing';

type JobStatus = 'pending' | 'processing' | 'completed' | 'failed';

type RecommendationStatus = 'pending' | 'accepted' | 'rejected' | 'implemented';

// ========================================
// Core Types
// ========================================

interface WorkflowPhase {
  name: string;
  start: string;  // ISO 8601 datetime
  end: string | null;
  interaction_count: number;
  participants: string[];
}

interface TimelineEvent {
  timestamp: string;
  event_type: string;
  description: string;
  actor_id: string;
  actor_name?: string;
  interaction_id?: string;
  metadata?: Record<string, any>;
}

interface WorkflowBottleneck {
  bottleneck_type: string;
  duration_hours: number;
  description: string;
  start: string;
  end: string;
  caused_by?: string;
}

interface WorkflowTimeline {
  id: string;
  organization_id: string;
  entity_type: 'task' | 'goal';
  entity_id: string;
  total_interactions: number;
  total_participants: number;
  total_duration_hours: number | null;
  phases: WorkflowPhase[];
  key_events: TimelineEvent[];
  bottlenecks: WorkflowBottleneck[];
  status: string;
  opened_at: string;
  closed_at: string | null;
  last_analyzed_at: string;
}

interface ExtractedEntities {
  mentioned_users: string[];
  mentioned_deadlines: string[];
  action_items: string[];
  blockers: string[];
  resources: string[];
}

interface InteractionClassification {
  id: string;
  organization_id: string;
  source_type: 'task_comment' | 'goal_comment' | 'message' | 'activity_log';
  source_id: string;
  task_id?: string;
  goal_id?: string;
  sender_id: string;
  content: string;
  interaction_type: InteractionType;
  secondary_types: InteractionType[];
  confidence_score: number;
  sentiment: number;  // -1.0 to 1.0
  urgency_level: UrgencyLevel;
  entities: ExtractedEntities;
  references_interaction_id?: string;
  original_created_at: string;
  classified_at: string;
}

interface WorkflowPattern {
  id: string;
  organization_id: string;
  pattern_type: PatternType;
  pattern_name: string;
  description: string;
  criteria: Record<string, any>;
  occurrence_count: number;
  success_correlation: number | null;
  avg_time_impact_hours: number | null;
  confidence_score: number;
  examples: string[];
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

interface EfficiencyRecommendation {
  id: string;
  organization_id: string;
  target_type: 'task' | 'goal' | 'team' | 'organization';
  target_id?: string;
  recommendation_type: RecommendationType;
  title: string;
  description: string;
  suggested_actions: string[];
  based_on_patterns: string[];
  evidence: Record<string, any>;
  priority: 'low' | 'medium' | 'high';
  estimated_time_savings_hours: number | null;
  status: RecommendationStatus;
  user_feedback?: string;
  generated_at: string;
}

interface AnalysisJob {
  job_id: string;
  status: JobStatus;
  progress_percent: number;
  current_stage: string;
  entity_type: 'task' | 'goal';
  entity_id: string;
  started_at: string;
  completed_at: string | null;
  error_message: string | null;
}

// ========================================
// API Request Types
// ========================================

interface AnalysisRequest {
  organization_id: string;
}

interface SearchInteractionsRequest {
  organization_id: string;
  interaction_types?: InteractionType[];
  urgency_levels?: UrgencyLevel[];
  task_id?: string;
  goal_id?: string;
  sender_id?: string;
  from_date?: string;
  to_date?: string;
  content_search?: string;
  limit?: number;
}

interface PatternLearnRequest {
  organization_id: string;
  min_occurrences?: number;
  include_completed_only?: boolean;
}

interface FeedbackRequest {
  organization_id: string;
  status: RecommendationStatus;
  feedback?: string;
}
```

---

## API Client

```typescript
// analytics-client.ts

const API_BASE = '/api/analytics';

class AnalyticsApiError extends Error {
  constructor(public code: string, message: string) {
    super(message);
    this.name = 'AnalyticsApiError';
  }
}

async function handleResponse<T>(response: Response): Promise<T> {
  if (!response.ok) {
    const error = await response.json().catch(() => ({ error: 'Unknown error' }));
    throw new AnalyticsApiError(error.code || 'UNKNOWN', error.error);
  }
  return response.json();
}

export const analyticsApi = {
  // ========================================
  // Analysis
  // ========================================

  analyzeTask: (taskId: string, orgId: string): Promise<AnalysisJob> =>
    fetch(`${API_BASE}/analysis/task/${taskId}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ organization_id: orgId })
    }).then(handleResponse),

  analyzeGoal: (goalId: string, orgId: string): Promise<AnalysisJob> =>
    fetch(`${API_BASE}/analysis/goal/${goalId}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ organization_id: orgId })
    }).then(handleResponse),

  getJobStatus: (jobId: string): Promise<AnalysisJob> =>
    fetch(`${API_BASE}/analysis/job/${jobId}`).then(handleResponse),

  // Poll until complete
  waitForAnalysis: async (jobId: string, pollInterval = 2000): Promise<AnalysisJob> => {
    while (true) {
      const job = await analyticsApi.getJobStatus(jobId);
      if (job.status === 'completed' || job.status === 'failed') {
        return job;
      }
      await new Promise(resolve => setTimeout(resolve, pollInterval));
    }
  },

  // ========================================
  // Timeline
  // ========================================

  getTaskTimeline: (taskId: string, orgId: string): Promise<WorkflowTimeline> =>
    fetch(`${API_BASE}/timeline/task/${taskId}?org=${encodeURIComponent(orgId)}`)
      .then(handleResponse),

  getGoalTimeline: (goalId: string, orgId: string): Promise<WorkflowTimeline> =>
    fetch(`${API_BASE}/timeline/goal/${goalId}?org=${encodeURIComponent(orgId)}`)
      .then(handleResponse),

  // ========================================
  // Interactions
  // ========================================

  getTaskInteractions: (taskId: string, orgId: string): Promise<InteractionClassification[]> =>
    fetch(`${API_BASE}/interactions/task/${taskId}?org=${encodeURIComponent(orgId)}`)
      .then(handleResponse),

  searchInteractions: (params: SearchInteractionsRequest): Promise<InteractionClassification[]> =>
    fetch(`${API_BASE}/interactions/search`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params)
    }).then(handleResponse),

  // ========================================
  // Patterns
  // ========================================

  getPatterns: (orgId: string, type?: PatternType, activeOnly = true): Promise<WorkflowPattern[]> => {
    const params = new URLSearchParams({ org: orgId });
    if (type) params.set('type', type);
    params.set('active_only', String(activeOnly));
    return fetch(`${API_BASE}/patterns?${params}`).then(handleResponse);
  },

  learnPatterns: (params: PatternLearnRequest): Promise<{ patterns_learned: number; patterns_updated: number }> =>
    fetch(`${API_BASE}/patterns/learn`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params)
    }).then(handleResponse),

  // ========================================
  // Recommendations
  // ========================================

  getTaskRecommendations: (taskId: string, orgId: string): Promise<EfficiencyRecommendation[]> =>
    fetch(`${API_BASE}/recommendations/task/${taskId}?org=${encodeURIComponent(orgId)}`)
      .then(handleResponse),

  getOrgRecommendations: (orgId: string, limit = 20): Promise<EfficiencyRecommendation[]> =>
    fetch(`${API_BASE}/recommendations/organization?org=${encodeURIComponent(orgId)}&limit=${limit}`)
      .then(handleResponse),

  submitFeedback: (recId: string, params: FeedbackRequest): Promise<{ success: boolean }> =>
    fetch(`${API_BASE}/recommendations/${recId}/feedback`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params)
    }).then(handleResponse),
};
```

---

## Visualization Components

### 1. Timeline Gantt Chart

**Recommended Libraries:** `react-chrono`, `vis-timeline`, `@nivo/calendar`, or custom with D3.js

```tsx
// components/TimelineGantt.tsx
import React from 'react';
import { WorkflowTimeline, WorkflowPhase, WorkflowBottleneck } from '../types';

interface Props {
  timeline: WorkflowTimeline;
}

const PHASE_COLORS: Record<string, string> = {
  initiated: '#3498db',
  assigned: '#9b59b6',
  in_progress: '#27ae60',
  blocked: '#e74c3c',
  review: '#f39c12',
  pending_approval: '#e67e22',
  approved: '#2ecc71',
  escalated: '#c0392b',
  completed: '#1abc9c',
};

export const TimelineGantt: React.FC<Props> = ({ timeline }) => {
  const startTime = new Date(timeline.opened_at).getTime();
  const endTime = timeline.closed_at
    ? new Date(timeline.closed_at).getTime()
    : Date.now();
  const totalDuration = endTime - startTime;

  const getPositionPercent = (dateStr: string) => {
    const time = new Date(dateStr).getTime();
    return ((time - startTime) / totalDuration) * 100;
  };

  return (
    <div className="timeline-gantt">
      {/* Phase bars */}
      <div className="phases">
        {timeline.phases.map((phase, i) => {
          const left = getPositionPercent(phase.start);
          const right = phase.end
            ? getPositionPercent(phase.end)
            : 100;
          const width = right - left;

          return (
            <div
              key={i}
              className="phase-bar"
              style={{
                left: `${left}%`,
                width: `${width}%`,
                backgroundColor: PHASE_COLORS[phase.name] || '#95a5a6',
              }}
              title={`${phase.name}: ${phase.interaction_count} interactions`}
            >
              {phase.name}
            </div>
          );
        })}
      </div>

      {/* Bottleneck overlays */}
      <div className="bottlenecks">
        {timeline.bottlenecks.map((b, i) => {
          const left = getPositionPercent(b.start);
          const right = getPositionPercent(b.end);
          const width = right - left;

          return (
            <div
              key={i}
              className="bottleneck-bar"
              style={{
                left: `${left}%`,
                width: `${width}%`,
              }}
              title={b.description}
            />
          );
        })}
      </div>

      {/* Event markers */}
      <div className="events">
        {timeline.key_events.map((event, i) => (
          <div
            key={i}
            className="event-marker"
            style={{ left: `${getPositionPercent(event.timestamp)}%` }}
            title={`${event.event_type}: ${event.description}`}
          />
        ))}
      </div>
    </div>
  );
};
```

### 2. Interaction Type Distribution

**Recommended Libraries:** `@nivo/pie`, `recharts`, `chart.js`

```tsx
// components/InteractionPieChart.tsx
import React, { useMemo } from 'react';
import { ResponsivePie } from '@nivo/pie';
import { InteractionClassification, InteractionType } from '../types';

const INTERACTION_COLORS: Record<InteractionType, string> = {
  request_clarification: '#f1c40f',
  request_resources: '#e67e22',
  direction: '#9b59b6',
  suggestion: '#27ae60',
  request_approval: '#e74c3c',
  status_update: '#3498db',
  acknowledgment: '#1abc9c',
  escalation: '#c0392b',
  blocker: '#e74c3c',
  question: '#f39c12',
  answer: '#2ecc71',
  assignment: '#8e44ad',
  feedback: '#16a085',
  recognition: '#2ecc71',
  other: '#95a5a6',
};

interface Props {
  interactions: InteractionClassification[];
}

export const InteractionPieChart: React.FC<Props> = ({ interactions }) => {
  const data = useMemo(() => {
    const counts: Record<string, number> = {};
    interactions.forEach(i => {
      counts[i.interaction_type] = (counts[i.interaction_type] || 0) + 1;
    });

    return Object.entries(counts).map(([type, count]) => ({
      id: type,
      label: type.replace(/_/g, ' '),
      value: count,
      color: INTERACTION_COLORS[type as InteractionType] || '#95a5a6',
    }));
  }, [interactions]);

  return (
    <div style={{ height: 400 }}>
      <ResponsivePie
        data={data}
        colors={{ datum: 'data.color' }}
        margin={{ top: 40, right: 80, bottom: 80, left: 80 }}
        innerRadius={0.5}
        padAngle={0.7}
        cornerRadius={3}
        activeOuterRadiusOffset={8}
        arcLinkLabelsSkipAngle={10}
        arcLinkLabelsTextColor="#333"
        arcLabelsSkipAngle={10}
      />
    </div>
  );
};
```

### 3. Bottleneck Bar Chart

```tsx
// components/BottleneckChart.tsx
import React from 'react';
import { ResponsiveBar } from '@nivo/bar';
import { WorkflowBottleneck } from '../types';

interface Props {
  bottlenecks: WorkflowBottleneck[];
}

export const BottleneckChart: React.FC<Props> = ({ bottlenecks }) => {
  const data = bottlenecks.map(b => ({
    type: b.bottleneck_type.replace(/_/g, ' '),
    hours: b.duration_hours,
    description: b.description,
  }));

  return (
    <div style={{ height: 300 }}>
      <ResponsiveBar
        data={data}
        keys={['hours']}
        indexBy="type"
        colors={['#e74c3c']}
        margin={{ top: 20, right: 20, bottom: 50, left: 60 }}
        axisBottom={{
          legend: 'Bottleneck Type',
          legendPosition: 'middle',
          legendOffset: 40,
        }}
        axisLeft={{
          legend: 'Hours Delayed',
          legendPosition: 'middle',
          legendOffset: -50,
        }}
        labelSkipWidth={12}
        labelSkipHeight={12}
        tooltip={({ data }) => (
          <div className="tooltip">
            <strong>{data.type}</strong>
            <br />
            {data.hours.toFixed(1)} hours
            <br />
            <small>{data.description}</small>
          </div>
        )}
      />
    </div>
  );
};
```

### 4. Sentiment Timeline

```tsx
// components/SentimentChart.tsx
import React, { useMemo } from 'react';
import { ResponsiveLine } from '@nivo/line';
import { InteractionClassification } from '../types';

interface Props {
  interactions: InteractionClassification[];
}

export const SentimentChart: React.FC<Props> = ({ interactions }) => {
  const data = useMemo(() => {
    const sorted = [...interactions].sort(
      (a, b) => new Date(a.original_created_at).getTime() - new Date(b.original_created_at).getTime()
    );

    return [{
      id: 'sentiment',
      data: sorted.map(i => ({
        x: new Date(i.original_created_at),
        y: i.sentiment,
      })),
    }];
  }, [interactions]);

  return (
    <div style={{ height: 300 }}>
      <ResponsiveLine
        data={data}
        xScale={{ type: 'time' }}
        yScale={{ type: 'linear', min: -1, max: 1 }}
        margin={{ top: 20, right: 20, bottom: 50, left: 60 }}
        axisBottom={{
          format: '%b %d',
          tickRotation: -45,
        }}
        axisLeft={{
          legend: 'Sentiment',
          legendPosition: 'middle',
          legendOffset: -50,
        }}
        pointSize={8}
        pointColor="#3498db"
        enableArea
        areaBaselineValue={0}
        colors={['#3498db']}
        markers={[
          { axis: 'y', value: 0, lineStyle: { stroke: '#95a5a6', strokeDasharray: '4 4' } },
        ]}
      />
    </div>
  );
};
```

### 5. Pattern Cards

```tsx
// components/PatternCard.tsx
import React from 'react';
import { WorkflowPattern, PatternType } from '../types';

const PATTERN_STYLES: Record<PatternType, { color: string; icon: string }> = {
  success: { color: '#27ae60', icon: '✅' },
  failure: { color: '#e74c3c', icon: '❌' },
  bottleneck: { color: '#e67e22', icon: '⏳' },
  efficiency: { color: '#3498db', icon: '⚡' },
};

interface Props {
  pattern: WorkflowPattern;
}

export const PatternCard: React.FC<Props> = ({ pattern }) => {
  const style = PATTERN_STYLES[pattern.pattern_type];

  return (
    <div
      className="pattern-card"
      style={{ borderLeftColor: style.color }}
    >
      <div className="pattern-header">
        <span className="icon">{style.icon}</span>
        <span className="type">{pattern.pattern_type}</span>
      </div>
      <h3>{pattern.pattern_name.replace(/_/g, ' ')}</h3>
      <p>{pattern.description}</p>
      <div className="stats">
        <div className="stat">
          <span className="value">{pattern.occurrence_count}</span>
          <span className="label">occurrences</span>
        </div>
        {pattern.avg_time_impact_hours && (
          <div className="stat">
            <span className="value">{pattern.avg_time_impact_hours.toFixed(1)}h</span>
            <span className="label">avg impact</span>
          </div>
        )}
        <div className="stat">
          <span className="value">{(pattern.confidence_score * 100).toFixed(0)}%</span>
          <span className="label">confidence</span>
        </div>
      </div>
    </div>
  );
};
```

### 6. Recommendation Cards

```tsx
// components/RecommendationCard.tsx
import React from 'react';
import { EfficiencyRecommendation } from '../types';
import { analyticsApi } from '../api/analytics-client';

const PRIORITY_STYLES: Record<string, { color: string; bg: string }> = {
  high: { color: '#e74c3c', bg: '#fce4e4' },
  medium: { color: '#f39c12', bg: '#fef5e7' },
  low: { color: '#95a5a6', bg: '#f5f5f5' },
};

interface Props {
  recommendation: EfficiencyRecommendation;
  orgId: string;
  onUpdate: () => void;
}

export const RecommendationCard: React.FC<Props> = ({ recommendation, orgId, onUpdate }) => {
  const style = PRIORITY_STYLES[recommendation.priority];

  const handleFeedback = async (status: 'accepted' | 'rejected') => {
    await analyticsApi.submitFeedback(recommendation.id, {
      organization_id: orgId,
      status,
    });
    onUpdate();
  };

  return (
    <div
      className="recommendation-card"
      style={{ borderLeftColor: style.color, backgroundColor: style.bg }}
    >
      <div className="header">
        <span className="priority">{recommendation.priority.toUpperCase()}</span>
        <span className="type">{recommendation.recommendation_type}</span>
      </div>

      <h3>{recommendation.title}</h3>
      <p>{recommendation.description}</p>

      <div className="actions-list">
        <h4>Suggested Actions:</h4>
        <ul>
          {recommendation.suggested_actions.map((action, i) => (
            <li key={i}>{action}</li>
          ))}
        </ul>
      </div>

      {recommendation.estimated_time_savings_hours && (
        <div className="time-savings">
          Estimated savings: <strong>{recommendation.estimated_time_savings_hours.toFixed(1)} hours</strong>
        </div>
      )}

      {recommendation.status === 'pending' && (
        <div className="actions">
          <button
            className="accept"
            onClick={() => handleFeedback('accepted')}
          >
            Accept
          </button>
          <button
            className="reject"
            onClick={() => handleFeedback('rejected')}
          >
            Reject
          </button>
        </div>
      )}

      {recommendation.status !== 'pending' && (
        <div className="status-badge" data-status={recommendation.status}>
          {recommendation.status}
        </div>
      )}
    </div>
  );
};
```

---

## Dashboard Layout

```tsx
// pages/AnalyticsDashboard.tsx
import React, { useEffect, useState } from 'react';
import { analyticsApi } from '../api/analytics-client';
import { TimelineGantt } from '../components/TimelineGantt';
import { InteractionPieChart } from '../components/InteractionPieChart';
import { BottleneckChart } from '../components/BottleneckChart';
import { SentimentChart } from '../components/SentimentChart';
import { PatternCard } from '../components/PatternCard';
import { RecommendationCard } from '../components/RecommendationCard';

interface Props {
  taskId: string;
  orgId: string;
}

export const AnalyticsDashboard: React.FC<Props> = ({ taskId, orgId }) => {
  const [timeline, setTimeline] = useState<WorkflowTimeline | null>(null);
  const [interactions, setInteractions] = useState<InteractionClassification[]>([]);
  const [patterns, setPatterns] = useState<WorkflowPattern[]>([]);
  const [recommendations, setRecommendations] = useState<EfficiencyRecommendation[]>([]);
  const [loading, setLoading] = useState(true);
  const [analyzing, setAnalyzing] = useState(false);

  const loadData = async () => {
    setLoading(true);
    try {
      const [t, i, p, r] = await Promise.all([
        analyticsApi.getTaskTimeline(taskId, orgId).catch(() => null),
        analyticsApi.getTaskInteractions(taskId, orgId).catch(() => []),
        analyticsApi.getPatterns(orgId),
        analyticsApi.getTaskRecommendations(taskId, orgId).catch(() => []),
      ]);
      setTimeline(t);
      setInteractions(i);
      setPatterns(p);
      setRecommendations(r);
    } finally {
      setLoading(false);
    }
  };

  const triggerAnalysis = async () => {
    setAnalyzing(true);
    try {
      const job = await analyticsApi.analyzeTask(taskId, orgId);
      await analyticsApi.waitForAnalysis(job.job_id);
      await loadData();
    } finally {
      setAnalyzing(false);
    }
  };

  useEffect(() => {
    loadData();
  }, [taskId, orgId]);

  if (loading) return <div>Loading...</div>;

  return (
    <div className="analytics-dashboard">
      <header>
        <h1>Task Analytics</h1>
        <button onClick={triggerAnalysis} disabled={analyzing}>
          {analyzing ? 'Analyzing...' : 'Re-analyze'}
        </button>
      </header>

      {timeline && (
        <section className="timeline-section">
          <h2>Workflow Timeline</h2>
          <TimelineGantt timeline={timeline} />
          <div className="metrics">
            <div>Interactions: {timeline.total_interactions}</div>
            <div>Participants: {timeline.total_participants}</div>
            <div>Duration: {timeline.total_duration_hours?.toFixed(1)}h</div>
          </div>
        </section>
      )}

      <div className="charts-row">
        <section className="chart-section">
          <h2>Interaction Types</h2>
          <InteractionPieChart interactions={interactions} />
        </section>

        <section className="chart-section">
          <h2>Bottlenecks</h2>
          {timeline && <BottleneckChart bottlenecks={timeline.bottlenecks} />}
        </section>
      </div>

      <section className="sentiment-section">
        <h2>Sentiment Over Time</h2>
        <SentimentChart interactions={interactions} />
      </section>

      <section className="recommendations-section">
        <h2>Recommendations</h2>
        <div className="cards-grid">
          {recommendations.map(rec => (
            <RecommendationCard
              key={rec.id}
              recommendation={rec}
              orgId={orgId}
              onUpdate={loadData}
            />
          ))}
        </div>
      </section>

      <section className="patterns-section">
        <h2>Learned Patterns</h2>
        <div className="cards-grid">
          {patterns.map(pattern => (
            <PatternCard key={pattern.id} pattern={pattern} />
          ))}
        </div>
      </section>
    </div>
  );
};
```

---

## CSS Styles

```css
/* analytics-dashboard.css */

.analytics-dashboard {
  padding: 20px;
  max-width: 1400px;
  margin: 0 auto;
}

.analytics-dashboard header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 24px;
}

.charts-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 24px;
  margin-bottom: 24px;
}

.cards-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 16px;
}

/* Timeline Gantt */
.timeline-gantt {
  position: relative;
  height: 120px;
  background: #f5f5f5;
  border-radius: 4px;
  overflow: hidden;
}

.phase-bar {
  position: absolute;
  height: 40px;
  top: 20px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  padding: 0 8px;
  color: white;
  font-size: 12px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.bottleneck-bar {
  position: absolute;
  height: 20px;
  top: 70px;
  background: rgba(231, 76, 60, 0.3);
  border: 2px dashed #e74c3c;
  border-radius: 4px;
}

.event-marker {
  position: absolute;
  width: 8px;
  height: 8px;
  top: 100px;
  background: #3498db;
  border-radius: 50%;
  transform: translateX(-50%);
}

/* Pattern Card */
.pattern-card {
  background: white;
  border-radius: 8px;
  padding: 16px;
  border-left: 4px solid;
  box-shadow: 0 2px 4px rgba(0,0,0,0.1);
}

.pattern-card .pattern-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.pattern-card .stats {
  display: flex;
  gap: 24px;
  margin-top: 16px;
}

.pattern-card .stat {
  display: flex;
  flex-direction: column;
}

.pattern-card .stat .value {
  font-size: 18px;
  font-weight: 600;
}

.pattern-card .stat .label {
  font-size: 12px;
  color: #666;
}

/* Recommendation Card */
.recommendation-card {
  border-radius: 8px;
  padding: 16px;
  border-left: 4px solid;
}

.recommendation-card .header {
  display: flex;
  justify-content: space-between;
  margin-bottom: 8px;
}

.recommendation-card .priority {
  font-weight: 600;
  font-size: 12px;
}

.recommendation-card .actions-list ul {
  margin: 0;
  padding-left: 20px;
}

.recommendation-card .actions {
  display: flex;
  gap: 8px;
  margin-top: 16px;
}

.recommendation-card .actions button {
  padding: 8px 16px;
  border-radius: 4px;
  border: none;
  cursor: pointer;
}

.recommendation-card .actions .accept {
  background: #27ae60;
  color: white;
}

.recommendation-card .actions .reject {
  background: #e74c3c;
  color: white;
}

.status-badge {
  display: inline-block;
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 500;
  margin-top: 16px;
}

.status-badge[data-status="accepted"] {
  background: #d5f5e3;
  color: #27ae60;
}

.status-badge[data-status="rejected"] {
  background: #fce4e4;
  color: #e74c3c;
}

.status-badge[data-status="implemented"] {
  background: #d6eaf8;
  color: #3498db;
}
```

---

## Usage Example

```tsx
// App.tsx
import React from 'react';
import { AnalyticsDashboard } from './pages/AnalyticsDashboard';

function App() {
  // Get from route params or context
  const taskId = 'task-123';
  const orgId = 'org-456';

  return (
    <div className="app">
      <AnalyticsDashboard taskId={taskId} orgId={orgId} />
    </div>
  );
}

export default App;
```
