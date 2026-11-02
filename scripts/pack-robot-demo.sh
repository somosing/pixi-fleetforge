#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if ! command -v pixi-pack >/dev/null 2>&1; then
    echo "Install pixi-pack: pixi global install pixi-pack pixi-unpack" >&2
    exit 1
fi

pixi-pack --platform linux-64 robot-demo/pixi.toml

archive_path="$repo_root/environment.tar"
if [ ! -f "$archive_path" ]; then
    echo "Expected pixi-pack to create $archive_path" >&2
    exit 1
fi

archive_sha256="$(sha256sum "$archive_path" | cut -d ' ' -f 1)"
cat > "$repo_root/robot-demo/pixi-release.json" <<JSON
{
  "version": "pixi-python-0.1.0",
  "artifact_path": "$archive_path",
  "sha256": "$archive_sha256",
  "artifact_kind": "pixi_archive"
}
JSON

printf 'Created robot-demo/pixi-release.json\nArchive SHA-256: %s\n' "$archive_sha256"
