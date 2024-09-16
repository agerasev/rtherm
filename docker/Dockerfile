FROM rust:1.81

RUN apt-get update && \
    apt-get install -y \
        libpq-dev \
        libsqlite3-dev \
    && \
    apt-get clean

RUN cargo install diesel_cli \
    --no-default-features \
    --features=postgres,sqlite
