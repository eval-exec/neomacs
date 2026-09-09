"""Diagnose the system ncurses padding ABI without the Rust wrapper (macOS)."""
import ctypes as c
import os
from pathlib import Path
import subprocess
import tempfile
import termios

with tempfile.TemporaryDirectory() as directory:
    subprocess.run(["/usr/bin/tic", "-x", "-o", directory,
                    str(Path(__file__).with_name("fixtures") / "padding.src")], check=True)
    os.environ["TERMINFO"] = directory
    lib = c.CDLL("/usr/lib/libncurses.dylib")
    lib.setupterm.argtypes = [c.c_char_p, c.c_int, c.POINTER(c.c_int)]
    lib.tgetnum.argtypes = [c.c_char_p]
    lib.tgetflag.argtypes = [c.c_char_p]
    lib.curses_version.restype = c.c_char_p
    emit_type = c.CFUNCTYPE(c.c_int, c.c_int)
    lib.tputs.argtypes = [c.c_char_p, c.c_int, emit_type]
    master, slave = os.openpty()
    attributes = termios.tcgetattr(slave)
    attributes[4] = attributes[5] = termios.B9600
    termios.tcsetattr(slave, termios.TCSANOW, attributes)
    error = c.c_int()
    print("version", lib.curses_version(), "termios speeds", termios.tcgetattr(slave)[4:6])
    print("setupterm", lib.setupterm(b"neo-padding", slave, c.byref(error)), "error", error.value)
    print("baudrate", lib.baudrate(), "ospeed", c.c_short.in_dll(lib, "ospeed").value,
          "pb", lib.tgetnum(b"pb"), "xo", lib.tgetflag(b"xo"), "PC", c.c_char.in_dll(lib, "PC").value)
    for sequence in [b"A$<10>B", b"A$<10/>B"]:
        data = bytearray()
        def emit(byte):
            data.append(byte & 255)
            return byte
        print("tputs", sequence, lib.tputs(sequence, 1, emit_type(emit)), bytes(data))
    c.c_short.in_dll(lib, "ospeed").value = 13  # Apple's B9600 legacy code.
    data = bytearray()
    print("tputs after public ospeed correction", lib.tputs(b"A$<10/>B", 1, emit_type(emit)), bytes(data))
    os.close(slave)
    os.close(master)
