FROM debian:12.7
RUN apt-get update && apt-get install -y libssl3 && rm -rf /var/lib/apt/lists/*
# This Dockerfile does not actually build `rtherm-server` binary.
# Instead pre-build binary should be put in `server/docker/bin/` directory.
WORKDIR /opt/
COPY ./docker/bin/rtherm-server /opt/
COPY ./static /opt/static
COPY ./config /opt/config
CMD /opt/rtherm-server /opt/config/$RTHERM_CONFIG
