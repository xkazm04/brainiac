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
