#!/usr/bin/env bash
# Re-makes vendor/sonora-aec3: the sonora-aec3 crate exactly as published on crates.io at
# <version>, plus vendor/sonora-aec3.patch, which makes the echo canceller wait for 1.5 s of
# far-end audio (not 0.4 s) before it trusts its linear filter's echo estimate. With sparse
# far-end audio at the start of a call (short bursts, or a few words), 0.4 s arrives before the
# filter has converged, and the next bursts' onsets leak as audible echo until it does. Run it when
# sonora is bumped, then build: a patch that no longer applies fails here, and the change goes back
# in by hand. If upstream sonora makes the threshold configurable, delete vendor/sonora-aec3 and
# its `[patch.crates-io]` entry and set it through the config instead.
# Usage: scripts/vendor-sonora-aec3.sh <version>     e.g. scripts/vendor-sonora-aec3.sh 0.3.0
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
version="${1:?usage: scripts/vendor-sonora-aec3.sh <version>}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fsSL "https://static.crates.io/crates/sonora-aec3/sonora-aec3-$version.crate" -o "$tmp/crate.tar.gz"
tar -xzf "$tmp/crate.tar.gz" -C "$tmp"
rm -rf vendor/sonora-aec3
mv "$tmp/sonora-aec3-$version" vendor/sonora-aec3
tr -d '\r' < vendor/sonora-aec3.patch | git apply --directory=vendor/sonora-aec3
echo "vendor/sonora-aec3: sonora-aec3 $version + vendor/sonora-aec3.patch"
