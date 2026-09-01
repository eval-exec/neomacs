# Ubuntu 22.04 build environment

This directory contains two deliberately separate images:

- `Dockerfile.ubuntu-22.04` is a compiler-equipped source-build environment.
- `Dockerfile.runtime` is the non-root application image published from an
  already-built, verified release tarball.

See [the Docker user guide](../docs/docker.md) for the published image, tags,
runtime examples, and the local runtime-image build procedure.

## Source-build environment

`Dockerfile.ubuntu-22.04` reproduces the oldest supported Linux build baseline:
Ubuntu 22.04 with glibc 2.35 and the Rust toolchain pinned by the release workflow.

Build the image:

```sh
docker build -f docker/Dockerfile.ubuntu-22.04 -t neomacs-build:ubuntu-22.04 .
```

Run it from the repository root. Cargo caches, build output, and temporary files
stay under `./tmp/`:

```sh
mkdir -p ./tmp/cargo-home ./tmp/target ./tmp/work
docker run --rm -it \
  --user "$(id -u):$(id -g)" \
  --env CARGO_HOME=/workspace/tmp/cargo-home \
  --env CARGO_TARGET_DIR=/workspace/tmp/target \
  --env TMPDIR=/workspace/tmp/work \
  --volume "$PWD:/workspace" \
  --workdir /workspace \
  neomacs-build:ubuntu-22.04
```

Inside the container, build with:

```sh
cargo xtask fresh-build --release
```

The image intentionally uses Ubuntu 22.04's system Cairo. It does not work
around dependency requirements that exceed the supported distribution baseline.
