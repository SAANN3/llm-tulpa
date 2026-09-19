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
COPY --from=builder /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
