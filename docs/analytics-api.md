# Analytics API Documentation

## Base URL
```
https://your-server/api/analytics
```

---

## 1. Task/Goal Analysis

### Trigger Task Analysis
```
POST /api/analytics/analysis/task/:task_id
```
**Body:**
```json
{
  "organization_id": "uuid-string"
}
```
**Response:**
```json
{
  "job_id": "uuid",
  "status": "pending",
  "message": "Analysis job queued"
}
```

### Trigger Goal Analysis
```
POST /api/analytics/analysis/goal/:goal_id
```
**Body:**
```json
{
  "organization_id": "uuid-string"
}
```
**Response:** Same as task analysis

### Get Analysis Job Status
```
GET /api/analytics/jobs/:job_id
```
**Response:**
```json
{
  "id": "uuid",
  "organization_id": "uuid",
  "entity_type": "task",
  "entity_id": "uuid",
  "status": "completed",
  "created_at": "2026-02-05T02:36:54Z",
  "completed_at": "2026-02-05T02:37:02Z",
  "error": null
}
```

**Status values:** `pending`, `processing`, `completed`, `failed`

---

## 2. Timeline & Interactions

### Get Task Timeline
```
GET /api/analytics/timeline/task/:task_id?organization_id=uuid
```
**Response:**
```json
{
  "task_id": "uuid",
  "phases": [
    { "name": "planning", "started_at": "...", "ended_at": "..." },
    { "name": "execution", "started_at": "...", "ended_at": null }
  ],
  "key_events": [
    { "type": "interaction:assignment", "timestamp": "...", "description": "..." }
  ],
  "duration_days": 5
}
```

### Get Goal Timeline
```
GET /api/analytics/timeline/goal/:goal_id?organization_id=uuid
```
**Response:** Similar structure to task timeline

### Get Task Interactions
```
GET /api/analytics/interactions/task/:task_id?organization_id=uuid
```
**Response:**
```json
{
  "task_id": "uuid",
  "interactions": [
    {
      "id": "uuid",
      "interaction_type": "question",
      "sender_id": "user-uuid",
      "sentiment": 0.75,
      "confidence": 0.92,
      "source_type": "comment",
      "original_created_at": "2026-02-01T10:30:00Z",
      "entities": {
        "mentioned_users": ["user-uuid-1"],
        "referenced_tasks": [],
        "keywords": ["deadline", "review"]
      }
    }
  ]
}
```

**source_type values:** `comment`, `message`, `activity_log`

### Search Interactions
```
POST /api/analytics/interactions/search?organization_id=uuid
```
**Query Params:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `organization_id` | Yes | Organization UUID |
| `interaction_type` | No | Filter by type |
| `limit` | No | Max results (default 50, max 500) |

**Response:**
```json
{
  "interactions": [...],
  "total": 42
}
```

---

## 3. Patterns & Recommendations

### List Learned Patterns
```
GET /api/analytics/patterns?organization_id=uuid
```
**Response:**
```json
{
  "patterns": [
    {
      "id": "uuid",
      "pattern_type": "workflow",
      "description": "Tasks with blockers take 3x longer",
      "confidence": 0.85,
      "frequency": 12,
      "created_at": "..."
    }
  ]
}
```

### Trigger Pattern Learning
```
POST /api/analytics/patterns/learn
```
**Body:**
```json
{
  "organization_id": "uuid"
}
```

### Get Task Recommendations
```
GET /api/analytics/recommendations/task/:task_id?organization_id=uuid
```
**Response:**
```json
{
  "task_id": "uuid",
  "recommendations": [
    {
      "id": "uuid",
      "type": "efficiency",
      "priority": "high",
      "description": "Consider breaking this task into subtasks",
      "status": "pending",
      "confidence": 0.78
    }
  ]
}
```

### Get Organization Recommendations
```
GET /api/analytics/recommendations/organization?organization_id=uuid&limit=20
```

### Submit Recommendation Feedback
```
POST /api/analytics/recommendations/:id/feedback
```
**Body:**
```json
{
  "status": "accepted",
  "feedback": "Optional comment"
}
```

**status values:** `accepted`, `rejected`, `implemented`

---

## 4. User Analytics

### Get User Performance
```
GET /api/analytics/user/:user_id/performance?organization_id=uuid&from_date=2026-01-01&to_date=2026-02-05
```
**Query Params:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `organization_id` | Yes | Organization UUID |
| `from_date` | No | YYYY-MM-DD (default: 30 days ago) |
| `to_date` | No | YYYY-MM-DD (default: today) |

**Response:**
```json
{
  "user_id": "uuid",
  "organization_id": "uuid",
  "period": {
    "from": "2026-01-01T00:00:00Z",
    "to": "2026-02-05T23:59:59Z"
  },
  "tasks": {
    "total": 25,
    "completed": 18,
    "by_status": {
      "completed": 18,
      "in_progress": 5,
      "pending": 2
    }
  },
  "goals": {
    "total": 5,
    "completed": 3,
    "by_status": {
      "completed": 3,
      "active": 2
    }
  },
  "completion_rate": 72.0
}
```

### Get User Interactions
```
GET /api/analytics/user/:user_id/interactions?organization_id=uuid&days=30
```
**Query Params:**
| Parameter | Required | Description |
|-----------|----------|-------------|
| `organization_id` | Yes | Organization UUID |
| `days` | No | Number of days (default 30, max 365) |

**Response:**
```json
{
  "user_id": "uuid",
  "organization_id": "uuid",
  "period": {
    "days": 30,
    "from": "2026-01-05T02:36:54Z",
    "to": "2026-02-05T02:36:54Z"
  },
  "summary": {
    "total_interactions": 87,
    "average_sentiment": 0.65,
    "unique_collaborators": 12
  },
  "interaction_types": {
    "question": 23,
    "status_update": 31,
    "feedback": 15,
    "decision": 8,
    "blocker": 5,
    "general": 5
  },
  "top_collaborators": [
    { "user_id": "uuid-1", "interaction_count": 24 },
    { "user_id": "uuid-2", "interaction_count": 18 }
  ]
}
```

### Get User Sentiment Time Series
```
GET /api/analytics/user/:user_id/sentiment?organization_id=uuid&from_date=2026-01-01&to_date=2026-02-05
```
**Response:**
```json
{
  "user_id": "uuid",
  "organization_id": "uuid",
  "period": {
    "from": "2026-01-01T00:00:00Z",
    "to": "2026-02-05T23:59:59Z"
  },
  "overall": {
    "average_sentiment": 0.62,
    "total_interactions": 87
  },
  "time_series": [
    { "date": "2026-01-01", "average_sentiment": 0.58, "interaction_count": 5 },
    { "date": "2026-01-02", "average_sentiment": 0.72, "interaction_count": 3 }
  ]
}
```

---

## 5. System Info

### Analytics Info
```
GET /api/analytics/info
```
**Response:**
```json
{
  "name": "Workflow Intelligence Analytics",
  "version": "0.1.0",
  "capabilities": {
    "interaction_classification": true,
    "timeline_reconstruction": true,
    "pattern_learning": true,
    "recommendations": true,
    "user_analytics": true
  },
  "interaction_types": [
    "question", "decision", "blocker", "escalation",
    "status_update", "feedback", "assignment", "general"
  ]
}
```

---

## Interaction Types

| Type | Description |
|------|-------------|
| `question` | Questions seeking information |
| `decision` | Decisions made on the task/goal |
| `blocker` | Blockers or impediments |
| `escalation` | Issues escalated to higher authority |
| `status_update` | Progress or status updates |
| `feedback` | Feedback on work or process |
| `assignment` | Task/role assignments |
| `general` | General discussion |

---

## Error Responses

All endpoints return errors in this format:
```json
{
  "error": "Description of what went wrong"
}
```

**Common HTTP status codes:**
| Code | Description |
|------|-------------|
| `400` | Bad Request (invalid parameters) |
| `404` | Not Found |
| `500` | Internal Server Error |
| `503` | Service Unavailable (PostgreSQL not available) |

---

## Usage Notes

1. **Organization Scoping**: All endpoints require an `organization_id` to ensure data isolation between organizations.

2. **Analysis Jobs**: Task/goal analysis runs asynchronously. Poll the job status endpoint to check completion.

3. **Sentiment Scores**: Range from -1.0 (very negative) to 1.0 (very positive), with 0.0 being neutral.

4. **Confidence Scores**: Range from 0.0 to 1.0, indicating the classifier's confidence in the interaction type.

5. **Rate Limits**: Query limits are capped at 500 results per request to prevent excessive load.
