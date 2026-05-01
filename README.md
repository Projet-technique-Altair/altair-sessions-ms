# Altaïr Sessions Microservice

> **Lab session orchestrator managing lifecycle, progression tracking, and runtime coordination**
> 

[![Cloud Run](https://img.shields.io/badge/deploy-Cloud%20Run-blue)](https://cloud.google.com/run)

[![Rust](https://img.shields.io/badge/rust-nightly-orange)](https://www.rust-lang.org)

[![PostgreSQL](336791)](https://www.postgresql.org)

---

## Description

The **Altaïr Sessions Microservice** is the orchestrator for lab execution environments. It manages the complete lifecycle of lab sessions, from spawning ephemeral runtime containers to tracking student progress through challenges.

This service coordinates between **Labs MS** (pedagogical data), **Lab API Service** (runtime orchestration), and **PostgreSQL** (session persistence) to provide a seamless learning experience.

**Key capabilities:**

- Create and manage lab session lifecycle (created → running → stopped/expired)
- Spawn/stop runtime environments via Lab API Service
- Track CTF-style progression (steps, hints, score, attempts)
- Validate step answers with multiple validation types
- Enforce linear progression through challenge steps
- Expire stale sessions automatically (2-hour limit)
- Provide authorization based on session ownership

---

## ⚠️ Security Notice

**This service must run in a private network behind the API Gateway.**

- Trusts `x-altair-user-id` and `x-altair-roles` headers injected by Gateway
- Enforces ownership-based access control for sessions
- Creator-only access for lab-specific session listings
- No JWT validation (relies on Gateway trust boundary)

**Deployment requirement:** Must be accessible only via authenticated Gateway.

---

## Architecture

```
┌─────────────┐       ┌──────────────┐       ┌─────────────────┐
│  Frontend   │──────▶│   Gateway    │──────▶│  Sessions MS    │
│             │       │              │       │    (:3003)      │
└─────────────┘       └──────────────┘       └────────┬────────┘
                                                       │
                        ┌──────────────────────────────┼──────────────────┐
                        │                              │                  │
                        ▼                              ▼                  ▼
                ┌───────────────┐            ┌─────────────────┐  ┌─────────────┐
                │  PostgreSQL   │            │   Labs MS       │  │ Lab API     │
                │  (Sessions)   │            │   (:3002)       │  │ Service     │
                └───────────────┘            └─────────────────┘  └─────────────┘
                  lab_sessions                Lab metadata          Runtime pods
                  lab_progress                Steps & hints         (GKE)
```

### Service Flow

1. **User requests lab start** → Gateway validates JWT
2. **Sessions MS** checks for existing session (start-or-resume)
3. **Sessions MS** fetches lab metadata from Labs MS
4. **Sessions MS** spawns runtime via Lab API Service
5. **Sessions MS** persists session + progress to PostgreSQL
6. **User interacts** → Validates steps, requests hints
7. **Sessions MS** tracks progression → Updates score and state
8. **User completes** → Sessions MS stops runtime and finalizes session

---

## Tech Stack

| Component | Technology | Purpose |
| --- | --- | --- |
| **Language** | Rust (nightly) | High-performance async runtime |
| **HTTP Framework** | Axum | HTTP routing and middleware |
| **Async Runtime** | Tokio | Async I/O and concurrency |
| **Database** | PostgreSQL | Session and progress persistence |
| **DB Client** | SQLx | Compile-time checked queries |
| **HTTP Client** | reqwest | Labs MS and Lab API calls |
| **Logging** | tracing + EnvFilter | Structured logging |
| **CI/CD** | GitHub Actions | fmt, clippy, tests |
| **Deployment** | Google Cloud Run | Serverless auto-scaling |

---

## Requirements

### Development

- **Rust** nightly toolchain
- **Docker** & Docker Compose
- **PostgreSQL** 14+ (via `docker compose up postgres`)
- **Labs MS** running (for lab metadata)
- **Lab API Service** running (for runtime spawning)

### Production (Cloud Run)

- **DATABASE_URL** environment variable (PostgreSQL connection string)
- **LABS_MS_URL** – Labs microservice internal URL
- **LAB_API_URL** – Lab API Service internal URL
- **PORT** environment variable (default: `3003`)

### Environment Variables

```bash
# Database (required)
DATABASE_URL=postgresql://altair:altair@postgres:5432/altair_sessions_db

# Upstream services
LABS_MS_URL=http://localhost:3002              # Labs microservice
LAB_API_URL=http://localhost:8085              # Lab API Service

# Server configuration
PORT=3003                                       # Server port (default: 3003)
RUST_LOG=info                                   # Log level filter
```

---

## Installation

### 0. Start infrastructure (database required)

```bash
cd ../altair-infra
docker compose up postgres
```

### 1. Build the Docker image

**Build only if:**

- You modified the sessions code
- You modified the Dockerfile
- First run on a new machine

```bash
cd altair-sessions-ms
docker build -t altair-sessions-ms .
```

### 2. Run the service

```bash
docker run --rm -it \
  --network altair-infra_default \
  -p 3003:3003 \
  --env-file .env \
  --name altair-sessions-ms \
  altair-sessions-ms
```

**Note:** The service is designed to be destroyed when the terminal closes. Rebuild is necessary for code changes.

---

## Usage

### API Endpoints

#### **GET /health**

Health check for liveness/readiness probes.

**Response:**

```json
{
  "status": "ok"
}
```

---

#### **POST /labs/:lab_id/start**

Start a new session or resume an existing one for a lab.

**Headers (injected by Gateway):**

- `x-altair-user-id` (required, UUID) – User's internal ID

**Behavior:**

- **Start-or-resume:** If active session exists (`created` or `running`), returns it
- **New session:** Creates session, spawns runtime, initializes progress

**Processing Flow:**

1. Check for existing session with status `created` or `running`
2. If exists, return existing session (resume)
3. If not exists:
    - Insert `lab_sessions` with status `created`
    - Insert `lab_progress` with initial state
    - Fetch lab steps from Labs MS → calculate `max_score`
    - Fetch lab details (type, template_path) from Labs MS
    - Call Lab API Service `POST /spawn`
    - Update session to `running` with pod info
    - Set 2-hour expiration timestamp

**Response (Success):**

```json
{
  "success": true,
  "data": {
    "session_id": "550e8400-e29b-41d4-a716-446655440000",
    "user_id": "...",
    "lab_id": "...",
    "status": "running",
    "container_id": "ctf-session-550e8400-...",
    "webshell_url": "wss://labs-api.altair.io/spawn/webshell/ctf-session-550e8400-...",
    "created_at": "2026-02-08T16:00:00Z",
    "expires_at": "2026-02-08T18:00:00Z"
  }
}
```

**Response (Error - Runtime Spawn Failed):**

```json
{
  "success": false,
  "error": {
    "code": "RUNTIME_SPAWN_FAILED",
    "message": "Failed to spawn lab runtime",
    "details": "Lab API Service returned error"
  }
}
```

---

#### **GET /sessions/:session_id**

Retrieve session details with runtime steps.

**Headers:**

- `x-altair-user-id` (required)

**Authorization:** Only session owner can access.

**Response:**

```json
{
  "success": true,
  "data": {
    "session": {
      "session_id": "...",
      "status": "running",
      "webshell_url": "wss://...",
      "created_at": "...",
      "expires_at": "..."
    },
    "steps": [
      {
        "step_number": 1,
        "title": "Reconnaissance",
        "description": "Explore the environment",
        "question": "What cron job did you find?",
        "validation_type": "exact_match",
        "points": 10,
        "hints": [
          {
            "hint_number": 1,
            "cost": 5,
            "text": "Check /etc/cron.d/"
          }
        ]
      }
    ]
  }
}
```

---

#### **DELETE /sessions/:session_id**

Stop a running session.

**Authorization:** Owner or admin only.

**Behavior:**

- Calls Lab API Service `POST /spawn/stop` to delete pod
- Updates session status to `stopped`

**Response:**

```json
{
  "success": true,
  "data": {
    "message": "Session stopped successfully"
  }
}
```

---

#### **GET /sessions/:session_id/progress**

Get current progress for a session.

**Authorization:** Owner only.

**Response:**

```json
{
  "success": true,
  "data": {
    "progress_id": "...",
    "session_id": "...",
    "current_step": 2,
    "completed_steps": [1],
    "hints_used": ["1_1"],
    "attempts_per_step": {"1": 3, "2": 1},
    "score": 45,
    "max_score": 100,
    "created_at": "..."
  }
}
```

---

#### **POST /sessions/:session_id/validate-step**

Validate an answer for a specific step.

**Request:**

```json
{
  "step_number": 1,
  "user_answer": "/opt/backup.sh"
}
```

**Authorization:** Owner only.

**Validation Logic:**

1. Must be validating `current_step` (linear progression enforced)
2. Increments `attempts_per_step` counter
3. Fetches step details from Labs MS
4. Validates according to `validation_type`:
    - `exact_match` – Compares to `expected_answer`
    - `contains` – Checks if answer contains `validation_pattern`
    - `regex` – Matches `validation_pattern` as regex
5. If correct:
    - Appends step to `completed_steps`
    - Increments `current_step`
    - Adds `points` to `score`

**Response (Correct):**

```json
{
  "success": true,
  "data": {
    "is_correct": true,
    "current_step": 2,
    "score": 55,
    "message": "Correct answer! Moving to next step."
  }
}
```

**Response (Incorrect):**

```json
{
  "success": true,
  "data": {
    "is_correct": false,
    "current_step": 1,
    "score": 45,
    "attempts": 4,
    "message": "Incorrect answer. Try again."
  }
}
```

---

#### **POST /sessions/:session_id/request-hint**

Request a hint for a specific step.

**Request:**

```json
{
  "step_number": 1,
  "hint_number": 1
}
```

**Authorization:** Owner only.

**Behavior:**

1. Checks if hint already used (key: `"{step}_{hint}"`)
2. Fetches hint from Labs MS
3. Applies cost: `score = max(score - cost, 0)`
4. Stores hint key in `hints_used`

**Response:**

```json
{
  "success": true,
  "data": {
    "hint_text": "Check /etc/cron.d/ for scheduled tasks",
    "cost": 5,
    "new_score": 40
  }
}
```

---

#### **POST /sessions/:session_id/complete**

Mark a session as complete.

**Authorization:** Owner only.

**Validation:**

- Must have completed ALL steps
- Checks `completed_steps` array length vs total steps

**Behavior:**

1. Verifies all steps completed
2. Stops runtime via Lab API Service (best-effort)
3. Updates session status to `stopped`
4. Returns final statistics

**Response:**

```json
{
  "success": true,
  "data": {
    "final_score": 85,
    "max_score": 100,
    "time_elapsed_seconds": 1847,
    "hints_used": 2,
    "total_attempts": 8,
    "completion_rate": 0.85
  }
}
```

---

#### **GET /sessions/user/:user_id**

List all sessions for a specific user.

**Authorization:** Only if `caller.user_id == :user_id`

**Response:**

```json
{
  "success": true,
  "data": [
    {
      "session_id": "...",
      "lab_id": "...",
      "status": "running",
      "created_at": "...",
      "score": 45
    }
  ]
}
```

---

#### **GET /sessions/lab/:lab_id**

List all sessions for a specific lab.

**Authorization:** Admin OR lab creator only.

**Behavior:**

- Fetches `creator_id` from Labs MS
- Allows if `caller.role == admin` OR `caller.user_id == creator_id`

**Response:**

```json
{
  "success": true,
  "data": [
    {
      "session_id": "...",
      "user_id": "...",
      "status": "running",
      "score": 45,
      "created_at": "..."
    }
  ]
}
```

---

#### **POST /internal/cron/expire** (Internal Endpoint)

Expire stale sessions older than 2 hours.

**Authorization:** Should be called by internal cron job only.

**Behavior:**

1. Selects all `running` sessions with `created_at + 2h < NOW()`
2. For each session:
    - Calls Lab API Service to stop runtime (best-effort)
    - Updates status to `expired`
    - Sets `expires_at` timestamp

**Response:**

```json
{
  "success": true,
  "data": {
    "expired_count": 3
  }
}
```

---

## Database Schema

### `lab_sessions` Table

| Column | Type | Constraints | Description |
| --- | --- | --- | --- |
| `session_id` | UUID | PRIMARY KEY | Session identifier |
| `user_id` | UUID | NOT NULL | User who created session |
| `lab_id` | UUID | NOT NULL | Lab being executed |
| `status` | TEXT | NOT NULL | Session status (lowercase) |
| `container_id` | TEXT | NULLABLE | Pod name from Lab API |
| `webshell_url` | TEXT | NULLABLE | WebSocket URL for terminal |
| `created_at` | TIMESTAMP | NOT NULL | Session creation timestamp |
| `expires_at` | TIMESTAMP | NULLABLE | Expiration timestamp (2h) |

**Status values:** `created`, `running`, `stopped`, `expired`, `error`

---

### `lab_progress` Table

| Column | Type | Constraints | Description |
| --- | --- | --- | --- |
| `progress_id` | UUID | PRIMARY KEY | Progress record identifier |
| `session_id` | UUID | NOT NULL, FK | Associated session |
| `current_step` | INT | NOT NULL | Current step number (starts at 1) |
| `completed_steps` | INT[] | NOT NULL | Array of completed step numbers |
| `hints_used` | JSONB | NOT NULL | Array of hint keys (e.g., `["1_1", "2_2"]`) |
| `attempts_per_step` | JSONB | NOT NULL | Map of step to attempt count |
| `score` | INT | NOT NULL | Current score |
| `max_score` | INT | NOT NULL | Maximum possible score |
| `created_at` | TIMESTAMP | NOT NULL | Progress creation timestamp |

**Example data:**

```json
{
  "current_step": 3,
  "completed_steps": [1, 2],
  "hints_used": ["1_1", "2_2"],
  "attempts_per_step": {"1": 3, "2": 1},
  "score": 85,
  "max_score": 100
}
```

---

## Project Structure

```
altair-sessions-ms/
├── Cargo.toml                    # Rust dependencies
├── Dockerfile                    # Multi-stage build
├── README.md                     # This file
├── .github/
│   └── workflows/
│       └── ci.yml               # CI pipeline
└── src/
    ├── main.rs                  # Server bootstrap, CORS, routes
    ├── state.rs                 # AppState (DB pool + services)
    ├── routes/
    │   ├── mod.rs              # Route declarations
    │   ├── health.rs           # Health check endpoint
    │   ├── sessions.rs         # Main session endpoints
    │   ├── internal.rs         # Internal cron endpoint
    │   ├── metrics.rs          # Metrics (not mounted)
    │   └── labs.rs             # Legacy file (not used)
    ├── services/
    │   ├── sessions_service.rs # Core session logic
    │   ├── extractor.rs        # Header extraction
    │   └── labs_client.rs      # Labs MS client
    ├── models/
    │   ├── session.rs          # Session data models
    │   ├── progress.rs         # Progress data models
    │   └── api.rs              # API response wrappers
    └── middleware/
        └── fake_auth.rs        # Dev middleware (obsolete)
```

---

## Deployment (Google Cloud Run)

The service is containerized and deployed to **Google Cloud Run** as an internal service.

### Container Configuration

- Listens on port `3003` (configurable via `PORT` env variable)
- Multi-stage Docker build optimizes image size
- Rust nightly toolchain for compilation

**⚠️ Dockerfile Issue:** Exposes port `3001` but service listens on `3003` (inconsistency).

### Runtime Requirements

- `DATABASE_URL` environment variable (Cloud SQL or external PostgreSQL)
- `LABS_MS_URL` – Internal URL for Labs microservice
- `LAB_API_URL` – Internal URL for Lab API Service
- Must be deployed in **private network** (no public access)

### Service Account Permissions

The Cloud Run service account requires:

- **Cloud Run Invoker** role for calling Labs MS and Lab API Service
- Network access to Cloud SQL (or external PostgreSQL)
- No special GCP API permissions required

### Scaling

- Auto-scales based on request load
- Cold start optimized with Rust's fast startup time
- Stateless design enables horizontal scaling

---

## Current CI/Security status (Feb 2026)

- CI runs on `develop` for the main workflow.
- CodeQL currently reports **critical findings** that must be fixed before tightening security gates.

## Known Issues & Limitations

### 🔴 Critical Issues

- **Dockerfile port mismatch** – Exposes `3001` but listens on `3003`
- **Fake auth middleware obsolete** – Present in `middleware/fake_auth.rs` but not mounted in `main.rs`; code relies on Gateway-injected headers.
- **Test coverage mismatches** – Tests are not fully aligned with the current routes / some routes are not covered (à confirmer au cas par cas plutôt que d’affirmer un endpoint précis).

### 🟡 Operational Gaps

- **No retry logic** – Failed Lab API calls are not retried
- **Best-effort runtime cleanup** – Stopped sessions may leave orphaned pods
- **Linear progression only** – Cannot skip or retry previous steps
- **Metrics endpoint not mounted** – `metrics.rs` exists but is not wired into the router
- **Fragile dependency on Labs MS** – Lab-specific listing authorization depends on calling Labs MS (no cache); if Labs MS is down, this path fails.

### 🟡 Business Logic Limitations

- **Start-or-resume is idempotent (no explicit resume endpoint)** – This is a deliberate behavior choice, but it can be confusing to name it “start-or-resume” without a separate route.
- **No session pause** – Sessions are either running or stopped
- **2-hour hard limit** – No extension mechanism
- **Hint cost cannot be negative** – Score floors at 0 (business rule, not a bug).

---

## CI/CD Pipeline

GitHub Actions workflow (`.github/workflows/ci.yml`):

1. **Format Check** – `cargo fmt --check`
2. **Linting** – `cargo clippy -D warnings`
3. **Tests** – `cargo test`
4. **Release Build** – `cargo build --release`

---

## Project Status

**✅ Current Status: MVP (Minimum Viable Product)**

This microservice is **functional for MVP deployment** with core session lifecycle and progress tracking operational. Some operational gaps and test coverage remain before production-ready status.

**Known limitations to address for production:**

1. Fix Dockerfile port mismatch (3001 vs 3003)
2. Remove obsolete fake auth middleware
3. Add retry logic for Lab API calls
4. Implement comprehensive error recovery
5. Mount metrics endpoint
6. Update test suite to match actual routes

**Maintainers:** Altaïr Platform Team

---

## Notes

- **Start-or-resume behavior** – Existing sessions are reused automatically
- **Linear progression enforced** – Must complete steps in order
- **2-hour expiration** – Sessions expire after 2 hours from creation
- **Best-effort cleanup** – Runtime pods may require manual cleanup
- **Trust model** – Relies on Gateway for authentication

---

## License

Internal Altaïr Platform Service – Not licensed for external use.
## May 2026 Security And Platform Updates

- Runtime Docker image now installs only required packages with `--no-install-recommends` and runs as non-root UID `10001`.
- CORS origin handling is now allowlist-based through `ALLOWED_ORIGINS`; local defaults are `http://localhost:5173,http://localhost:3000`.
- The service now respects the `PORT` environment variable, with `3003` as local fallback.
- Upstream URLs such as `LABS_MS_URL`, `LAB_API_URL`, and `GROUPS_MS_URL` should be supplied by environment configuration per deployment.
- Latest Trivy scan status for this repo: no HIGH or CRITICAL findings.
