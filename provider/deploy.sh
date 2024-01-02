#!/usr/bin/bash

cd $(dirname "$0") && \
cargo build --release --target=aarch64-unknown-linux-gnu && \
scp ../target/aarch64-unknown-linux-gnu/release/rtherm-provider 10.4.0.10:develop/rtherm/ && \
ssh 10.4.0.10 "./develop/rtherm/rtherm-provider"
