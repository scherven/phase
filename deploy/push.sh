#!/usr/bin/env bash
set -euo pipefail

# Quick-deploy phase-server to VPS for testing.
# Cross-compiles via cargo-zigbuild, SCPs to VPS, restarts service.

HOST="phase-vps"

echo "Building phase-server for linux x86_64..."
cargo zigbuild --release --bin phase-server --target x86_64-unknown-linux-gnu

echo "Uploading..."
scp target/x86_64-unknown-linux-gnu/release/phase-server "${HOST}:/tmp/phase-server"

echo "Deploying..."
ssh "${HOST}" "\
  sudo systemctl stop phase-server || true \
  && sudo cp /tmp/phase-server /opt/phase-server/phase-server \
  && sudo chmod +x /opt/phase-server/phase-server \
  && sudo chown phase:phase /opt/phase-server/phase-server \
  && sudo systemctl start phase-server \
  && rm -f /tmp/phase-server \
  && echo 'Service status:' \
  && sudo systemctl is-active phase-server"

echo "Done — phase-server deployed to ${HOST}"
