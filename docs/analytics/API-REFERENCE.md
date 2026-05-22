# Analytics API Reference

## Base URL
```
/api/analytics
```

## Authentication
All endpoints require `organization_id` parameter for multi-tenancy.

---

## 1. Analysis Endpoints

### Trigger Task Analysis
```http
POST /api/analytics/analysis/task/{task_id}
Content-Type: application/json

{
  "organization_id": "org-123"
}
```

**Response:**
```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "pending",
  "progress_percent": 0,
  "current_stage": "initializing",
  "entity_type": "task",
  "entity_id": "task-456",
  "started_at": "2025-01-27T10:00:00Z",
  "completed_at": null,
  "error_message": null
}
```

### Trigger Goal Analysis
```http
POST /api/analytics/analysis/goal/{goal_id}
Content-Type: application/json

{
  "organization_id": "org-123"
}
```

### Get Analysis Job Status
```http
GET /api/analytics/analysis/job/{job_id}
```

**Response:**
```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "processing",
  "progress_percent": 45,
  "current_stage": "classifying_interactions",
  "entity_type": "task",
  "entity_id": "task-456",
  "started_at": "2025-01-27T10:00:00Z",
  "completed_at": null,
  "error_message": null
}
```

**Job Statuses:**
- `pending` - Job created, waiting to start
- `processing` - Currently running
- `completed` - Successfully finished
- `failed` - Error occurred

**Processing Stages:**
- `initializing`
- `fetching_data`
- `classifying_interactions`
- `reconstructing_timeline`
- `detecting_bottlenecks`
- `matching_patterns`
- `generating_recommendations`
- `finalizing`

---

## 2. Timeline Endpoints

### Get Task Timeline
```http
GET /api/analytics/timeline/task/{task_id}?org={organization_id}
```

### Get Goal Timeline
```http
GET /api/analytics/timeline/goal/{goal_id}?org={organization_id}
```

**Response:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "organization_id": "org-123",
  "entity_type": "task",
  "entity_id": "task-456",
  "total_interactions": 23,
  "total_participants": 5,
  "total_duration_hours": 48.5,
  "phases": [
    {
      "name": "initiated",
      "start": "2025-01-20T09:00:00Z",
      "end": "2025-01-20T11:00:00Z",
      "interaction_count": 3,
      "participants": ["user-1", "user-2"]
    },
    {
      "name": "assigned",
      "start": "2025-01-20T11:00:00Z",
      "end": "2025-01-20T11:30:00Z",
      "interaction_count": 2,
      "participants": ["user-1"]
    },
    {
      "name": "in_progress",
      "start": "2025-01-20T11:30:00Z",
      "end": "2025-01-21T15:00:00Z",
      "interaction_count": 15,
      "participants": ["user-1", "user-2", "user-3"]
    },
    {
      "name": "review",
      "start": "2025-01-21T15:00:00Z",
      "end": "2025-01-22T09:30:00Z",
      "interaction_count": 3,
      "participants": ["user-2", "user-4"]
    }
  ],
  "key_events": [
    {
      "timestamp": "2025-01-20T09:00:00Z",
      "event_type": "activity:created",
      "description": "Task created",
      "actor_id": "user-1",
      "actor_name": "John Doe",
      "interaction_id": null,
      "metadata": null
    },
    {
      "timestamp": "2025-01-20T14:00:00Z",
      "event_type": "interaction:request_approval",
      "description": "request_approval: Can you please review and approve...",
      "actor_id": "user-2",
      "actor_name": null,
      "interaction_id": "int-789",
      "metadata": {
        "source_type": "task_comment",
        "confidence": 0.92,
        "urgency": "medium"
      }
    }
  ],
  "bottlenecks": [
    {
      "bottleneck_type": "approval_delay",
      "duration_hours": 26.5,
      "description": "Approval took 26.5 hours (threshold: 24h)",
      "start": "2025-01-20T14:00:00Z",
      "end": "2025-01-21T16:30:00Z",
      "caused_by": null
    },
    {
      "bottleneck_type": "communication_gap",
      "duration_hours": 52.0,
      "description": "No activity for 52.0 hours",
      "start": "2025-01-19T10:00:00Z",
      "end": "2025-01-21T14:00:00Z",
      "caused_by": null
    }
  ],
  "status": "completed",
  "opened_at": "2025-01-20T09:00:00Z",
  "closed_at": "2025-01-22T09:30:00Z",
  "last_analyzed_at": "2025-01-27T10:05:00Z"
}
```

**Phase Names:**
- `initiated` - Task created
- `assigned` - Assignee set
- `in_progress` - Work started
- `blocked` - Impediment encountered
- `review` - Awaiting review
- `pending_approval` - Awaiting approval
- `approved` - Approved
- `escalated` - Escalated to management
- `completed` - Task done

**Bottleneck Types:**
- `approval_delay` - Approval took > 24h
- `blocked_period` - Task blocked > 4h
- `communication_gap` - No activity > 48h
- `clarification_loop` - 3+ clarification requests

---

## 3. Interaction Endpoints

### Get Task Interactions
```http
GET /api/analytics/interactions/task/{task_id}?org={organization_id}
```

**Response:**
```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "organization_id": "org-123",
    "source_type": "task_comment",
    "source_id": "comment-123",
    "task_id": "task-456",
    "goal_id": null,
    "sender_id": "user-1",
    "content": "Can you clarify the requirements for the authentication module?",
    "interaction_type": "request_clarification",
    "secondary_types": ["question"],
    "confidence_score": 0.92,
    "sentiment": -0.1,
    "urgency_level": "medium",
    "entities": {
      "mentioned_users": ["user-2"],
      "mentioned_deadlines": ["Friday"],
      "action_items": ["Clarify authentication requirements"],
      "blockers": [],
      "resources": ["authentication module"]
    },
    "references_interaction_id": null,
    "original_created_at": "2025-01-20T10:30:00Z",
    "classified_at": "2025-01-27T10:02:00Z"
  }
]
```

### Search Interactions
```http
POST /api/analytics/interactions/search
Content-Type: application/json

{
  "organization_id": "org-123",
  "interaction_types": ["blocker", "escalation"],
  "urgency_levels": ["high", "critical"],
  "task_id": "task-456",
  "goal_id": null,
  "sender_id": null,
  "from_date": "2025-01-01T00:00:00Z",
  "to_date": "2025-01-31T23:59:59Z",
  "content_search": "blocked",
  "limit": 50
}
```

**All fields except `organization_id` are optional.**

---

## 4. Pattern Endpoints

### List Patterns
```http
GET /api/analytics/patterns?org={organization_id}&type={pattern_type}&active_only={true|false}
```

**Query Parameters:**
- `org` (required) - Organization ID
- `type` (optional) - Filter by pattern type: `success`, `failure`, `bottleneck`, `efficiency`
- `active_only` (optional, default: true) - Only show active patterns

**Response:**
```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "organization_id": "org-123",
    "pattern_type": "bottleneck",
    "pattern_name": "common_approval_delay",
    "description": "Recurring approval_delay bottleneck affecting 12 tasks (avg 28.3h delay)",
    "criteria": {
      "bottleneck_type": "approval_delay"
    },
    "occurrence_count": 12,
    "success_correlation": null,
    "avg_time_impact_hours": 28.3,
    "confidence_score": 0.75,
    "examples": ["task-1", "task-5", "task-9", "task-12"],
    "is_active": true,
    "created_at": "2025-01-15T08:00:00Z",
    "updated_at": "2025-01-27T10:00:00Z"
  },
  {
    "id": "660e8400-e29b-41d4-a716-446655440001",
    "organization_id": "org-123",
    "pattern_type": "success",
    "pattern_name": "fast_completion",
    "description": "Tasks completed faster than median (4.2h) share these traits: minimal communication overhead, focused team (1-2 people)",
    "criteria": {
      "duration_below_hours": 4.2,
      "traits": ["minimal communication overhead", "focused team (1-2 people)"]
    },
    "occurrence_count": 8,
    "success_correlation": 1.0,
    "avg_time_impact_hours": 2.1,
    "confidence_score": 0.67,
    "examples": ["task-2", "task-7", "task-11"],
    "is_active": true,
    "created_at": "2025-01-15T08:00:00Z",
    "updated_at": "2025-01-27T10:00:00Z"
  }
]
```

### Trigger Pattern Learning
```http
POST /api/analytics/patterns/learn
Content-Type: application/json

{
  "organization_id": "org-123",
  "min_occurrences": 3,
  "include_completed_only": true
}
```

**Response:**
```json
{
  "patterns_learned": 5,
  "patterns_updated": 2,
  "message": "Pattern learning completed successfully"
}
```

---

## 5. Recommendation Endpoints

### Get Task Recommendations
```http
GET /api/analytics/recommendations/task/{task_id}?org={organization_id}
```

### Get Organization Recommendations
```http
GET /api/analytics/recommendations/organization?org={organization_id}&limit={n}
```

**Response:**
```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "organization_id": "org-123",
    "target_type": "task",
    "target_id": "task-456",
    "recommendation_type": "communication",
    "title": "Reduce Clarification Overhead",
    "description": "This task has 5 clarification requests, which is 2.5x higher than average. Consider improving initial specifications to reduce back-and-forth.",
    "suggested_actions": [
      "Add detailed acceptance criteria to task description",
      "Include visual mockups or wireframes where applicable",
      "Define specific deliverables with measurable outcomes",
      "Link to relevant documentation or prior similar tasks"
    ],
    "based_on_patterns": ["pattern-uuid-1", "pattern-uuid-2"],
    "evidence": {
      "clarification_count": 5,
      "avg_for_similar_tasks": 2,
      "time_spent_on_clarifications_hours": 4.5
    },
    "priority": "high",
    "estimated_time_savings_hours": 3.5,
    "status": "pending",
    "user_feedback": null,
    "generated_at": "2025-01-27T10:05:00Z"
  },
  {
    "id": "660e8400-e29b-41d4-a716-446655440001",
    "organization_id": "org-123",
    "target_type": "organization",
    "target_id": null,
    "recommendation_type": "process",
    "title": "Streamline Approval Process",
    "description": "12 tasks experienced approval delays averaging 28.3 hours. Consider implementing automated approval routing or setting SLAs.",
    "suggested_actions": [
      "Set 24-hour SLA for approvals",
      "Implement automated escalation after 12 hours",
      "Consider parallel approval workflows",
      "Add backup approvers for vacation coverage"
    ],
    "based_on_patterns": ["pattern-uuid-3"],
    "evidence": {
      "affected_tasks": 12,
      "avg_delay_hours": 28.3,
      "total_time_lost_hours": 339.6
    },
    "priority": "high",
    "estimated_time_savings_hours": 200.0,
    "status": "pending",
    "user_feedback": null,
    "generated_at": "2025-01-27T10:05:00Z"
  }
]
```

### Submit Recommendation Feedback
```http
POST /api/analytics/recommendations/{recommendation_id}/feedback
Content-Type: application/json

{
  "organization_id": "org-123",
  "status": "accepted",
  "feedback": "Implemented the suggestion. Will track results over next sprint."
}
```

**Status Values:**
- `pending` - Not yet reviewed
- `accepted` - User accepted recommendation
- `rejected` - User rejected recommendation
- `implemented` - Recommendation has been implemented

**Response:**
```json
{
  "success": true,
  "message": "Feedback recorded"
}
```

---

## Error Responses

All endpoints return errors in this format:

```json
{
  "error": "Brief error message",
  "code": "ERROR_CODE"
}
```

**Common Error Codes:**
- `INVALID_ORG_ID` - Organization ID missing or invalid
- `NOT_FOUND` - Resource not found
- `VALIDATION_ERROR` - Request validation failed
- `INTERNAL_ERROR` - Server error (details logged, not exposed)

**HTTP Status Codes:**
- `200` - Success
- `201` - Created
- `400` - Bad Request
- `404` - Not Found
- `500` - Internal Server Error

---

## Rate Limits

- Query limit: Maximum 500 results per request
- Batch classification: Maximum 100 interactions per batch
- Analysis jobs: One concurrent job per entity

---

## Interaction Types Reference

| Type | Description | Example |
|------|-------------|---------|
| `request_clarification` | Asking for more details | "Can you explain what you mean by...?" |
| `request_resources` | Requesting tools, people, budget | "We need access to the staging server" |
| `direction` | Giving instructions | "Please implement it this way..." |
| `suggestion` | Proposing alternatives | "What if we tried a different approach?" |
| `request_approval` | Seeking sign-off | "Can you approve this PR?" |
| `status_update` | Progress reports | "Completed the first milestone" |
| `acknowledgment` | Confirmations | "Got it, will do" |
| `escalation` | Raising to higher authority | "Need to escalate this to management" |
| `blocker` | Reporting impediments | "I'm blocked on the database migration" |
| `question` | General questions | "How does this work?" |
| `answer` | Responses to questions | "It works by..." |
| `assignment` | Delegating work | "Can you take over this task?" |
| `feedback` | Reviews and critiques | "The code looks good, but..." |
| `recognition` | Praise and appreciation | "Great job on this!" |
| `other` | Uncategorized | (fallback) |
