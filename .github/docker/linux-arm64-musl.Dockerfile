FROM golang:1.26.8-alpine3.23@sha256:9bba48a62950bb149df18491ed0a7ba9a8edf38907e7a5bb53c1972df2b20042 AS go-toolchain
FROM rust:1.96.1-alpine3.23@sha256:b38e527af9cd485d62885dcf64a7842e8d127e94d8b43fa6dc67347675a94d65 AS build

RUN apk add --no-cache alsa-lib-dev bash binutils build-base chafa-dev glib-dev pkgconf tar wayland-dev
COPY --from=go-toolchain /usr/local/go /usr/local/go
ENV PATH="/usr/local/go/bin:$PATH" \
    GOTOOLCHAIN=local \
    CGO_ENABLED=1 \
    CARGO_INCREMENTAL=0 \
    CARGO_PROFILE_TEST_DEBUG=0

WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml THIRD_PARTY_NOTICES.md ./
COPY assets/ assets/
COPY src/ src/
COPY tests/ tests/
COPY whatsrust/ whatsrust/
COPY .github/scripts/build-linux-arm64-musl.sh /usr/local/bin/build-linux-arm64-musl
RUN bash /usr/local/bin/build-linux-arm64-musl

FROM alpine:3.23 AS smoke
RUN apk add --no-cache alsa-lib ca-certificates chafa glib libgcc libstdc++ wayland-libs-client
COPY --from=build /dist/ /dist/
WORKDIR /smoke
RUN --network=none tar -xzf /dist/wptui-aarch64-unknown-linux-musl.tar.gz && ./wp-tui --version

FROM scratch AS artifact
COPY --from=smoke /dist/ /
