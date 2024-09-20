#!/usr/bin/bash

cd $(dirname "$0")/../ && \
cargo build --release --target=aarch64-unknown-linux-gnu && \
rsync -aP ../target/aarch64-unknown-linux-gnu/release/rtherm-client 10.4.0.10:/opt/rtherm/ && \
rsync -aP ./config/ 10.4.0.10:/opt/rtherm/config/ && \
echo "Done!"
