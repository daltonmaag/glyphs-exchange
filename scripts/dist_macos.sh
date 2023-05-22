#!/bin/sh

cargo build --locked --release --target aarch64-apple-darwin
cargo build --locked --release --target x86_64-apple-darwin

mkdir -p target/universal
lipo -create -output target/universal/glyphs-exchange \
    target/aarch64-apple-darwin/release/glyphs-exchange \
    target/x86_64-apple-darwin/release/glyphs-exchange

/usr/local/bin/packagesbuild --package-version $(python scripts/extract-version.py) glyphs-exchange.pkgproj
