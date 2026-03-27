# Build stage
FROM oven/bun:1-alpine AS builder

WORKDIR /app

# Install dependencies
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile

# Copy source and build
COPY . .
RUN bun run build

# Runtime stage
FROM oven/bun:1-alpine

WORKDIR /app

# Copy the build artifacts and package manifest
COPY --from=builder /app/build ./build
COPY --from=builder /app/package.json ./package.json
COPY --from=builder /app/bun.lock ./bun.lock

# Install only production dependencies
RUN bun install --production

# Export port
EXPOSE 5173

# Env vars for runtime
ENV NODE_ENV=production
ENV PORT=5173
ENV BACKEND_URL=http://nmcscan:3000
ENV AUTH_SECRET=""

# Start server using Bun
CMD ["bun", "build/index.js"]
