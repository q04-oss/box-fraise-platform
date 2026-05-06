#!/bin/bash
# Generate Ed25519 signing key for Box Fraise soultoken signing
# Add output to Railway environment variables
set -e
SIGNING_KEY_HEX=$(openssl rand -hex 32)
echo "SOULTOKEN_SIGNING_KEY_HEX=$SIGNING_KEY_HEX"
echo ""
echo "SOULTOKEN_VERIFYING_KEY_HEX will be logged at server"
echo "startup — copy it from the startup logs after first run."
