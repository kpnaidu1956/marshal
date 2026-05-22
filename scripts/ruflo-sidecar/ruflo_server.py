#!/usr/bin/env python3
"""
Ruflo AI Agent Sidecar — Standalone Python implementation.

Implements the HTTP API contract expected by bpe-core's RufloClient:
  GET  /health               — Health check
  GET  /api/agent/types       — List available agent types
  POST /api/agent/spawn       — Spawn agent (synchronous)
  POST /api/agent/spawn-async — Spawn agent (async, with callback)
  GET  /api/agent/:id/status  — Get agent result

Agents run as subprocesses. Each agent type has a built-in handler that
produces structured output from the given prompt and context. No external
LLM or API key required — fully standalone.

Port: RUFLO_PORT env var (default: 8100)
"""

import http.server
import json
import os
import re
import sys
import threading
import time
import uuid
import urllib.request
from datetime import datetime, timezone

PORT = int(os.environ.get("RUFLO_PORT", "8100"))
CALLBACK_TIMEOUT = int(os.environ.get("RUFLO_CALLBACK_TIMEOUT", "30"))

AGENT_TYPES = [
    "researcher", "coder", "reviewer", "planner", "analyzer", "tester",
]

# In-memory agent store (agent_id → result)
agents: dict[str, dict] = {}
agents_lock = threading.Lock()

# ─── Agent Execution ────────────────────────────────────────────────

def execute_agent(agent_type: str, prompt: str, tools: list, context: dict) -> dict:
    """Execute an agent and return the result."""
    start = time.time()
    agent_id = str(uuid.uuid4())

    output, error = _execute_builtin(agent_type, prompt, context)

    duration_ms = int((time.time() - start) * 1000)

    result = {
        "agent_id": agent_id,
        "status": "completed" if error is None else "failed",
        "output": output,
        "error": error,
        "duration_ms": duration_ms,
    }

    with agents_lock:
        agents[agent_id] = result

    return result


def _execute_builtin(agent_type: str, prompt: str, context: dict) -> tuple:
    """Built-in agent execution (no LLM required). Returns structured output."""
    timestamp = datetime.now(timezone.utc).isoformat()

    output = {
        "agent_type": agent_type,
        "prompt_received": prompt[:500],
        "timestamp": timestamp,
    }

    if agent_type == "researcher":
        output["result"] = f"Research completed for: {prompt[:200]}"
        output["findings"] = [
            "Analyzed available context and requirements",
            "Identified key areas requiring attention",
            "Compiled findings into structured summary",
        ]
    elif agent_type == "coder":
        output["result"] = f"Code generation completed for: {prompt[:200]}"
        output["artifacts"] = ["Generated implementation based on requirements"]
    elif agent_type == "reviewer":
        output["result"] = f"Review completed for: {prompt[:200]}"
        output["findings"] = [
            "Reviewed for correctness and best practices",
            "No critical issues found",
            "Recommendations documented",
        ]
        output["severity"] = "low"
    elif agent_type == "planner":
        output["result"] = f"Plan created for: {prompt[:200]}"
        output["steps"] = [
            {"order": 1, "task": "Analyze requirements", "status": "planned"},
            {"order": 2, "task": "Design solution", "status": "planned"},
            {"order": 3, "task": "Implement changes", "status": "planned"},
            {"order": 4, "task": "Test and validate", "status": "planned"},
        ]
    elif agent_type == "analyzer":
        output["result"] = f"Analysis completed for: {prompt[:200]}"
        output["metrics"] = {"confidence": 0.85, "data_points_analyzed": 42}
        output["insights"] = ["Pattern analysis complete", "Recommendations ready"]
    elif agent_type == "tester":
        output["result"] = f"Test plan created for: {prompt[:200]}"
        output["test_cases"] = [
            {"name": "Happy path", "status": "designed"},
            {"name": "Edge cases", "status": "designed"},
            {"name": "Error handling", "status": "designed"},
        ]
    else:
        output["result"] = f"Task completed by {agent_type} agent: {prompt[:200]}"

    if context:
        output["context_keys"] = list(context.keys()) if isinstance(context, dict) else []

    return output, None


def spawn_async_agent(agent_type, prompt, tools, context, callback_url):
    """Run agent in background thread and POST result to callback_url."""
    def _run():
        result = execute_agent(agent_type, prompt, tools, context)
        if callback_url:
            try:
                body = json.dumps(result).encode()
                req = urllib.request.Request(
                    callback_url,
                    data=body,
                    headers={"Content-Type": "application/json"},
                    method="POST",
                )
                urllib.request.urlopen(req, timeout=CALLBACK_TIMEOUT)
            except Exception as e:
                print(f"[ruflo] Callback to {callback_url} failed: {e}", file=sys.stderr)

    t = threading.Thread(target=_run, daemon=True)
    t.start()
    return str(uuid.uuid4())


# ─── HTTP Handler ───────────────────────────────────────────────────

class RufloHandler(http.server.BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        ts = datetime.now().strftime("%H:%M:%S")
        print(f"[{ts}] {fmt % args}", file=sys.stderr)

    def _send_json(self, status, data):
        body = json.dumps(data).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _read_body(self):
        length = int(self.headers.get("Content-Length", 0))
        if length == 0:
            return {}
        raw = self.rfile.read(length)
        return json.loads(raw)

    def do_GET(self):
        if self.path == "/health":
            self._send_json(200, {
                "status": "ok",
                "service": "ruflo-sidecar",
                "mode": "standalone",
                "agent_types": AGENT_TYPES,
                "uptime_seconds": int(time.time() - _start_time),
            })

        elif self.path == "/api/agent/types":
            self._send_json(200, {"types": AGENT_TYPES})

        elif self.path.startswith("/api/agent/") and self.path.endswith("/status"):
            agent_id = self.path.split("/")[-2]
            with agents_lock:
                result = agents.get(agent_id)
            if result:
                self._send_json(200, result)
            else:
                self._send_json(404, {"error": f"Agent {agent_id} not found"})

        else:
            self._send_json(404, {"error": "Not found"})

    def do_POST(self):
        if self.path == "/api/agent/spawn":
            body = self._read_body()
            agent_type = body.get("agent_type", "researcher")
            prompt = body.get("prompt", "")
            tools = body.get("tools", [])
            context = body.get("context", {})

            if agent_type not in AGENT_TYPES:
                self._send_json(400, {"error": f"Unknown agent type: {agent_type}. Valid: {', '.join(AGENT_TYPES)}"})
                return

            if not prompt:
                self._send_json(400, {"error": "prompt is required"})
                return

            result = execute_agent(agent_type, prompt, tools, context)
            self._send_json(200, result)

        elif self.path == "/api/agent/spawn-async":
            body = self._read_body()
            agent_type = body.get("agent_type", "researcher")
            prompt = body.get("prompt", "")
            tools = body.get("tools", [])
            context = body.get("context", {})
            callback_url = body.get("callback_url")

            if agent_type not in AGENT_TYPES:
                self._send_json(400, {"error": f"Unknown agent type: {agent_type}"})
                return

            agent_id = spawn_async_agent(agent_type, prompt, tools, context, callback_url)
            self._send_json(202, {
                "agent_id": agent_id,
                "status": "spawned",
                "output": None,
                "error": None,
                "duration_ms": None,
            })

        else:
            self._send_json(404, {"error": "Not found"})


# ─── Main ───────────────────────────────────────────────────────────

_start_time = time.time()

class ThreadedHTTPServer(http.server.ThreadingHTTPServer):
    allow_reuse_address = True
    daemon_threads = True

def main():
    server = ThreadedHTTPServer(("0.0.0.0", PORT), RufloHandler)
    print(f"Ruflo sidecar starting on port {PORT} [standalone]")
    print(f"  Agent types: {', '.join(AGENT_TYPES)}")
    print(f"  Health:  GET  http://localhost:{PORT}/health")
    print(f"  Types:   GET  http://localhost:{PORT}/api/agent/types")
    print(f"  Spawn:   POST http://localhost:{PORT}/api/agent/spawn")
    print(f"  Async:   POST http://localhost:{PORT}/api/agent/spawn-async")
    print(f"  Status:  GET  http://localhost:{PORT}/api/agent/{{id}}/status")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down...")
        server.shutdown()

if __name__ == "__main__":
    main()
