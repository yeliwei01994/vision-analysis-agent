# PostgreSQL JSONB Migration Design

## Purpose and scope

Replace the local-development MySQL database with PostgreSQL 16. The system will start against an empty PostgreSQL database; migrating existing MySQL records is explicitly out of scope. The change preserves the Rust API contract, frontend contract, Redis queue behavior, and YOLO HTTP request/response schemas.

PostgreSQL is selected because the project stores structured but evolving YOLO-derived detections, evidence, model analysis, and rule geometry. `JSONB` retains that structure without serializing it into text, supports validation at write time, and can be indexed when product queries require it. It does not change YOLO itself: YOLO continues to produce JSON over its existing API.

## Architecture

```text
React frontend -> Axum API / Worker -> PostgreSQL 16
                                  -> Redis
                                  -> YOLO service
```

The API and Worker each receive one `DATABASE_URL` pointing to the `postgres` Compose service. The Rust persistence boundary remains `backend/src/persistence.rs`; no database-specific behavior leaks into the domain, HTTP, queue, or YOLO modules.

## Database schema

Replace the MySQL-only initial migration with a PostgreSQL initial schema in `db/migrations`. Since this is a new empty database, a new clean baseline is preferable to cross-engine conditional migrations.

- Use native `UUID` for `video_jobs.id`, `events.id`, `events.job_id`, review IDs, and related foreign keys.
- Use `TIMESTAMPTZ` with `CURRENT_TIMESTAMP` defaults for timestamps.
- Preserve existing business columns, status strings, default rule seed, and delete/cascade behavior.
- Use `JSONB` for `events.objects_json`, `events.evidence_json`, `events.analysis_json`, and `event_rules.geometry_json`.
- Keep commonly filtered scalar values (`event_type`, `status`, `confidence`, time fields, job status) as columns with B-tree indexes.
- Add GIN indexes only to JSONB data that has a defined near-term query path: event objects and rule geometry. This allows containment queries such as a detection object class without requiring schema changes. Do not add speculative expression indexes for unknown YOLO payload shapes.

## Persistence implementation

Switch SQLx from its `mysql` feature and types to `postgres`, using `PgPool`, `PgPoolOptions`, and `PgRow`. SQL statements use PostgreSQL positional parameters (`$1`, `$2`, ...).

The migration translates SQL dialect behavior while preserving semantics:

- Job saves use `INSERT ... ON CONFLICT (id) DO UPDATE` and `EXCLUDED` values. The existing attempt/terminal-status guard is expressed with `CASE` and `GREATEST`, so stale worker writes still cannot overwrite a terminal row.
- Event and rule saves use `ON CONFLICT (...) DO UPDATE`.
- MySQL timestamp expressions become PostgreSQL epoch-millisecond expressions, while review timestamps are formatted as ISO-8601 strings or decoded directly as timestamp values according to the existing domain contract.
- `CURRENT_TIMESTAMP`, `FOR UPDATE`, `LIMIT`, cascades, and affected-row behavior use their PostgreSQL equivalents.
- JSON values are bound as `sqlx::types::Json<serde_json::Value>` (or `Option<Json<_>>`) and read back through the same mapping. No JSON column is encoded into a string before persistence.

## Runtime configuration and deployment

Update `docker-compose.yml` to use a `postgres:16` service called `postgres`. It receives `POSTGRES_DB`, `POSTGRES_USER`, and `POSTGRES_PASSWORD`, stores data in `postgres_data:/var/lib/postgresql/data`, and uses `pg_isready` for health checks. API and Worker dependencies and URLs point to `postgres`.

Update `.env.example` to document `POSTGRES_HOST`, `POSTGRES_PORT`, `POSTGRES_DATABASE`, `POSTGRES_USER`, `POSTGRES_PASSWORD`, and a `postgres://` `DATABASE_URL`. Existing `mysql_data` is not deleted or transformed by this change; it is deliberately left untouched so operators can retain it until they dispose of it themselves.

Change user-facing logs and active operational docs from MySQL terminology to PostgreSQL. Historical design records remain historical unless they are active runbooks used by this repository.

## Failure handling

The existing startup behavior remains: if the database URL is absent or PostgreSQL cannot be reached, the API/Worker report the failure and use the current in-memory fallback where configured. A failed PostgreSQL write remains an error; it must not be treated as a successful persisted operation.

## Testing and verification

Tests are updated to use PostgreSQL pool types and parameter syntax. The migration adds coverage for:

1. JSONB round-tripping of YOLO-shaped detections, evidence, and nullable analysis.
2. A JSONB containment query backed by the selected GIN index.
3. Existing job terminal-state write protection and retry behavior on PostgreSQL.
4. Existing rule and event persistence behavior.

Verification runs the focused backend tests first, then the repository checks: `cargo test`, frontend tests and build, and `docker compose config`. When Docker is available, Compose is started against an empty `postgres_data` volume and the migration plus JSONB persistence tests are exercised with a PostgreSQL `DATABASE_URL`.

## Explicit non-goals

- No MySQL-to-PostgreSQL data export/import utility.
- No YOLO API, model, or inference-service changes.
- No frontend API contract changes.
- No storage of video/image binaries in PostgreSQL.
- No generic JSONB indexes for undefined future query patterns.
