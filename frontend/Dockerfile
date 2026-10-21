FROM node:22-bookworm-slim AS builder
WORKDIR /app

COPY package.json package-lock.json ./
RUN npm ci

COPY . .

# Baked in at build time (Vite env vars are compile-time) and empty by default —
# `config.ts` falls back to whatever host the page itself was loaded from when this
# is unset, which is right for the common case (backend and frontend served from the
# same machine). Only set this if the backend actually lives somewhere else.
ARG VITE_BACKEND_URL=
ENV VITE_BACKEND_URL=$VITE_BACKEND_URL
RUN npm run build

FROM nginx:alpine
# openssl: the https certificate is made here on first start (docker-entrypoint.d/40-tls.sh)
RUN apk add --no-cache openssl
COPY --from=builder /app/dist /usr/share/nginx/html
# The nginx image turns this into conf.d/default.conf at start, filling in BACKEND_ADDR from the environment
COPY nginx.conf.template /etc/nginx/templates/default.conf.template
# Not `.conf`: it is a piece of a server, not a file of its own, so nginx does not read it by itself; both servers include it
COPY api-proxy.inc.template /etc/nginx/templates/api-proxy.inc.template
COPY docker-entrypoint.d/40-tls.sh /docker-entrypoint.d/40-tls.sh
# Where the backend answers as seen from this container (the host, since the backend runs on it or on its network).
# Both servers forward /api there (see nginx.conf.template).
ENV BACKEND_ADDR=host.docker.internal:3000

EXPOSE 80 443
