# Brainiac console — Next.js standalone output (next.config.ts already sets
# output: "standalone", so the runtime image carries only the traced deps).

FROM node:22-slim AS deps
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci

FROM node:22-slim AS build
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY . .
ENV NEXT_TELEMETRY_DISABLED=1
# ── Sentry wiring — MUST be set before `npm run build` ──────────────────────
# Next.js substitutes `NEXT_PUBLIC_*` at BUILD time into both the browser and
# the server bundles. Passing these at container runtime does nothing: by then
# the reads in instrumentation-client.ts / sentry.{server,edge}.config.ts have
# already been compiled to a literal. Without these ARGs they compiled to
# `undefined`, so every `if (dsn)` was dead code and console error tracking
# could not work no matter what the compose file set. Both stay OPTIONAL —
# unset means the empty string, `if (dsn)` stays false, and Sentry is simply
# never initialized (the local/dev posture: never phone home).
ARG NEXT_PUBLIC_SENTRY_DSN=""
ARG NEXT_PUBLIC_BRAINIAC_ENV=""
ENV NEXT_PUBLIC_SENTRY_DSN=$NEXT_PUBLIC_SENTRY_DSN
ENV NEXT_PUBLIC_BRAINIAC_ENV=$NEXT_PUBLIC_BRAINIAC_ENV
# The build must not need the API — pages are force-dynamic and every fetch
# has a demo fallback.
RUN npm run build

FROM node:22-slim AS runtime
WORKDIR /app
ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1
ENV PORT=3100
ENV HOSTNAME=0.0.0.0
COPY --from=build /app/.next-build/standalone ./
COPY --from=build /app/.next-build/static ./.next-build/static
EXPOSE 3100
CMD ["node", "server.js"]
