# syntax=docker/dockerfile:1.7

FROM rust:1.88-alpine AS calmserve-build

RUN apk add --no-cache build-base cmake perl

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo build --locked --release && \
    cp target/release/calmserve /tmp/calmserve

FROM nginx:1.27-alpine

RUN apk add --no-cache su-exec tini

ENV NGINX_ENVSUBST_TEMPLATE_DIR=/etc/nginx/templates
ENV NGINX_ENVSUBST_OUTPUT_DIR=/etc/nginx/conf.d
ENV GEMINI_CERTIFICATE_DIRECTORY=/var/lib/calmserve/certificates
ENV GEMINI_LISTEN=0.0.0.0:1965
ENV CALMSERVE_ROOT=/srv/calmserve
ENV SPARTAN_LISTEN=0.0.0.0:3000

COPY --from=calmserve-build /tmp/calmserve /usr/local/bin/calmserve
COPY --chmod=755 generate-status-badge.sh /usr/local/bin/generate-status-badge
COPY --chmod=755 start.sh /usr/local/bin/start-calmserve

EXPOSE 80 1965 3000

ENTRYPOINT ["/sbin/tini", "--", "/usr/local/bin/start-calmserve"]
