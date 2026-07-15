# syntax=docker/dockerfile:1
#
# Production image for the Tokens frontend (Next.js 16 on Bun).
# The frontend has no local workspace dependencies, so it builds
# standalone — the build context is just packages/frontend.

# --- deps: install node_modules (incl. devDeps so drizzle-kit is available
#     at runtime for migrations) -------------------------------------------
FROM oven/bun:1 AS deps
WORKDIR /app
COPY package.json ./
RUN bun install

# --- builder: produce the Next.js production build ------------------------
FROM oven/bun:1 AS builder
WORKDIR /app
# NEXT_PUBLIC_* is inlined into the client bundle at build time, so the public
# URL must be known here (e.g. ProfileEmbedDialog embeds tokens.ci links).
ARG NEXT_PUBLIC_URL=https://tokens.ci
ENV NEXT_PUBLIC_URL=$NEXT_PUBLIC_URL
ENV NEXT_TELEMETRY_DISABLED=1
COPY --from=deps /app/node_modules ./node_modules
COPY . .
RUN bun run build

# --- runner: serve with `next start` --------------------------------------
FROM oven/bun:1 AS runner
WORKDIR /app
ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1
ENV PORT=3000
# Bind inside the container to all interfaces; host exposure is restricted to
# 127.0.0.1 by the compose port mapping.
ENV HOSTNAME=0.0.0.0

COPY --from=builder /app/node_modules ./node_modules
COPY --from=builder /app/.next ./.next
COPY --from=builder /app/public ./public
COPY --from=builder /app/package.json ./package.json
COPY --from=builder /app/next.config.ts ./next.config.ts
COPY --from=builder /app/drizzle.config.ts ./drizzle.config.ts
COPY --from=builder /app/src ./src
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["bun", "run", "start"]
