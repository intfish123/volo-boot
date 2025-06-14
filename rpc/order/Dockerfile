FROM rust:1.87.0-alpine3.21 AS builder
WORKDIR /usr/src/app
RUN set -eux && sed -i 's/dl-cdn.alpinelinux.org/mirrors.aliyun.com/g' /etc/apk/repositories
RUN apk update && apk add --no-cache wget alpine-sdk ca-certificates bash sudo pkgconf pkgconfig protobuf musl-dev openssl openssl-dev

RUN rustup component add rustfmt

ENV PATH=/usr/bin:/usr/local/bin:$PATH
ENV LIBRARY_PATH=/lib:/usr/lib:/usr/local/lib
ENV LD_LIBRARY_PATH=/lib:/usr/lib:/usr/local/lib
ENV OPENSSL_DIR=/usr/bin/openssl
ENV OPENSSL_LIB_DIR=/usr/lib
ENV OPENSSL_INCLUDE_DIR=/usr/include/openssl
ENV RUST_BACKTRACE=1
RUN echo "$LIBRARY_PATH"
RUN echo "$LD_LIBRARY_PATH"

RUN rustc --version && \
    rustfmt --version && \
    cargo --version

COPY cargo-source.toml $CARGO_HOME/config.toml

COPY . .
RUN cargo install -v --path .


FROM alpine:3.21
WORKDIR /
RUN set -eux && sed -i 's/dl-cdn.alpinelinux.org/mirrors.aliyun.com/g' /etc/apk/repositories
RUN apk update && apk add --no-cache nmap nmap-scripts wget alpine-sdk ca-certificates git bash sudo tzdata pkgconf pkgconfig
COPY --from=builder /usr/local/cargo/bin/server /app

ENV TZ=Asia/Shanghai
CMD ["/app", "--config", "/config/app_config.toml"]