# Docker image

Release tags publish the same image to
[`evalexec/neomacs`](https://hub.docker.com/r/evalexec/neomacs) on Docker Hub and
[`ghcr.io/eval-exec/neomacs`](https://github.com/eval-exec/neomacs/pkgs/container/neomacs)
on GitHub Container Registry for `linux/amd64` and `linux/arm64`. The image
contains the complete Linux release tree: the Neomacs and neomacsclient
binaries, portable dump, Lisp, etc, leim, documentation, and Linux desktop
resources. It does not contain Rust, Cargo, a compiler, or the source checkout.

The portable default is the terminal frontend:

```sh
docker run --rm -it evalexec/neomacs:latest
```

Arguments after the image name are passed to Neomacs. For example, run a batch
expression with:

```sh
docker run --rm evalexec/neomacs:latest \
  --batch --eval '(princ (emacs-version))'
```

Mount a project and a persistent configuration volume with:

```sh
docker volume create neomacs-config
docker run --rm -it \
  --volume neomacs-config:/home/neomacs/.emacs.d \
  --volume "$PWD:/workspace" \
  --workdir /workspace \
  evalexec/neomacs:latest
```

The container runs as the unprivileged `neomacs` user (UID/GID 1000). Host
files mounted read-write therefore need to be writable by that identity.

## Tags and platform support

For a stable `v0.0.16` release, publication creates `0.0.16`, `0.0`, and
`latest`. Starting with the 1.x series it also creates the major tag, such as
`1`. A prerelease updates only its exact prerelease tag, not `latest` or a
stable minor tag. For reproducible deployments, prefer the exact version or
the digest shown by either registry instead of a moving tag. Both registries
receive the same verified OCI index, so either registry's exact version tag is
interchangeable.

Docker Desktop can run the Linux image on macOS and Windows, but it does not
turn the container into a native Metal or DX12 application. Terminal and batch
operation are the portable container use cases. Linux GUI use is possible but
host-specific: the container must be given a Wayland or X11 socket, audio
socket, and (for acceleration) the appropriate `/dev/dri` device. These grants
cross the host/container isolation boundary and should be made deliberately.

## Release provenance

The image is not a second build of Neomacs. Each architecture wraps the exact
canonical Linux tarball that already passed the release smoke test and GLIBC
audit. The release workflow then:

1. validates the archive root and target metadata;
2. builds on the matching native amd64 or arm64 runner;
3. pushes an untagged, content-addressed image with SBOM and provenance;
4. pulls and batch-starts that digest as the non-root runtime user; and
5. moves the public tags only after both digests pass.

The runtime base is Ubuntu 22.04, pinned by multi-architecture manifest digest,
which matches Neomacs's GLIBC 2.35 release baseline.

### Maintainer configuration

Docker Hub publication reads the repository variable `DOCKERHUB_USERNAME` and
repository secret `DOCKERHUB_TOKEN`. The token should be a Docker Hub personal
access token with read/write scope for the image repository, not an account
password. GHCR publication uses the workflow's short-lived `GITHUB_TOKEN` with
`packages: write`; it needs no long-lived registry secret. The final manifest
job records a `container-release` GitHub deployment linked to the package.
Registry pushes alone do not create GitHub Deployments; the workflow's explicit
environment binding is what creates and updates that deployment record.

Publishing with this public repository's `GITHUB_TOKEN` links the GHCR package
to the repository and gives it the repository's public visibility model. Every
release drops its GHCR credentials after publication and is verified with an
anonymous registry read, so an accidental private-package configuration fails
the deployment instead of producing an inaccessible advertised image.

A normal tag release calls the Docker workflow after the GitHub release is
published. To publish or repair an older GitHub release without moving its Git
tag, run the same workflow manually:

```sh
gh workflow run docker-release.yml -f release_tag=v0.0.15
```

The workflow downloads that release's canonical tarballs and verifies them
against its published `SHA256SUMS` before using the credentials. The v0.0.15
arm64 tarball's older flat filesystem shape is normalized into the current
runtime layout only after checksum and release-tag verification.

## Build locally from a release tarball

First build or download a canonical Linux release archive. Then prepare the
same validated context used by CI:

```sh
./scripts/prepare-docker-runtime-context.sh \
  --archive dist/neomacs-VERSION-x86_64-unknown-linux-gnu.tar.gz \
  --target x86_64-unknown-linux-gnu \
  --release-git FULL_RELEASE_COMMIT_SHA \
  --output ./tmp/docker-context

docker build \
  --file docker/Dockerfile.runtime \
  --tag neomacs:local \
  ./tmp/docker-context
```

`docker/Dockerfile.ubuntu-22.04` remains the separate source-build environment;
it is not the image distributed to users.
