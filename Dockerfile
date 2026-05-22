ARG RUST_VERSION=1
FROM rust:"$RUST_VERSION-alpine" AS build

RUN --mount=type=cache,target=/var/cache/apk \
  apk add git

WORKDIR /app
RUN \
  --mount=type=bind,source=.git,target=.git \
  --mount=type=bind,source=src,target=src \
  --mount=type=bind,source=build.rs,target=build.rs \
  --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
  --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
  --mount=type=bind,source=README.md,target=README.md \
  --mount=type=bind,source=schemas,target=schemas \
  --mount=type=cache,target=/usr/local/cargo/registry/index \
  --mount=type=cache,target=/usr/local/cargo/registry/cache \
  --mount=type=cache,target=/usr/local/cargo/git/db \
  --mount=type=cache,target=/app/target \
  <<RUN
  set -ex
  cargo build --locked --release
  cp target/release/ocppsim /bin/ocppsim
RUN

FROM scratch
COPY --from=build /bin/ocppsim /bin/ocppsim
ENTRYPOINT ["/bin/ocppsim"]
