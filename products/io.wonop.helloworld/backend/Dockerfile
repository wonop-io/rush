FROM debian:bookworm-slim
ARG APP=/usr/src/app

ENV TZ=Etc/UTC \
    APP_USER=appuser

# Install OpenSSL and required libraries for runtime
RUN apt-get update \
    && apt-get install -y ca-certificates tzdata openssl libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create appuser
RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP}

COPY ./server/target/x86_64-unknown-linux-gnu/release/server /server

RUN chown -R $APP_USER:$APP_USER ${APP}

USER $APP_USER
WORKDIR ${APP}

EXPOSE 8000

CMD ["/server"]
