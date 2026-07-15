#!/usr/bin/env bash
set -eu

##############################################################################
# Sponge CLI Install Script (macOS Apple Silicon)
#
# Downloads the latest stable Sponge CLI (`goose` binary) from GitHub Releases
# and installs it into $GOOSE_BIN_DIR (default: $HOME/.local/bin).
#
# Usage:
#   curl -fsSL https://github.com/floatfinancial/goose/releases/download/stable/download_cli.sh | bash
#
# Environment variables:
#   GOOSE_BIN_DIR  - Install directory (default: $HOME/.local/bin)
#   GOOSE_VERSION  - Specific version to install (e.g., "v1.43.0"). Overrides CANARY.
#   CANARY         - If "true", downloads the canary release instead of stable
#   CONFIGURE      - If "false", skips interactive `goose configure`
##############################################################################

for cmd in curl tar bzip2; do
  command -v "$cmd" >/dev/null 2>&1 || { echo "Error: '$cmd' is required."; exit 1; }
done

REPO="floatfinancial/goose"
OUT_FILE="goose"
GOOSE_BIN_DIR="${GOOSE_BIN_DIR:-$HOME/.local/bin}"
CONFIGURE="${CONFIGURE:-true}"

if [ -n "${GOOSE_VERSION:-}" ]; then
  if [[ ! "$GOOSE_VERSION" =~ ^v?[0-9]+\.[0-9]+\.[0-9]+(-.*)?$ ]]; then
    echo "[error]: invalid version '$GOOSE_VERSION' (expected: vX.Y.Z or X.Y.Z)"
    exit 1
  fi
  RELEASE_TAG=$(echo "$GOOSE_VERSION" | sed 's/^v\{0,1\}/v/')
else
  RELEASE_TAG="$([ "${CANARY:-false}" = "true" ] && echo "canary" || echo "stable")"
fi

# Sponge is Apple Silicon only.
if [ "$(uname -s)" != "Darwin" ] || [ "$(uname -m)" != "arm64" ]; then
  echo "Error: Sponge ships only macOS Apple Silicon builds. Detected: $(uname -s) $(uname -m)"
  exit 1
fi

FILE="goose-aarch64-apple-darwin.tar.bz2"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$RELEASE_TAG/$FILE"

echo "Downloading $RELEASE_TAG: $FILE..."
if ! curl -sLf "$DOWNLOAD_URL" --output "$FILE"; then
  # Fall back to the newest tag only if the user didn't pin a version or ask for canary.
  if [ -z "${GOOSE_VERSION:-}" ] && [ "${CANARY:-false}" != "true" ]; then
    LATEST_TAG=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    [ -n "$LATEST_TAG" ] || { echo "Error: failed to download and latest tag unavailable"; exit 1; }
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST_TAG/$FILE"
    curl -sLf "$DOWNLOAD_URL" --output "$FILE" || { echo "Error: fallback to $LATEST_TAG failed"; exit 1; }
  else
    echo "Error: failed to download $DOWNLOAD_URL"
    exit 1
  fi
fi

TMP_DIR="/tmp/sponge_install_$$"
mkdir -p "$TMP_DIR"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Extracting $FILE..."
tar -xjf "$FILE" -C "$TMP_DIR"
rm "$FILE"
chmod +x "$TMP_DIR/goose"

mkdir -p "$GOOSE_BIN_DIR"
echo "Installing to $GOOSE_BIN_DIR/$OUT_FILE"

# If the binary is currently running, in-place overwrite fails with ETXTBSY on Linux
# and gets stale-caches on macOS. Move-then-swap so a failed install leaves the old
# binary in place.
if [ -f "$GOOSE_BIN_DIR/$OUT_FILE" ]; then
  mv "$GOOSE_BIN_DIR/$OUT_FILE" "$GOOSE_BIN_DIR/$OUT_FILE.old"
  if ! mv "$TMP_DIR/goose" "$GOOSE_BIN_DIR/$OUT_FILE"; then
    echo "Error: install failed, restoring previous binary"
    mv "$GOOSE_BIN_DIR/$OUT_FILE.old" "$GOOSE_BIN_DIR/$OUT_FILE"
    exit 1
  fi
  rm -f "$GOOSE_BIN_DIR/$OUT_FILE.old"
else
  mv "$TMP_DIR/goose" "$GOOSE_BIN_DIR/$OUT_FILE"
fi

# Strip macOS quarantine so the freshly-downloaded binary runs without Gatekeeper prompts.
xattr -d com.apple.quarantine "$GOOSE_BIN_DIR/$OUT_FILE" 2>/dev/null || true

if [ "$CONFIGURE" = "true" ]; then
  echo
  echo "Signing you in to AWS SSO..."
  if [ -t 0 ]; then
    "$GOOSE_BIN_DIR/$OUT_FILE" auth aws-sso
  elif [ -r /dev/tty ]; then
    "$GOOSE_BIN_DIR/$OUT_FILE" auth aws-sso < /dev/tty
  else
    echo "Non-interactive shell — run '$GOOSE_BIN_DIR/$OUT_FILE auth aws-sso' manually."
  fi
fi

if [[ ":$PATH:" != *":$GOOSE_BIN_DIR:"* ]]; then
  echo
  echo "Warning: $GOOSE_BIN_DIR is not in your PATH."
  SHELL_NAME=$(basename "$SHELL")
  echo "Add this to ~/.${SHELL_NAME}rc:"
  echo "    export PATH=\"$GOOSE_BIN_DIR:\$PATH\""
fi
