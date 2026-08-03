# syntax=docker/dockerfile:1
#
# Production image for the self-hosted deployment.
#
# Built in CI, never on the server: the target box is 2 vCPU / 3.8 GB with a
# 256 MB swapfile, and `next build` there would either be OOM-killed or spend
# the whole build competing with Postgres and the live site for the same two
# cores. Actions builds it, GHCR stores it, the server only pulls.
#
# Build context is the repository ROOT, not web/ — web's build script copies
# ../install.sh and ../.github/assets/client-* into public/ before compiling.

# --- deps: resolve the workspace from the lockfile -------------------------
# Only the manifests land here, so this layer survives every commit that is not
# a dependency change — which is nearly all of them.
FROM oven/bun:1 AS deps
WORKDIR /repo
COPY package.json bun.lock ./
COPY web/package.json ./web/
# 56 KB of npm wrappers for the CLI binaries. The web app depends on none of
# them, but they are workspace members, so the install fails to resolve without
# their manifests present.
COPY packages ./packages
RUN bun install --frozen-lockfile

# --- builder: produce the standalone server --------------------------------
FROM oven/bun:1 AS builder
WORKDIR /repo
ENV NEXT_TELEMETRY_DISABLED=1
# Selects `output: "standalone"` and skips the Cloudflare dev hook in
# next.config.ts. The Workers build is untouched by this file.
ENV BUILD_TARGET=node
# NEXT_PUBLIC_* is inlined into the client bundle at compile time, so the public
# origin has to be known here rather than at run time.
ARG NEXT_PUBLIC_URL=https://tokens.ci
ENV NEXT_PUBLIC_URL=$NEXT_PUBLIC_URL

COPY --from=deps /repo ./
COPY . .
RUN cd web && bun run build

# --- runner ----------------------------------------------------------------
FROM node:24-bookworm-slim AS runner
WORKDIR /app
ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1 \
    PORT=3000 \
    HOSTNAME=0.0.0.0

# Postgres is a sibling container and Caddy terminates TLS on the host, so the
# only thing this process ever needs to reach out for is the GitHub API.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --create-home --uid 10001 nextjs

# `output: standalone` traces the module graph and emits a server carrying only
# what it actually imports — 68 MB against 1.3 GB of installed node_modules.
# The workspace root sits one level above the app, so the trace is rooted at
# /repo and the entry point lands at web/server.js.
COPY --from=builder --chown=nextjs:nextjs /repo/web/.next/standalone ./
COPY --from=builder --chown=nextjs:nextjs /repo/web/.next/static ./web/.next/static
COPY --from=builder --chown=nextjs:nextjs /repo/web/public ./web/public

# Migrations run from this same image (see compose), which keeps the schema and
# the code that assumes it on exactly the same versions. What the trace leaves
# behind is not enough for them on its own: it keeps only the module graph the
# *application* imports, so `drizzle-orm/postgres-js/migrator` is absent, and
# `postgres` is inlined into the server chunks rather than left resolvable under
# node_modules. Both are restored whole here — a few MB, against maintaining a
# second image whose dependency versions could drift from the app's.
COPY --from=builder --chown=nextjs:nextjs /repo/node_modules/drizzle-orm ./node_modules/drizzle-orm
COPY --from=builder --chown=nextjs:nextjs /repo/node_modules/postgres ./node_modules/postgres
COPY --from=builder --chown=nextjs:nextjs /repo/web/src/lib/db/migrations ./migrations
COPY --from=builder --chown=nextjs:nextjs /repo/web/scripts/migrate.mjs ./migrate.mjs
COPY --from=builder --chown=nextjs:nextjs /repo/web/scripts/cron.mjs ./cron.mjs

USER nextjs
EXPOSE 3000

# Compose overrides this for the one-shot migration container.
CMD ["node", "web/server.js"]
