# ── deps: production dependencies only ───────────────────────────────────────
FROM node:20.19.1-alpine3.21 AS deps
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev

# ── builder: compile TypeScript ───────────────────────────────────────────────
FROM node:20.19.1-alpine3.21 AS builder
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
RUN npm run build

# ── runner: minimal production image ─────────────────────────────────────────
FROM node:20.19.1-alpine3.21 AS runner
WORKDIR /app
ENV NODE_ENV=production

# Non-root user
RUN addgroup -S appgroup && adduser -S appuser -G appgroup
COPY --from=deps  --chown=appuser:appgroup /app/node_modules ./node_modules
COPY --from=builder --chown=appuser:appgroup /app/dist ./dist
COPY --from=builder --chown=appuser:appgroup /app/package.json ./package.json

USER appuser
EXPOSE 3001
CMD ["node", "dist/index.js"]
