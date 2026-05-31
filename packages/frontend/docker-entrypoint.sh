#!/bin/sh
set -e

# Postgres is guaranteed healthy by compose `depends_on`, so we can migrate
# immediately. drizzle-kit migrate is idempotent (tracked via the journal).
echo "[entrypoint] Applying database migrations..."
bun run db:migrate

echo "[entrypoint] Starting Next.js..."
exec "$@"
