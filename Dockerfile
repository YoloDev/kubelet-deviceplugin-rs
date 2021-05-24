FROM rustlang/rust:nightly as builder

ARG package=k8s-udev-device-manager

RUN apt-get update \
  && apt-get install -y libudev-dev \
  && rm -rf /var/lib/apt/lists/*

COPY Cargo.lock /src/Cargo.lock
WORKDIR /src
# Update index
RUN cargo install lazy_static >/dev/null 2>/dev/null || true

COPY . /src
RUN cargo build --package ${package} --release --locked --bin ${package} --target-dir /src/obj \
  && cp /src/obj/release/${package} /src/${package}

FROM debian:buster-slim
ARG APP=/usr/src/${package}

RUN apt-get update \
  && apt-get install -y ca-certificates tzdata libudev1 \
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
