FROM rustlang/rust:nightly as builder

ARG package=k8s-udev-device-manager

COPY Cargo.lock /src/Cargo.lock
WORKDIR /src
# Update index
RUN --mount=type=cache,target=/src/obj --mount=type=cache,target=$CARGO_HOME \
  cargo install lazy_static >/dev/null 2>/dev/null || true

COPY . /src
RUN --mount=type=cache,target=/src/obj --mount=type=cache,target=$CARGO_HOME \
  cargo build --package ${package} --release --locked --bin ${package} --target-dir /src/obj \
  # && ls -R /src/obj \
  && cp /src/obj/release/${package} /src/${package}
# RUN --mount=type=cache,target=/src/target ls -la target && ls -la target/release && exit 1

FROM debian:buster-slim
ARG APP=/usr/src/${package}

RUN apt-get update \
  && apt-get install -y ca-certificates tzdata \
  && rm -rf /var/lib/apt/lists/*

EXPOSE 8080
ENV TZ=Etc/UTC \
  APP_USER=appuser

RUN groupadd $APP_USER \
  && useradd -g $APP_USER $APP_USER \
  && mkdir -p ${APP}

COPY --from=builder /src/${package} ${APP}/${package}
RUN chown -R $APP_USER:$APP_USER ${APP}

USER $APP_USER
WORKDIR ${APP}

CMD ["./${package}"]
