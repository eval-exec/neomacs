"""Shared prelaunch safety boundary for disposable native GUI runners."""
import os
from pathlib import Path
import re
import stat


def validate_render_node(render_node: str) -> None:
    """Inspect metadata only; never open a card, input device, or seat."""
    match = re.fullmatch(r'/dev/dri/renderD([0-9]+)', render_node)
    if match is None:
        raise RuntimeError('select an existing DRM render node, never a card/seat')
    try:
        info = Path(render_node).lstat()
    except OSError as error:
        raise RuntimeError('selected DRM render node is missing or inaccessible') from error
    # Reject symlinks and regular files even when their names resemble nodes.
    # Linux DRM uses major 226, with render-node minors starting at 128.
    if (not stat.S_ISCHR(info.st_mode) or os.major(info.st_rdev) != 226
            or os.minor(info.st_rdev) < 128
            or os.minor(info.st_rdev) != int(match[1])):
        raise RuntimeError('select a native DRM render character device, never a card/seat')
