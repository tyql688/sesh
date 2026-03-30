#!/bin/bash
# Usage: ./scripts/release.sh 0.2.0
#
# Bumps version in package.json, Cargo.toml, tauri.conf.json,
# commits, tags, and pushes. GitHub Actions builds the release.

set -euo pipefail

VERSION="${1:?Usage: $0 <version>  (e.g. 0.2.0)}"

if [[ "$VERSION" == v* ]]; then
  VERSION="${VERSION#v}"
fi

TAG="v${VERSION}"

# Check clean working tree
if ! git diff --quiet HEAD; then
  echo "Error: working tree is dirty. Commit or stash changes first."
  exit 1
fi

# Check tag doesn't exist
if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "Error: tag $TAG already exists."
  exit 1
fi

echo "Running tests before release..."
(cd src-tauri && cargo test) || { echo "Error: Rust tests failed. Aborting release."; exit 1; }
npm test || { echo "Error: Frontend tests failed. Aborting release."; exit 1; }

echo "Bumping version to $VERSION..."

# Cross-platform sed in-place (BSD vs GNU)
sedi() {
  if [[ "$OSTYPE" == darwin* ]]; then
    sed -i '' "$@"
  else
    sed -i "$@"
  fi
}

# Update package.json
sedi "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/" package.json

# Update Cargo.toml
sedi "s/^version = \"[^\"]*\"/version = \"$VERSION\"/" src-tauri/Cargo.toml

# Update tauri.conf.json
sedi "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/" src-tauri/tauri.conf.json

# Update Cargo.lock
(cd src-tauri && cargo check --quiet) || { echo "ERROR: cargo check failed"; exit 1; }

# Update package-lock.json
npm install --package-lock-only --ignore-scripts --silent || { echo "ERROR: npm install failed"; exit 1; }

echo "Committing..."
git add package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
git commit -m "chore: release $TAG"

echo "Tagging $TAG..."
git tag -a "$TAG" -m "Release $TAG"

echo "Pushing..."
git push && git push origin "$TAG"

echo ""
echo "Done! GitHub Actions will build the release."
echo "Check: https://github.com/tyql688/cc-session/actions"
echo "Release draft: https://github.com/tyql688/cc-session/releases"
