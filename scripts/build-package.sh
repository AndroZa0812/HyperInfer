#!/bin/bash
set -e

PACKAGE_NAME=$1
PACKAGE_DIR=$2

echo "Building $PACKAGE_NAME in $PACKAGE_DIR"

if [ -f "$PACKAGE_DIR/Cargo.toml" ]; then
  echo "Detected Rust package (maturin)"
  # Assuming maturin for Rust/Python projects
  cd "$PACKAGE_DIR"
  uvx maturin build --release --out ../../dist
else
  echo "Detected Python package (pyproject-build)"
  cd "$PACKAGE_DIR"
  uvx --from build pyproject-build --installer uv --outdir ../../dist .
fi
