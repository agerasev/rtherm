#!/usr/bin/bash

SERVER_DIR="$(dirname $(realpath -s $0))/../"

if [ "$1" == "--debug" ]; then
  TARGET_DIR="debug"
else
  BUILD_ARGS="$BUILD_ARGS --release"
  TARGET_DIR="release"
fi

cd $SERVER_DIR && \
cargo build \
    $BUILD_ARGS \
    --no-default-features \
    --features=postgres,telegram \
&& \
mkdir -p ./docker/bin/ && \
cp ../target/$TARGET_DIR/rtherm-server ./docker/bin/ && \
docker build \
    -t rtherm-server:latest \
    -f docker/Dockerfile \
    . \
&& \
echo "Done!"
