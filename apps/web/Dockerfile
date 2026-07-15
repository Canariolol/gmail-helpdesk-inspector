FROM node:20-bookworm AS builder
WORKDIR /app
COPY apps/web/package*.json ./
RUN npm ci
COPY apps/web .
ARG VITE_API_BASE_URL=http://localhost:8080
ARG VITE_MERCADOPAGO_PUBLIC_KEY
ENV VITE_API_BASE_URL=$VITE_API_BASE_URL
ENV VITE_MERCADOPAGO_PUBLIC_KEY=$VITE_MERCADOPAGO_PUBLIC_KEY
RUN npm run build

FROM node:20-bookworm-slim
WORKDIR /app
COPY --chown=node:node --from=builder /app/dist dist
COPY --chown=node:node apps/web/server.mjs server.mjs
USER node
EXPOSE 5173
CMD ["node", "server.mjs"]
