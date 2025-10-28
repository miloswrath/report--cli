#!/usr/bin/env bash

set -euo pipefail

TARGET="${1:-aarch64-apple-darwin}"
BIN_NAME="report-builder"

echo "Building $BIN_NAME release binary for target ${TARGET}..."
cargo build --release --target "${TARGET}"

VERSION="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; data=json.load(sys.stdin); pkg=next(p for p in data["packages"] if p["name"]=="report-builder"); print(pkg["version"])')"

DIST_DIR="target/dist"
INSTALL_IMAGE="${DIST_DIR}/${BIN_NAME}-${VERSION}-${TARGET}"

rm -rf "${INSTALL_IMAGE}"
mkdir -p "${INSTALL_IMAGE}"

cp "target/${TARGET}/release/${BIN_NAME}" "${INSTALL_IMAGE}/"

pushd "${DIST_DIR}" >/dev/null
ARCHIVE_NAME="${BIN_NAME}-${VERSION}-${TARGET}.tar.gz"
tar -czf "${ARCHIVE_NAME}" "$(basename "${INSTALL_IMAGE}")"
popd >/dev/null

echo "Created archive target/dist/${ARCHIVE_NAME}"
echo "Contents staged under ${INSTALL_IMAGE}"
