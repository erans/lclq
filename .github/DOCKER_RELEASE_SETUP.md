# Docker Release Setup

This document explains how to configure GitHub secrets for automated Docker image publishing.

## Required GitHub Secrets

To publish Docker images to Docker Hub when creating releases, add these secrets to your GitHub repository:

### 1. Docker Hub Credentials

Navigate to: `Settings > Secrets and variables > Actions > New repository secret`

**DOCKERHUB_USERNAME**
- Your Docker Hub username
- Example: `yourusername`

**DOCKERHUB_TOKEN**
- A Docker Hub access token (NOT your password)
- How to create:
  1. Log in to [Docker Hub](https://hub.docker.com/)
  2. Go to Account Settings > Security > New Access Token
  3. Give it a name like "GitHub Actions - lclq"
  4. Copy the token (you won't see it again!)
  5. Add it as `DOCKERHUB_TOKEN` secret

## How It Works

When you create a new release by pushing a git tag:

```bash
git tag -a v1.0.0 -m "Release v1.0.0"
git push origin v1.0.0
```

The workflow will automatically:

1. ✅ Extract version from tag (e.g., `v1.0.0` → `1.0.0`)
2. ✅ Build Docker image with Alpine Linux
3. ✅ Tag image with multiple versions:
   - `lclq/lclq:1.0.0` (full version)
   - `lclq/lclq:1.0` (major.minor)
   - `lclq/lclq:1` (major)
   - `lclq/lclq:latest` (latest)
4. ✅ Push to Docker Hub (`lclq/lclq:*`)
5. ✅ Push to GitHub Container Registry (`ghcr.io/erans/lclq:*`)
6. ✅ Add OCI labels with metadata (version, git commit, build date)
7. ✅ Build for multiple platforms: `linux/amd64`, `linux/arm64`

## Image Metadata

Each image includes OCI labels:

```yaml
org.opencontainers.image.title: lclq
org.opencontainers.image.version: 1.0.0
org.opencontainers.image.revision: abc123 (git commit)
org.opencontainers.image.created: 2025-01-15T10:30:00Z
org.opencontainers.image.licenses: MIT OR Apache-2.0
```

View metadata:
```bash
docker inspect lclq/lclq:latest | jq '.[0].Config.Labels'
```

## Testing Locally

Build the image locally to test before releasing:

```bash
docker build -t lclq:test \
  --build-arg VERSION=0.1.0 \
  --build-arg BUILD_DATE=$(date -u +'%Y-%m-%dT%H:%M:%SZ') \
  --build-arg VCS_REF=$(git rev-parse --short HEAD) \
  .
```

## Release Checklist

Before creating a release:

- [ ] Update version in `Cargo.toml`
- [ ] Update `CHANGELOG.md` with changes
- [ ] Run tests: `cargo test`
- [ ] Build locally: `cargo build --release`
- [ ] Test Docker build locally
- [ ] Commit changes
- [ ] Create and push git tag: `git tag -a v1.0.0 -m "Release v1.0.0" && git push origin v1.0.0`
- [ ] Verify GitHub Actions workflow succeeds
- [ ] Verify images are published to Docker Hub and GHCR
- [ ] Test pulling and running the published image

## Updating Docker Hub Repository

After first release, update your Docker Hub repository:

1. Go to https://hub.docker.com/repository/docker/yourusername/lclq
2. Update description with README content
3. Add relevant tags (queue, sqs, pubsub, emulator, testing)
4. Link to GitHub repository
