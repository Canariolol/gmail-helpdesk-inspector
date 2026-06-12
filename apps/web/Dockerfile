FROM node:20-bookworm AS builder
WORKDIR /app
COPY apps/web/package*.json ./
RUN npm install
COPY apps/web .
ARG VITE_API_BASE_URL=http://localhost:8080
ENV VITE_API_BASE_URL=$VITE_API_BASE_URL
RUN npm run build

FROM node:20-bookworm-slim
WORKDIR /app
COPY --from=builder /app/dist dist
COPY apps/web/server.mjs server.mjs
EXPOSE 5173
CMD ["node", "server.mjs"]
