#!/bin/sh
# Builds this app and packages it as target/tab/my-app.tab. elf2tab 0.13.0 is necessary.
# `--kernel-minor 3`: the Tock release-2.3.0 kernel reports version 2.3, and its loader skips
# an app that requires a later version.
set -eu
cd "$(dirname "$0")"
cargo build --release
mkdir -p target/tab
elf2tab --kernel-major 2 --kernel-minor 3 \
    --package-name my-app \
    --stack 16384 --minimum-footer-size 256 \
    --output-file target/tab/my-app.tab \
    "target/thumbv7em-none-eabi/release/my-app,cortex-m4"
