#!/bin/bash
set -e

PACKAGE_NAME=$1
PACKAGE_DIR=$2
BUILD_SDIST=${3:-false}

echo "Building $PACKAGE_NAME in $PACKAGE_DIR"

if [ -f "$PACKAGE_DIR/Cargo.toml" ]; then
  echo "Detected Rust package (maturin)"
  cd "$PACKAGE_DIR"
  if [ "$BUILD_SDIST" = "true" ]; then
    uvx maturin build --release --sdist --out ../../dist
  else
    uvx maturin build --release --out ../../dist
  fi
else
  echo "Detected Python package (pyproject-build)"
  cd "$PACKAGE_DIR"
  uvx --from build pyproject-build --installer uv --outdir ../../dist .
fi
