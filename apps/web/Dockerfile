# Build stage
FROM node:20-alpine AS build
ARG HTTP_PROXY
ARG HTTPS_PROXY
ARG ALL_PROXY
ENV HTTP_PROXY=$HTTP_PROXY HTTPS_PROXY=$HTTPS_PROXY ALL_PROXY=$ALL_PROXY
WORKDIR /workspace
COPY apps/web/package.json apps/web/package-lock.json* ./apps/web/
WORKDIR /workspace/apps/web
RUN npm ci --no-audit --no-fund
WORKDIR /workspace
COPY apps/web ./apps/web
COPY packages ./packages
WORKDIR /workspace/apps/web
RUN npm run build

# Production stage
FROM nginx:alpine
COPY --from=build /workspace/apps/web/dist /usr/share/nginx/html
COPY apps/web/nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
