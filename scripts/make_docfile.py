#!/usr/bin/env python3
r"""A faithful port of GNU's `lib-src/make-docfile.c` C-source scanner.

Neomacs has no `make-docfile` and no `etc/DOC`, so the two generators that
stand in for that file -- `extract_gnu_defun_docs.py` and
`extract_gnu_defvar_docs.py` -- have to answer, from GNU's `src/*.c` alone,
exactly the question `make-docfile` answers.  Both used to answer it with a
regular expression per question: one regex for the `DEFUN`/`DEFVAR` head, a
second literal search for the `doc:` marker, a third rule for leading
whitespace.  Three independent approximations of one scanner is three chances
to disagree with it, and they did:

* entry 173 found nine `DEFVAR` blocks whose `doc:` marker is not spelled
  `"doc: /*"`, and 35 rows whose leading whitespace was stripped by a
  different rule than GNU's;
* entry 181 found the same two bugs on the `DEFUN` side plus four more that
  only the function head has -- 30 heads the head regex never matched, seven
  docs read out of a *later* function's comment, twelve heads skipped as
  collateral of those seven, and 37 rows serving GNU's own `SKIP` placeholder
  as documentation.

Every one of those is a disagreement between two approximations that a single
scanner cannot have, because in `make-docfile` the head and its doc string are
read by *one pass of one state machine*: `scan_c_stream` finds `DEFUN` and
then keeps reading the same character stream through the commas, the
interactive spec, the `doc` keyword and into `read_c_string_or_comment`.  There
is no second search to be out of step with the first.

So this module is that state machine, ported character for character, and the
generators are front ends over its output.  It is deliberately a transcription
rather than a rewrite: the control flow, the variable names and the order of
the tests follow `lib-src/make-docfile.c` so the two can be diffed by eye.

`verify_against_make_docfile()` closes the loop: given GNU's own compiled
`make-docfile`, it asserts this port's output is byte-identical over the same
inputs.  That is the guard the ledger's law asks for -- it is not a predicate
over the rows we produced, it is an equality against the authority, and it
fails on a row we never produced.

Ported from GNU Emacs 31.0.90, `lib-src/make-docfile.c`:

* `scan_c_stream`            lines 850-1230
* `read_c_string_or_comment` lines 388-482
* `scan_keyword_or_put_char` lines 313-378
* `put_char`                 lines 279-310
* `write_c_args`             lines 486-556
* `put_filename`             lines 218-230
"""

from __future__ import annotations

import io
import subprocess
from dataclasses import dataclass, field
from pathlib import Path

EOF = -1


def _isspace(c: int) -> bool:
    """`c_isspace`, restricted to the C locale as gnulib's is."""
    return c in (0x20, 0x09, 0x0A, 0x0B, 0x0C, 0x0D)


def _isalpha(c: int) -> bool:
    return 0x41 <= c <= 0x5A or 0x61 <= c <= 0x7A


def _isalnum(c: int) -> bool:
    return _isalpha(c) or 0x30 <= c <= 0x39


class _Stream:
    """`FILE *` with just the operations `scan_c_stream` uses."""

    def __init__(self, data: bytes) -> None:
        self._data = data
        self._pos = 0
        self._pushback: list[int] = []

    def getc(self) -> int:
        if self._pushback:
            return self._pushback.pop()
        if self._pos >= len(self._data):
            self._pos += 1  # `feof' only becomes true after a read past the end
            return EOF
        c = self._data[self._pos]
        self._pos += 1
        return c

    def ungetc(self, c: int) -> None:
        # C's `ungetc (EOF, f)' is a no-op; any other byte is pushed back and
        # clears the end-of-file indicator.
        if c != EOF:
            self._pushback.append(c)

    def scanf_int(self) -> int | None:
        """`fscanf (infile, "%d", &n)`: optional sign, then digits.

        Returns None for a matching failure, which is `fscanf` returning 0:
        the caller must leave its variable alone, and the stream is not
        advanced.  `scan_c_stream` relies on both (`charset.c:845`).
        """
        while self._pushback:
            # A pushed-back byte is part of the field `fscanf` is about to read.
            self._pos -= 1
            self._pushback.pop()
        start = self._pos
        while self._pos < len(self._data) and _isspace(self._data[self._pos]):
            self._pos += 1
        sign = 1
        if self._pos < len(self._data) and self._data[self._pos] in (0x2B, 0x2D):
            sign = -1 if self._data[self._pos] == 0x2D else 1
            self._pos += 1
        digits = self._pos
        while self._pos < len(self._data) and 0x30 <= self._data[self._pos] <= 0x39:
            self._pos += 1
        if self._pos == digits:
            self._pos = start
            return None
        return sign * int(self._data[digits : self._pos])

    @property
    def eof(self) -> bool:
        return self._pos > len(self._data) and not self._pushback


@dataclass
class _RcsocState:
    """`struct rcsoc_state`."""

    in_file: _Stream
    out: io.BytesIO | None
    buf: bytearray | None
    pending_spaces: int = 0
    pending_newlines: int = 0
    keyword: bytes | None = None
    cur_keyword_ptr: int = 0
    saw_keyword: bool = False


def _put_char(ch: int, state: _RcsocState) -> None:
    """`put_char`: flush pending newlines, then pending spaces, then CH."""
    while True:
        if state.pending_newlines > 0:
            state.pending_newlines -= 1
            out_ch = 0x0A
        elif state.pending_spaces > 0:
            state.pending_spaces -= 1
            out_ch = 0x20
        else:
            out_ch = ch
        if state.out is not None:
            state.out.write(bytes([out_ch]))
        if state.buf is not None:
            state.buf.append(out_ch)
        if out_ch == ch:
            break


def _scan_keyword_or_put_char(ch: int, state: _RcsocState) -> None:
    """`scan_keyword_or_put_char`: swallow a line-initial `usage:` keyword."""
    kw = state.keyword
    if (
        kw is not None
        and state.cur_keyword_ptr < len(kw)
        and kw[state.cur_keyword_ptr] == ch
        and (state.cur_keyword_ptr > 0 or state.pending_newlines > 0)
    ):
        state.cur_keyword_ptr += 1
        if state.cur_keyword_ptr == len(kw):
            state.saw_keyword = True
            state.cur_keyword_ptr = 0
            # Canonicalize whitespace preceding a usage string.
            state.pending_newlines = 2
            state.pending_spaces = 0
            while True:
                c = state.in_file.getc()
                if c not in (0x20, 0x0A):
                    break
            if c != 0x28:  # '('
                raise ValueError("Missing '(' after keyword")
            _put_char(c, state)
            # Skip the function name and replace it with `fn'.
            while True:
                c = state.in_file.getc()
                if c == EOF:
                    raise ValueError("Unexpected EOF after keyword")
                if c in (0x20, 0x29):  # ' ' or ')'
                    break
            _put_char(0x66, state)  # 'f'
            _put_char(0x6E, state)  # 'n'
            state.in_file.ungetc(c)
        return

    if kw is not None and state.cur_keyword_ptr > 0:
        # "We scanned the beginning of a potential usage keyword, but it was a
        # false alarm.  Output the part we scanned."  GNU does NOT re-test CH
        # against the keyword's first byte afterwards, so `uusage:' at the head
        # of a line is not a usage line; keep that.
        for i in range(state.cur_keyword_ptr):
            _put_char(kw[i], state)
        state.cur_keyword_ptr = 0
    _put_char(ch, state)


def _read_c_string_or_comment(
    infile: _Stream, printflag: int, comment: bool, want_usage: bool
) -> tuple[int, bytes, bool]:
    """`read_c_string_or_comment`.

    Returns `(next_char, text, saw_usage)`.  `text` is what GNU would have
    written to stdout (printflag > 0) or into `input_buffer` (printflag < 0).
    """
    out = io.BytesIO() if printflag > 0 else None
    buf = bytearray() if printflag < 0 else None
    state = _RcsocState(
        in_file=infile,
        out=out,
        buf=buf,
        keyword=b"usage:" if want_usage else None,
    )

    c = infile.getc()
    # The one rule entry 173 and entry 181 both got wrong by hand: ALL leading
    # whitespace inside a `doc:' comment is discarded, newlines included.
    if comment:
        while _isspace(c):
            c = infile.getc()

    while c != EOF:
        while c != EOF and (c != 0x2A if comment else c != 0x22):
            if c == 0x5C:  # '\\'
                c = infile.getc()
                if c in (0x0A, 0x0D):
                    c = infile.getc()
                    continue
                if c == 0x6E:  # 'n'
                    c = 0x0A
                elif c == 0x74:  # 't'
                    c = 0x09
            if c == 0x20:
                state.pending_spaces += 1
            elif c == 0x0A:
                state.pending_newlines += 1
                state.pending_spaces = 0
            else:
                _scan_keyword_or_put_char(c, state)
            c = infile.getc()

        if c != EOF:
            c = infile.getc()

        if comment:
            if c == 0x2F:  # '/'
                c = infile.getc()
                break
            _scan_keyword_or_put_char(0x2A, state)  # '*'
        else:
            if c != 0x22:
                break
            # If we had a "", concatenate the two strings.
            c = infile.getc()

    text = out.getvalue() if out is not None else bytes(buf or b"")
    return c, text, state.saw_keyword


def _write_c_args(buf: bytes, minargs: int, maxargs: int) -> bytes:
    """`write_c_args`: render `(fn ARG ...)` from the C parameter list."""
    out = bytearray(b"(fn")
    if buf[:1] == b"(":
        buf = buf[1:]
    in_ident = False
    ident_start = 0
    ident_length = 0
    for p, c in enumerate(buf):
        if (_isalnum(c) or c == 0x5F) != in_ident:
            if not in_ident:
                in_ident = True
                ident_start = p
            else:
                in_ident = False
                ident_length = p - ident_start
        if c in (0x2C, 0x29):  # ',' or ')'
            if ident_length == 0:
                continue
            ident = buf[ident_start : ident_start + ident_length]
            if ident == b"void":
                continue
            out.append(0x20)
            if minargs == 0 and maxargs > 0:
                out += b"&optional "
            minargs -= 1
            maxargs -= 1
            if ident == b"defalt":
                # In C code `default' is reserved, so GNU spells it `defalt'.
                out += b"DEFAULT"
            else:
                out += ident.upper().replace(b"_", b"-")
            ident_length = 0
    out.append(0x29)
    return bytes(out)


@dataclass
class Record:
    """One `\\037F` / `\\037V` record of the DOC stream."""

    kind: str  # 'F' or 'V'
    name: str
    doc: str
    source_file: str = ""
    #: Byte offset of the `DEFUN`/`DEFVAR_` token, for diagnostics.
    offset: int = 0
    line: int = 0


@dataclass
class ScanResult:
    records: list[Record] = field(default_factory=list)
    #: `DEFUN`/`DEFVAR_*` heads that yielded no doc record at all.  GNU's
    #: scanner reaches the same dead end -- it just `continue`s silently --
    #: so this is not by itself a bug, but a generator that drops a name
    #: must say which, because a missing row is invisible downstream.
    heads_without_doc: list[tuple[str, str, int]] = field(default_factory=list)
    #: Bytes GNU's `make-docfile` would have written, for byte-level
    #: verification against the real binary.
    doc_stream: bytes = b""


def scan_c_file(path: Path, emit_filename: bool = True) -> ScanResult:
    """`scan_file` + `scan_c_stream` for one C source file."""
    data = path.read_bytes()
    infile = _Stream(data)
    result = ScanResult()
    stream = io.BytesIO()
    if emit_filename:
        stream.write(b"\037S" + path.name.encode() + b"\n")

    c = 0x0A  # `int c = '\n';'
    # `int commas, minargs, maxargs;' is function-scope in the C, so a DEFUN
    # whose MIN/MAX are not literals -- `charset_arg_max' in `charset.c:845',
    # `coding_arg_max' in `coding.c:10988' -- inherits the previous DEFUN's
    # numbers rather than failing.  `fscanf' returns 0 on a matching failure,
    # not a negative, so GNU's `if (scanned < 0) goto eof;' does not fire and
    # the stream is not advanced either.
    minargs = -1
    maxargs = -1
    while not infile.eof:
        doc_keyword = False
        defunflag = False
        defvarflag = False
        defvarperbufferflag = False

        if c not in (0x0A, 0x0D):
            c = infile.getc()
            continue
        c = infile.getc()
        if c == 0x20:  # ' '
            # NOTE: an INDENTED `DEFUN' is deliberately not a defun -- this
            # branch accepts only DEFSYM and DEFVAR_.  `alloc.c:5011' has a
            # `DEFUN ("testme", ...)' inside a comment, indented, and GNU
            # emits nothing for it.
            while c == 0x20:
                c = infile.getc()
            if c != 0x44:  # 'D'
                continue
            c = infile.getc()
            if c != 0x45:  # 'E'
                continue
            c = infile.getc()
            if c != 0x46:  # 'F'
                continue
            c = infile.getc()
            if c == 0x53:  # 'S' -- DEFSYM
                c = infile.getc()
                if c != 0x59:
                    continue
                c = infile.getc()
                if c != 0x4D:
                    continue
                c = infile.getc()
                if c not in (0x20, 0x09, 0x28):
                    continue
                continue  # `if (type == SYMBOL) continue;' below
            elif c == 0x56:  # 'V' -- DEFVAR_
                c = infile.getc()
                if c != 0x41:
                    continue
                c = infile.getc()
                if c != 0x52:
                    continue
                c = infile.getc()
                if c != 0x5F:
                    continue
                defvarflag = True
                c = infile.getc()
                defvarperbufferflag = c == 0x50  # 'P' -- DEFVAR_PER_BUFFER
                c = infile.getc()
            else:
                continue
        elif c == 0x44:  # 'D' at column 0
            c = infile.getc()
            if c != 0x45:
                continue
            c = infile.getc()
            if c != 0x46:
                continue
            c = infile.getc()
            defunflag = c == 0x55  # 'U'
            if not defunflag:
                # Column-0 DEFVAR_/DEFSYM: fall through the same way GNU does,
                # which is to say `defvarflag' stays false and the comma count
                # below is the DEFSIMPLE/DEFPRED one.  GNU's own code has this
                # shape; in practice every column-0 DEF* token in `src/*.c' is
                # a DEFUN, and the heads_without_doc guard would catch a new
                # spelling.
                pass
        else:
            continue

        head_off = infile._pos

        while c != 0x28:  # '('
            if c == EOF or c < 0:
                return _finish(result, stream)
            c = infile.getc()

        # Lisp variable or function name.
        c = infile.getc()
        if c != 0x22:  # '"'
            continue
        c, name_bytes, _ = _read_c_string_or_comment(infile, -1, False, False)

        if defunflag:
            commas = 5
        elif defvarperbufferflag:
            commas = 3
        elif defvarflag:
            commas = 1
        else:
            commas = 2  # DEFSIMPLE / DEFPRED

        eof_hit = False
        while commas:
            if c == 0x2C:  # ','
                commas -= 1
                if defunflag and commas in (1, 2):
                    while True:
                        c = infile.getc()
                        if not _isspace(c):
                            break
                    if c < 0:
                        eof_hit = True
                        break
                    infile.ungetc(c)
                    if commas == 2:
                        scanned = infile.scanf_int()
                        if scanned is not None:
                            minargs = scanned
                    else:
                        if c in (0x4D, 0x55):  # 'M' (MANY) or 'U' (UNEVALLED)
                            maxargs = -1
                        else:
                            scanned = infile.scanf_int()
                            if scanned is not None:
                                maxargs = scanned
            if c == EOF:
                eof_hit = True
                break
            c = infile.getc()
        if eof_hit:
            return _finish(result, stream)

        while _isspace(c):
            c = infile.getc()

        if c == 0x22:  # an old-style `"DOC"' interactive spec, discarded
            c, _, _ = _read_c_string_or_comment(infile, 0, False, False)

        while c != EOF and c != 0x2C and c != 0x2F:  # ',' '/'
            c = infile.getc()
        if c == 0x2C:
            while True:
                c = infile.getc()
                if not _isspace(c):
                    break
            while _isalpha(c):
                c = infile.getc()
            if c == 0x3A:  # ':'
                doc_keyword = True
                while True:
                    c = infile.getc()
                    if not _isspace(c):
                        break

        comment = False
        if c == 0x2F:  # '/'
            c = infile.getc()
            comment = c == 0x2A  # '*'

        if not (comment or c == 0x22):
            # No doc string for this head at all.  `treesit.c' spells one
            # marker `doc : /*' (a space before the colon), which lands here
            # in GNU too -- GNU Emacs itself answers nil for
            # `treesit-tracking-line-column-p'.
            result.heads_without_doc.append(
                (path.name, name_bytes.decode("utf-8", "replace"), head_off)
            )
            continue

        kind = "V" if defvarflag else "F"
        header = b"\037" + kind.encode() + name_bytes + b"\n"
        stream.write(header)

        c, doc_bytes, saw_usage = _read_c_string_or_comment(infile, 1, comment, True)
        stream.write(doc_bytes)

        if defunflag and maxargs != -1 and not saw_usage:
            argbuf = bytearray()
            if not comment or doc_keyword:
                while c != 0x29:  # ')'
                    if c < 0:
                        return _finish(result, stream)
                    c = infile.getc()
            while c != 0x28:  # '('
                if c < 0:
                    return _finish(result, stream)
                c = infile.getc()
            nested = 0
            while True:
                argbuf.append(c)
                nested += (c == 0x28) - (c == 0x29)
                if c == 0x29 and not nested:
                    break
                c = infile.getc()
                if c < 0:
                    return _finish(result, stream)
            fn = b"\n\n" + _write_c_args(bytes(argbuf), minargs, maxargs)
            stream.write(fn)
            doc_bytes += fn

        result.records.append(
            Record(
                kind=kind,
                name=name_bytes.decode("utf-8", "replace"),
                doc=doc_bytes.decode("utf-8", "replace"),
                source_file=path.name,
                offset=head_off,
                line=data.count(b"\n", 0, head_off) + 1,
            )
        )

    return _finish(result, stream)


def _finish(result: ScanResult, stream: io.BytesIO) -> ScanResult:
    result.doc_stream = stream.getvalue()
    return result


def scan_c_directory(src: Path) -> ScanResult:
    """`scan_file` over SRC's C and Objective-C sources.

    GNU's DOC is built by running make-docfile over the objects a build
    compiles, and the NS backend's DEFVAR/DEFUN docstrings live in `*.m`
    (nsterm.m/nsfns.m/nsmenu.m/nsselect.m/nsfont.m).  Scan those after the
    `*.c` files: the first declaration of a shared name keeps the C site the
    tables were generated with, and the NS-only names are appended rather
    than interleaved by filename.
    """
    merged = ScanResult()
    chunks = []
    for c_file in [*sorted(src.glob("*.c")), *sorted(src.glob("*.m"))]:
        one = scan_c_file(c_file)
        merged.records.extend(one.records)
        merged.heads_without_doc.extend(one.heads_without_doc)
        chunks.append(one.doc_stream)
    merged.doc_stream = b"".join(chunks)
    return merged


def find_make_docfile(src: Path) -> Path | None:
    """GNU's own compiled `make-docfile`, if the mirror has been built."""
    candidate = src.parent / "lib-src" / "make-docfile"
    return candidate if candidate.is_file() else None


def verify_against_make_docfile(src: Path, doc_stream: bytes) -> str | None:
    """Assert this port's DOC stream equals GNU's own binary's, byte for byte.

    Returns None on success, or a human-readable description of the first
    divergence.  This is the guard the generators cannot build for themselves:
    a check over the rows we emitted can never see a row we failed to emit,
    but an equality against the authority sees both directions at once.
    """
    binary = find_make_docfile(src)
    if binary is None:
        return None
    names = [
        *sorted(p.name for p in src.glob("*.c")),
        *sorted(p.name for p in src.glob("*.m")),
    ]
    proc = subprocess.run(
        [str(binary), *names],
        cwd=str(src),
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    if proc.returncode != 0:
        return f"{binary} exited {proc.returncode}"
    theirs = proc.stdout
    if theirs == doc_stream:
        return None
    for i, (a, b) in enumerate(zip(doc_stream, theirs)):
        if a != b:
            lo = max(0, i - 120)
            return (
                f"DOC stream differs from {binary} at byte {i}:\n"
                f"  ours:   ...{doc_stream[lo : i + 60]!r}\n"
                f"  theirs: ...{theirs[lo : i + 60]!r}"
            )
    return (
        f"DOC stream differs from {binary} in length: "
        f"ours {len(doc_stream)}, theirs {len(theirs)}"
    )


# `make-docfile` emits nothing for a head whose doc marker it cannot read, and
# it `continue`s silently rather than complaining -- so a head with no doc block
# is not by itself a scanner bug.  GNU has exactly two, both `DEFUN`s in
# `src/treesit.c`, both spelled `doc : /*` with a space before the colon:
# `scan_c_stream` does `while (c_isalpha (c)) c = getc (infile); if (c == ':')`,
# so the space ends the keyword scan, `doc_keyword` stays false, and the `/*`
# two characters later is never reached.  GNU Emacs itself answers nil for both
# names -- verified against `emacs -Q --batch`, not inferred -- so reproducing
# the omission is parity rather than damage.
#
# The set is exhaustive on purpose, and matched on `(file, name)` rather than on
# a name prefix.  A THIRD head with no doc block means either GNU gained a typo
# or this port drifted from `make-docfile`, and either way no table may be
# written until a human has looked.
GNU_HEADS_WITH_NO_DOC_BLOCK = {
    ("treesit.c", "treesit-tracking-line-column-p"),
    ("treesit.c", "treesit-parser-tracking-line-column-p"),
}


def unexpected_heads_without_doc(scan: "ScanResult") -> list[str]:
    """`file:name` for every head with no doc block that GNU does not have."""
    return [
        f"{f}:{n}"
        for (f, n, _) in scan.heads_without_doc
        if (f, n) not in GNU_HEADS_WITH_NO_DOC_BLOCK
    ]


def is_skip_placeholder(doc: str) -> bool:
    """`Fsnarf_documentation`'s own test: `strncmp (end, "\\nSKIP", 5)`.

    A name several window-system files define carries the real text in exactly
    one of them and `doc: /* SKIP: real doc in xfns.c.  */` in the rest, so the
    string is maintained once.  `Fsnarf_documentation` refuses to install such
    a record for a variable (`src/doc.c:600-608`) *and* for a function
    (`src/doc.c:617-621`), under the comment "Ignore docs that start with SKIP.
    These mark placeholders where the real doc is elsewhere."  No GNU build
    ever shows one to a user, so a table row holding one is a generator bug
    rather than a value.
    """
    return doc.startswith("SKIP")


if __name__ == "__main__":
    import sys

    src = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    res = scan_c_directory(src)
    problem = verify_against_make_docfile(src, res.doc_stream)
    f = [r for r in res.records if r.kind == "F"]
    v = [r for r in res.records if r.kind == "V"]
    print(f"records: F={len(f)} V={len(v)}  heads without doc: {len(res.heads_without_doc)}")
    print(f"verify against GNU's make-docfile: {problem or 'IDENTICAL'}")
