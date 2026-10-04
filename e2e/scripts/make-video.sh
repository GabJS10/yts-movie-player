#!/usr/bin/env bash
# Generates the short H.264/AAC MP4 the local seeder shares in the real-app E2E.
# Output: e2e/.cache/sample.mp4 (git-ignored). Needs ffmpeg.
set -euo pipefail
out="$(dirname "$0")/../.cache/sample.mp4"
mkdir -p "$(dirname "$out")"
[ -s "$out" ] && { echo "$out"; exit 0; }
ffmpeg -loglevel error -y \
  -f lavfi -i testsrc2=size=640x360:rate=24 \
  -f lavfi -i sine=frequency=440:sample_rate=44100 \
  -t 30 -c:v libx264 -pix_fmt yuv420p -profile:v main -preset veryfast \
  -c:a aac -b:a 96k -movflags +faststart "$out"
echo "$out"
