# Releasing Neomacs for Linux

## Where each Linux artifact is built

| Artifact | Built on | Why there |
|---|---|---|
| tarball, `.deb`, AppImage | the `ubuntu-22.04` runner | Their linkage is Debian-family by design, and 22.04 is the oldest baseline the release still carries (glibc 2.35). |
| `.rpm` | an el9 container (`build-linux-rpm`) | Because the build host is part of an RPM's metadata — see below. |

## Why the RPM is built inside el9

`rpmbuild` derives each package's library `Requires` from the payload's ELF *and*
from the build host's rpm, so an RPM built on the Ubuntu runner records Ubuntu's
libraries as Fedora's requirements:

```
Requires: libtinfo.so.6(NCURSES6_TINFO_5.0.19991023)(64bit)
Requires: libm.so.6(GLIBC_2.35)(64bit)
```

The first line is Ubuntu's ncurses, which is built with versioned symbols.
Fedora's ncurses publishes no such version and no Fedora package provides it, so
`dnf` refused the package outright — "nothing provides
libtinfo.so.6(NCURSES6_TINFO_5.0.19991023)(64bit)" (issue #388). The second line
is Ubuntu 22.04's glibc, which rules out RHEL 9 and its rebuilds: their glibc is
2.34.

Building the binary **and** the package inside el9 fixes both by construction.
The requirements become el9's, and a binary linked against glibc 2.34 and an
unversioned ncurses runs on everything newer — RHEL 9 / Rocky / Alma and Fedora
43+. The GLIBC 2.35 ceiling that `scripts/test-linux-release-artifacts.sh`
enforces still passes, because 2.34 sorts below it.

`%{?dist}` is defined on el9, so the file is
`dist/neomacs-<version>-1.el9.<arch>.rpm`. Scripts that name or find that file
derive it from the directory rather than hardcoding the tag — the tag belongs to
the build host.

## Building the RPM by hand

The CI job is `build-linux-rpm` in `.github/workflows/release.yml`. The same
thing locally, from the repository root — build in a scratch clone rather than
the working tree, because the container runs as root and would otherwise leave
root-owned files (and a root-owned `target/`) behind:

```bash
git clone --local . tmp/el9build

docker run --rm --network host -v "$PWD/tmp/el9build:/build/src" \
  quay.io/almalinuxorg/almalinux:9 bash -c '
    set -e
    dnf install -y git tar gzip which make binutils cpio file gcc gcc-c++ \
      pkgconf-pkg-config gawk ncurses-devel fontconfig-devel freetype-devel \
      glib2-devel gstreamer1-devel gstreamer1-plugins-base-devel libdrm-devel \
      zlib-devel lcms2-devel rpm-build
    dnf install -y gstreamer1 gstreamer1-plugins-base glib2 fontconfig freetype \
      ncurses-libs libstdc++ zlib libdrm
    curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain 1.96.1
    . "$HOME/.cargo/env"
    cd /build/src
    # The clone is owned by your uid while the container runs as root, so git
    # refuses it as dubiously owned -- silently, because package-rpm.sh discards
    # stderr.  Without this line `git describe` and `git rev-parse` both fail,
    # the version falls back to 0.0.0-dev, and rpmbuild rejects it at the very
    # end with "Illegal char '-' (0x2d) in: Version".
    git config --global --add safe.directory /build/src
    export NEOMACS_BUILD_PROFILE=release
    features=video,neomacs-layout-engine/freetype-bundled
    cargo build -p neomacs --features "$features" --profile release
    cargo xtask fresh-build --release --features "$features" --skip-build
    ./scripts/package-rpm.sh --target x86_64-unknown-linux-gnu --skip-build
  '
```

Details that cost time to discover:

- The el9 image already ships `curl-minimal`, which provides the `curl` binary.
  Asking for the full `curl` package is a hard conflict on el9 ("curl-minimal
  conflicts with curl provided by curl").
- Once the image is running, `git` must be installed before anything that needs
  it (`actions/checkout` in CI, `git clone` locally). The image has no git.
- `libdrm-devel` and `gawk` are mandatory and invisible to the payload's
  `DT_NEEDED`: `gstreamer-allocators-sys` links `drm`, and `neovm-core/build.rs`
  generates Lisp with GNU awk and panics without it.
- `gcc-c++` is required even though Neomacs is Rust: `simdutf` (via `rio-vt`)
  compiles C++ and links `libstdc++`.
- `gstreamer1-plugins-base-devel` must accompany `gstreamer1-devel`; the former
  supplies `gstapp`, `gstvideo`, `gstpbutils` and `gstallocators`.
- `make` is needed by `tikv-jemalloc-sys`, which runs `configure` and `make`.
- The second `dnf install` is not optional: the packaging scripts execute the
  binary they stage, so the runtime closure has to be present.

## Why there is no source RPM

`rpmbuild -ba` on this spec produces a hollow `.src.rpm`, and RPM is behaving
correctly: an SRPM carries the spec plus the files named in `SourceN:`, and this
spec names none -- it has no `Source:`, no `%prep` and no `%build`, because it
re-packages an already-built payload rather than compiling one.  Measured:

```
$ rpm -qpl neomacs-0.0.18-1.el9.src.rpm
neomacs.spec                       # 6.9 KB, nothing else
```

Shipping that would be worse than shipping nothing, since it looks like source.
Making it real means either embedding the 149 MB payload tarball as `Source0:`
(an SRPM that contains prebuilt binaries, which Fedora and COPR reject as a
source package), or writing a genuine source spec -- `Source0:` pointing at the
upstream tree, `%build` running cargo, `BuildRequires`, and vendored crates
because build hosts have no network.  The latter is the COPR/Packit path, and it
is where an SRPM belongs anyway: COPR builds and serves it from the repo, and
you stop hand-carrying the RPM at all.

The packager identity is passed to the build as a macro rather than written into
the spec, because Fedora's guidelines prohibit `Vendor:` and `Packager:` tags
there -- the build system is meant to own them.

## What CI verifies

1. **`build-linux-rpm`** installs its own output on el9
   (`dnf install -y ./dist/*.rpm`, no `--nodeps`) and runs
   `neomacs --batch --eval "(kill-emacs 0)"` — the package's declared
   dependencies resolved by a real transaction on the distro it was built for.
   It also fails outright if a versioned ncurses/tinfo requirement reappears.
2. **`verify-rpm-on-fedora`** downloads the x86_64 RPM and installs it on a real
   Fedora container, then runs the binary. This is the assertion issue #388
   needed and nothing performed before it shipped. `create-release` lists it in
   `needs:`, so a failing Fedora install blocks the release rather than
   annotating it.
3. **`scripts/test-linux-release-artifacts.sh --formats tar,rpm`** extracts the
   package, runs the binary from the extracted tree, and audits the GLIBC
   ceiling, the archlib layout and the desktop entry (and, for the tarball,
   that every member is owned by root and none is writable by group or others;
   see [File ownership](#file-ownership)). `build-linux` runs the same script
   with `--formats tar,appimage,deb`, which audits the `.deb` the same way and
   checks its maintainer scripts.

When changing `scripts/package-rpm.sh`, run the command above and check
`rpm -qp --requires dist/*.rpm`. What an el9 build records, and what to insist
on:

- **No ncurses/tinfo symbol version at all** — the requirement must read
  `libtinfo.so.6()(64bit)`, never
  `libtinfo.so.6(NCURSES6_TINFO_5.0.19991023)(64bit)`. That version stamp *is*
  issue #388. `build-linux-rpm` fails the job if it reappears.
- **No glibc version el9 does not itself provide.** The 2.35 ceiling in the
  artifact audit is the enforced bound; a build here lands at `GLIBC_2.34` for
  `libc.so.6` and `GLIBC_2.29` for `libm.so.6`, which is what the el9 install
  transaction proves.

## File ownership

The release runner stages every artifact as an unprivileged account (v0.0.20's
packages carry uid/gid 1001), and archive formats record the staged owner of
each member unless told otherwise. Root restores that owner when it unpacks
one: `dpkg -i` does, and so does GNU tar run as root, which is what
`install.sh --prefix` and a manual extraction into `/usr/local` or `/opt` do.
The installed binary, dump and Lisp tree would then belong to whichever local
account holds that number, and it could rewrite them to run code as every user
who starts `neomacs`, root included (`sudo neomacs`, or `neomacs` as root's
`EDITOR`).

So the root-installed artifacts record root, and nothing in them is writable
by group or others:

- `scripts/package-deb.sh` builds with `dpkg-deb --build --root-owner-group`,
  after `chmod -R go-w` on the staged tree, and gives the package a `preinst`
  that refuses to unpack over an installation another account can modify (see
  below).
- `scripts/package-release.sh` writes the tarball with
  `tar --owner=0 --group=0 --numeric-owner`, after `chmod -R go-w`, as GNU's
  `make-dist` does with `--mode=go-w`; `scripts/package-macos-app.sh` writes the
  macOS one with `tar --uid 0 --gid 0 --numeric-owner`.
- `install.sh` extracts with `tar --no-same-owner`, because `--tag` still
  reaches the older tarballs that record uid 1001.
- The RPM already installs as root:root (checked on v0.0.20). The AppImage is
  outside this: it runs in place rather than being installed by root.

`scripts/test-linux-release-artifacts.sh` fails if any member of the tarball or
the `.deb` is not owned by `0/0` or, symlinks aside, is writable by group or
others; it also extracts the `.deb`'s maintainer scripts, checks that they
parse, and runs the `preinst` over fixture trees, not over the runner's own
`/usr` (see below). It must refuse a world-writable tree whoever owns it. A
copy in which the runner's uid stands for root, so that ownership cannot be
the reason, and whose mount table is a file the test writes, must accept a
healthy tree with an unrelated symlink, for both `install` and `upgrade`. It
must refuse each of these on its own, listing that one path: a world-writable
tree, a group-writable file, a group- or world-writable shared directory, a
file system mounted in the tree, and a symlink where the package has a
directory. A copy in which root is another uid must refuse the tree for its
owner. `scripts/package-macos-app.sh`
checks its tarball in the release job: `0/0`, and no `o+w` member. The
`install.sh` step of `.github/workflows/test-suite.yml` installs an archive
whose members belong to uid 1001 as root, then fails if anything it installed
is not root's.

### Repairing an installation from v0.0.20 or earlier

Every `.deb` and Linux tarball up to v0.0.20 is affected; v0.0.20's carry
uid/gid 1001. Installing a fixed `.deb` does not repair an affected
installation: dpkg leaves a directory that already exists as it is, owner and
mode included, so `/usr/share/neomacs`, `/usr/libexec/neomacs` and
`/usr/share/doc/neomacs` would still belong to that uid, which could then
rename, replace or add to what root unpacks into them. The fixed package's
`preinst` therefore stops before unpacking anything, and lists every path in
those trees that root does not own or that is writable by group or others
(a named ACL entry with write shows in the group bits), every symlink where
the package has a directory, every directory it writes through that root does
not own or that is writable by group or others, any path whose existence it
cannot establish, and any file system mounted in the trees: repair first,
then install it again.

The `preinst` judges the system as well as an old installation. On a system
whose shared directories are writable by group or others, the `.deb` is
refused until `chmod go-w` has been run on the directories it lists, as
below. GitHub-hosted Ubuntu runners are such a system: their image makes
`/usr/share` world-writable.

First list what the package did not install, and every symlink in its trees.
That account created those entries -- one `site-start.el` in
`/usr/share/neomacs/lisp` is enough to run code -- so examine each one before
anything else; the `chown` below would make them look like the package's. The
package ships no symlinks in these trees, so each one listed was made by
someone else (`install.sh --prefix /usr` makes `share/neomacs/current`). A
symlink where the package has a directory makes dpkg unpack through it,
wherever it points (Debian Policy 6.6), and the `preinst` refuses it: see what
it points to, then remove it.

```bash
comm -23 \
  <(sudo find /usr/share/neomacs /usr/libexec/neomacs /usr/share/doc/neomacs | sort) \
  <(dpkg -L neomacs | sort)
sudo find /usr/share/neomacs /usr/libexec/neomacs /usr/share/doc/neomacs -type l -ls
```

Then list what root does not own or others can write in the package's trees
-- no output means there is nothing to repair in them -- and the directories
under `/usr` that root does not own or that everyone can write, among which
any the package created outside its trees (such as `/usr/libexec` on a system
that had none):

```bash
sudo find /usr/share/neomacs /usr/libexec/neomacs /usr/share/doc/neomacs \
  /usr/bin/neomacs /usr/bin/neomacsclient \
  \( ! -uid 0 -o ! -gid 0 -o \( ! -type l \( -perm -020 -o -perm -002 \) \) \) -ls
sudo find /usr -xdev -type d \( ! -uid 0 -o -perm -002 \) -ls
```

The `preinst` also refuses a group-writable directory it writes through. The
second `find` leaves group write out because some systems have group-writable
directories under `/usr` by design (Debian's `/usr/local` is `root:staff`,
mode 2775); take those from the `preinst`'s list.

After `install.sh --prefix DIR` run as root, check the whole tree, not the
binary: `install.sh` installs the binaries with `install -m`, so they are
root's even from v0.0.20, while `share/neomacs` and `libexec` are not:

```bash
sudo find DIR/share/neomacs \
  \( ! -uid 0 -o ! -gid 0 -o \( ! -type l \( -perm -020 -o -perm -002 \) \) \) -ls
```

and the same over the extracted tree for a tarball extracted by hand.

If the `preinst` listed a mount point, see what is mounted there and unmount it
(`sudo umount PATH`) before going on: `chown -R` and `chmod -R` descend into
mounted file systems.

```bash
findmnt -l | grep -e /usr/share/neomacs -e /usr/libexec/neomacs -e /usr/share/doc/neomacs
```

Then give the files back to root and take write access away from everyone
else -- the `chmod` also clears the write right of any named ACL entry,
through the ACL mask. Work from the top down, so that nothing is handled
while the other account can still replace what holds it, and never let
`chmod` follow a symlink. For the `.deb`, first each directory outside the
package's trees that the `preinst` listed, or that the second `find` lists and
the package created, parents first (`-type d`: a symlink is neither followed
nor changed):

```bash
sudo find DIR -maxdepth 0 -type d -exec chown -h root:root {} + -exec chmod go-w {} +
```

If a listed DIR is itself a symlink, this changes nothing and the `preinst`
keeps refusing: examine where the link points, and fix or remove it.

Then check that none of the package's trees is itself a symlink: `chmod -R`
follows a symlink named on its command line. This must print nothing; if it
prints a path, examine that link and remove it before going on:

```bash
sudo find /usr/share/neomacs /usr/libexec/neomacs /usr/share/doc/neomacs -maxdepth 0 -type l
```

Only then the trees, and last the package's files outside them:

```bash
sudo chown -hR root:root /usr/share/neomacs /usr/libexec/neomacs \
  /usr/share/doc/neomacs
sudo chmod -R go-w /usr/share/neomacs /usr/libexec/neomacs /usr/share/doc/neomacs
sudo chown -h root:root /usr/bin/neomacs /usr/bin/neomacsclient \
  /usr/share/applications/neomacs.desktop \
  /usr/share/icons/hicolor/scalable/apps/neomacs.svg
sudo find /usr/bin/neomacs /usr/bin/neomacsclient \
  /usr/share/applications/neomacs.desktop \
  /usr/share/icons/hicolor/scalable/apps/neomacs.svg \
  -maxdepth 0 -type f -exec chmod go-w {} +
```

(`-type f` because `chmod` follows a symlink; `chown -h` does not.) For
`install.sh --prefix DIR`, the same `chown -hR` and `chmod -R go-w` on
`DIR/share/neomacs`, once `find DIR/share/neomacs -maxdepth 0 -type l` prints
nothing; for a tarball extracted by hand, on the extracted tree, checked the
same way. Run the `find` again afterwards: anything that
account created while you worked is still its own. If such an account has
existed on the machine, reinstall from a fixed release so every file the
package ships comes from it again.
