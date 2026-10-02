#!/usr/bin/env python3
"""Explicit gui-test-hooks regressions on real daemon/native paths.

Registry fault peers are not compositors. Successful native controls use the
same pinned in-process EWM HeadlessBackend fixture as public acceptance.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import shutil
import socket
import array
import select
import threading
import subprocess
import tempfile
import time
from daemon_gui_safety import validate_render_node

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / 'test/daemon-gui'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def wait_for(predicate, owner, seconds=30):
    deadline = time.monotonic() + seconds
    while not predicate():
        if owner.poll() is not None or time.monotonic() >= deadline:
            raise AssertionError(f'gate not reached; process status {owner.poll()}')
        time.sleep(.01)


class Daemon:
    def __init__(self, binaries, module, render_node, directory):
        self.root = Path(directory)
        self.bin = binaries
        for name in ['home', 'config', 'data', 'cache', 'tmp', 'gates']:
            (self.root / name).mkdir(mode=0o700)
        shutil.copyfile(module, self.root / "module.so")
        self.gates = self.root / 'gates'
        # Keep descriptive persistent evidence separate from short Unix addresses.
        self.runtime = Path(tempfile.mkdtemp(prefix='gf-', dir=os.environ['TMPDIR']))
        self.endpoint = self.runtime / 'emacs/gui-faults'
        assert len(os.fsencode(self.endpoint)) < 108, self.endpoint
        assert len(os.fsencode(self.runtime / 'wayland-ewm-proof')) < 108, self.runtime
        self.records = []
        self.children = []
        self.native_pid = None
        env = {'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8', 'HOME': '/run/fixture/home',
               'XDG_RUNTIME_DIR': '/run/ewm', 'XDG_CONFIG_HOME': '/run/fixture/config',
               'XDG_DATA_HOME': '/run/fixture/data', 'XDG_CACHE_HOME': '/run/fixture/cache',
               'TMPDIR': '/run/fixture/tmp', 'NEOMACS_RUNTIME_ROOT': str(ROOT),
               'NEOMACS_LOG_FILE': '/run/fixture/runtime.log', 'RUST_MIN_STACK': '134217728',
               'NEOMACS_GUI_TEST_DIR': '/run/fixture/gates', 'LIBSEAT_BACKEND': 'disabled',
               'PROOF_MODULE': '/run/fixture/module.so', 'PROOF_OUTPUT': '/run/fixture/receipt.el'}
        if 'VK_DRIVER_FILES' in os.environ:
            env['VK_DRIVER_FILES'] = os.environ['VK_DRIVER_FILES']
        self.host_env = dict(env, HOME=str(self.root / 'home'), XDG_RUNTIME_DIR=str(self.runtime),
                             TMPDIR=str(self.root / 'tmp'))
        self.log = (self.root / 'launch.log').open('wb')
        argv = ['bwrap', '--die-with-parent', '--unshare-pid', '--unshare-net',
                '--ro-bind', '/', '/', '--tmpfs', '/home', '--tmpfs', '/run', '--tmpfs', '/tmp',
                '--dev', '/dev', '--proc', '/proc', '--ro-bind', str(ROOT), str(ROOT),
                '--ro-bind', str(binaries), str(binaries), '--bind', str(self.root), '/run/fixture',
                '--bind', str(self.runtime), '/run/ewm', '--dev-bind', render_node, render_node,
                '--chdir', str(ROOT), '/usr/bin/env', '-i',
                *[key + '=' + value for key, value in env.items()], str(binaries / 'neomacs'),
                '--dump-file', str(binaries / 'bootstrap-neomacs.pdump'), '-Q', '--fg-daemon=gui-faults']
        self.process = subprocess.Popen(argv, stdout=self.log, stderr=subprocess.STDOUT)
        try:
            self.discover_native()
        except BaseException:
            self.close()
            raise

    def discover_native(self):
        # bwrap is a namespace supervisor; signals go to the exact native child.
        wait_for(lambda: self.endpoint.exists(), self.process, 60)
        assert self.eval('(+ 20 22)') == '42'
        self.pid = int(self.eval('(emacs-pid)'))
        pending = [self.process.pid]
        native = []
        while pending:
            pid = pending.pop()
            if Path(f'/proc/{pid}/exe').resolve() == (self.bin / 'neomacs').resolve():
                native.append(pid)
            pending.extend(int(child) for child in Path(f'/proc/{pid}/task/{pid}/children').read_text().split())
        assert len(native) == 1, native
        self.native_pid = native[0]

    def argv(self, expression):
        return [str(self.bin / 'neomacsclient'), '-s', 'gui-faults', '-w', '30', '-e', expression]

    def eval(self, expression, timeout=40):
        result = subprocess.run(self.argv(expression), env=self.host_env, cwd=ROOT,
                                capture_output=True, text=True, timeout=timeout)
        self.records.append({'expression': expression, 'exit': result.returncode,
                             'stdout': result.stdout, 'stderr': result.stderr})
        assert result.returncode == 0, self.records[-1]
        return result.stdout.strip()

    def start_eval(self, expression):
        child = subprocess.Popen(self.argv(expression), env=self.host_env, cwd=ROOT,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.children.append(child)
        return child

    def finish(self, child, expected='t', timeout=40):
        stdout, stderr = child.communicate(timeout=timeout)
        assert child.returncode == 0 and stdout.decode().strip() == expected, (child.returncode, stdout, stderr)

    def fault(self, *names):
        for name in names:
            (self.gates / ('fault-' + name)).touch()

    def entered(self, name):
        wait_for(lambda: (self.gates / ('entered-' + name)).exists(), self.process)

    def release(self, name):
        (self.gates / ('release-' + name)).touch()

    def signal(self, sig):
        assert self.native_pid is not None
        os.kill(self.native_pid, sig)

    def stopped(self, status):
        assert self.process.wait(timeout=5) == status
        assert not self.endpoint.exists()
        assert not Path(f'/proc/{self.native_pid}').exists()

    def close(self):
        for child in self.children:
            if child.poll() is None:
                child.kill()
            child.communicate()
        if self.process.poll() is None:
            if self.native_pid is None:
                # No native identity was admitted. Stop/reap only our owned PID
                # namespace supervisor; never guess a child PID on setup failure.
                self.process.terminate()
            else:
                self.signal(signal.SIGTERM)
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                if self.native_pid is None:
                    self.process.kill()
                else:
                    self.signal(signal.SIGKILL)
                self.process.wait(timeout=5)
        self.log.close()
        (self.root / 'evaluations.json').write_text(json.dumps(self.records, indent=2))
        shutil.rmtree(self.runtime)


def make_frame():
    return '(make-frame \'((window-system . neo) (display . "wayland-ewm-proof") (name . "fault-control")))'


def caught(form):
    return f'(condition-case nil (progn {form} nil) (error t) (quit t))'


def first_selection(d):
    d.eval(f'(load {json.dumps(str(FIXTURE / "start.el"))} nil t)')
    d.eval("(setq first-old (list (selected-frame) (selected-window) (current-buffer)) first-hook nil first-server nil)")
    d.eval("(add-hook 'after-make-frame-functions (lambda (f) (setq first-hook (list f (selected-frame) (selected-window) (current-buffer)))))")
    assert d.eval('(progn (setq first-created ' + make_frame() + ') (equal first-old (list (selected-frame) (selected-window) (current-buffer))))') == 't'
    assert d.eval('(equal first-old (cdr first-hook))') == 't'
    d.eval('(select-frame first-created)')
    d.eval('(redisplay t)')
    wait_for(lambda: d.eval('(native-proof-mapped-count)') == '1', d.process)
    d.eval('(delete-frame first-created t)')
    wait_for(lambda: d.eval('(native-proof-mapped-count)') == '0', d.process)
    d.eval('(kill-emacs 0)')
    d.stopped(0)


def native_recovery(d):
    assert d.eval(f'(load {json.dumps(str(FIXTURE / "start.el"))} nil t)') == 't'
    d.eval('(setq fault-before (frame-list) fault-selection (list (selected-frame) (selected-window) (current-buffer)) fault-hooks nil)')
    d.eval('(add-hook \'after-make-frame-functions (lambda (frame) (push (list frame (selected-frame) (selected-window) (current-buffer)) fault-hooks)))')
    for fault in ['observer-reject', 'evaluator-font', 'input-bridge']:
        d.fault(fault)
        assert d.eval(caught(make_frame())) == 't', fault
        assert d.eval('(and (equal fault-before (frame-list)) (null fault-hooks) (equal fault-selection (list (selected-frame) (selected-window) (current-buffer))))') == 't'
    d.fault('abandon-opened', 'opened-queued')
    abandoned = d.start_eval(caught(make_frame()))
    d.entered('opened-queued')
    d.finish(abandoned)
    d.release('opened-queued')
    assert d.eval('(and (equal fault-before (frame-list)) (null fault-hooks))') == 't'
    # A healthy constructor completed, but the public deadline wins publication.
    d.fault('constructed')
    assert d.eval(caught(make_frame())) == 't'
    d.entered('constructed')
    assert d.eval('(and (equal fault-before (frame-list)) (null fault-hooks))') == 't'
    d.eval('(x-open-connection "wayland-ewm-proof")')
    for primary in [True, False]:
        d.fault('consumer')
        d.entered('consumer')
        d.fault('saturate', 'admit-fail')
        assert d.eval(caught(make_frame())) == 't'
        d.entered('admitted')
        d.entered('queue-64')
        d.release('consumer')
        assert d.eval('(and (= (native-proof-mapped-count) 0) (null fault-hooks) (equal fault-before (frame-list)) (equal fault-selection (list (selected-frame) (selected-window) (current-buffer))))') == 't'
        # The next gate must be fresh, not an old release/entry marker.
        for name in ['consumer', 'admitted', 'queue-64']:
            (d.gates / ('entered-' + name)).unlink(missing_ok=True)
            (d.gates / ('release-' + name)).unlink(missing_ok=True)
        if primary:
            for fault in ['gpu-error', 'gpu-start-error']:
                d.fault(fault)
                assert d.eval(caught(make_frame())) == 't'
                assert d.eval('(and (daemonp) (= (native-proof-mapped-count) 0) (null fault-hooks) (equal fault-before (frame-list)))') == 't'
        assert d.eval('(progn (setq fault-frame ' + make_frame() + ') (setq fault-direct-selection (list (selected-frame) (selected-window) (current-buffer))))') != 'nil'
        # Snapshot in the same evaluator turn: later native focus is allowed.
        selection = d.eval('(list fault-selection (cdr (car fault-hooks)) fault-direct-selection)')
        assert d.eval('(and (equal fault-selection (cdr (car fault-hooks))) (equal fault-selection fault-direct-selection))') == 't', selection
        d.eval('(select-frame-set-input-focus fault-frame)')
        d.eval('(redisplay t)')
        wait_for(lambda: d.eval('(native-proof-mapped-count)') == '1', d.process)
        assert int(d.eval('(emacs-pid)')) == d.pid
        assert d.eval('(progn (garbage-collect) (equal (ewm-hello) "Hello from EWM compositor!"))') == 't'
        d.eval('(delete-frame fault-frame t)')
        wait_for(lambda: d.eval('(native-proof-mapped-count)') == '0', d.process)
        d.eval('(setq fault-hooks nil)')
    d.eval('(setq fault-reexec-sentinel 123)')
    d.eval('(kill-emacs nil t)')
    d.entered('root-retired')
    def restarted():
        result = subprocess.run(d.argv('(+ 20 22)'), env=d.host_env, cwd=ROOT, capture_output=True, text=True, timeout=35)
        return result.returncode == 0 and result.stdout.strip() == '42'
    wait_for(restarted, d.process, 60)
    assert int(d.eval('(emacs-pid)')) == d.pid
    assert d.eval("(and (not (boundp 'fault-reexec-sentinel)) (null (x-display-list)))") == 't'
    d.eval('(kill-emacs 0)')
    d.stopped(0)


def cancellation(d, phase, action):
    peer = accepted = None
    try:
        if phase == 'registry':
            peer = socket.socket(socket.AF_UNIX)
            peer.bind(str(d.runtime / 'stalled'))
            peer.listen(1)
            peer.settimeout(20)
            form = '(x-open-connection "/run/ewm/stalled")'
        else:
            if phase == 'retained-font':
                d.eval(f'(load {json.dumps(str(FIXTURE / "start.el"))} nil t)')
                d.fault('observer-reject')
                assert d.eval(caught(make_frame())) == 't'
                d.fault('font-worker')
                form = '(x-open-connection "wayland-ewm-proof")'
            else:
                d.fault(phase)
                form = '(x-open-connection "/run/ewm/missing")'
        client = d.start_eval(caught(form))
        if peer:
            accepted, _ = peer.accept()
            accepted.settimeout(10)
            assert len(accepted.recv(4096)) == 24
        else:
            d.entered("font-worker" if phase == "retained-font" else phase)
        if action == 'deadline':
            d.finish(client, timeout=20)
            assert d.eval('(+ 20 22)') == '42'
            assert d.eval('(null (x-display-list))') == 't'
            d.eval('(kill-emacs 0)')
            d.stopped(0)
        elif action == 'quit':
            d.fault('lisp-quit')
            d.finish(client)
            assert d.eval('(+ 20 22)') == '42'
            d.eval('(kill-emacs 0)')
            d.stopped(0)
        else:
            d.signal(signal.SIGTERM if action == 'TERM' else signal.SIGHUP)
            d.stopped(15 if action == 'TERM' else 1)
        # Never release the fault peer or font/queue gate before process reaping.
    finally:
        if accepted:
            accepted.close()
        if peer:
            peer.close()


class WaylandProxy:
    """Forward genuine EWM protocol+SCM_RIGHTS; withhold replies, never invent them."""
    def __init__(self, d):
        self.d = d
        self.stop = threading.Event()
        self.request = threading.Event()
        # Address exact owned directory through a retained fd: descriptive
        # evidence paths must not exceed sockaddr_un's 108-byte host limit.
        self.runtime_fd = os.open(d.runtime, os.O_RDONLY | os.O_DIRECTORY)
        self.runtime_path = f'/proc/self/fd/{self.runtime_fd}'
        self.listener = socket.socket(socket.AF_UNIX)
        self.listener.bind(self.runtime_path + '/wayland-proxy')
        self.listener.listen(1)
        self.sockets = [self.listener]
        self.threads = [threading.Thread(target=self.accept, daemon=True)]
        self.threads[0].start()

    def accept(self):
        client, _ = self.listener.accept()
        upstream = socket.socket(socket.AF_UNIX)
        upstream.connect(self.runtime_path + '/wayland-ewm-proof')
        self.sockets.extend([client, upstream])
        for src, dst, replies in [(client, upstream, False), (upstream, client, True)]:
            thread = threading.Thread(target=self.forward, args=(src, dst, replies), daemon=True)
            self.threads.append(thread)
            thread.start()

    def forward(self, src, dst, replies):
        while not self.stop.is_set():
            if not select.select([src], [], [], .05)[0]:
                continue
            data, ancillary, _, _ = src.recvmsg(65536, socket.CMSG_SPACE(256 * 4))
            if not data:
                return
            fds = []
            for level, kind, value in ancillary:
                if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
                    items = array.array('i'); items.frombytes(value)
                    fds.extend(items)
            try:
                if not replies and (self.d.gates / 'entered-window-constructor').exists():
                    self.request.set()
                while replies and (self.d.gates / 'withhold-configure').exists() and (self.d.gates / 'entered-window-constructor').exists() and not self.stop.wait(.01):
                    pass
                if self.stop.is_set():
                    return
                sent = dst.sendmsg([data], ancillary)
                if sent < len(data):
                    dst.sendall(data[sent:])
            except (BrokenPipeError, ConnectionResetError):
                return
            finally:
                for fd in fds:
                    os.close(fd)

    def close(self):
        self.stop.set()
        for sock in self.sockets:
            try: sock.shutdown(socket.SHUT_RDWR)
            except OSError: pass
            sock.close()
        for thread in self.threads:
            thread.join(timeout=2)
            assert not thread.is_alive()
        os.close(self.runtime_fd)


def window_cancellation(d, secondary, action):
    d.eval(f'(load {json.dumps(str(FIXTURE / "start.el"))} nil t)')
    proxy = WaylandProxy(d)
    try:
        d.eval('(x-open-connection "wayland-proxy")')
        form = make_frame().replace('wayland-ewm-proof', 'wayland-proxy')
        if secondary:
            d.eval('(setq window-positive ' + form + ')')
            d.eval('(select-frame-set-input-focus window-positive)')
            d.eval('(redisplay t)')
            wait_for(lambda: d.eval('(native-proof-mapped-count)') == '1', d.process)
        d.eval('(setq window-before (frame-list) window-selected (list (selected-frame) (selected-window) (current-buffer)) window-hooks nil)')
        d.eval("(add-hook 'after-make-frame-functions (lambda (f) (push f window-hooks)))")
        (d.gates / 'entered-window-constructor').unlink(missing_ok=True)
        (d.gates / 'withhold-configure').touch()
        client = d.start_eval(caught(form))
        d.entered('window-constructor')
        assert proxy.request.wait(5), 'actual native constructor protocol not observed'
        if action in ['TERM', 'HUP']:
            d.signal(signal.SIGTERM if action == 'TERM' else signal.SIGHUP)
            d.stopped(15 if action == 'TERM' else 1)
        else:
            if action == 'quit':
                d.fault('frame-quit')
            d.finish(client, timeout=20)
            d.entered('connection-terminal')
            assert d.eval('(and (daemonp) (equal window-before (frame-list)) (null window-hooks) (equal window-selected (list (selected-frame) (selected-window) (current-buffer))))') == 't'
            # Socket interruption explicitly terminalizes the existing native
            # connection. A consumed winit singleton is never reconstructed.
            assert d.eval(caught(form)) == 't'
            assert int(d.eval('(emacs-pid)')) == d.pid
            d.eval('(kill-emacs 0)')
            d.stopped(0)
        # Withheld genuine replies are released only after bounded root reaping.
    finally:
        proxy.close()


def build_cases():
    cases = [('first-selection', first_selection), ('native-recovery', native_recovery)]
    cases += [(phase + '-' + action, lambda d, phase=phase, action=action: cancellation(d, phase, action))
              for phase in ['queued', 'registry', 'font-worker', 'retained-font', 'foreign-font-worker'] for action in ['TERM', 'HUP', 'quit', 'deadline']]
    cases += [('window-' + kind + '-' + action, lambda d, kind=kind, action=action: window_cancellation(d, kind == 'secondary', action))
              for kind in ['primary', 'secondary'] for action in ['TERM', 'HUP', 'quit', 'deadline']]
    return cases


def require_coverage(requested, passed):
    if passed != requested:
        raise RuntimeError(f'fault coverage mismatch: requested {requested}, passed {passed}')


def main():
    cases = build_cases()
    p = argparse.ArgumentParser()
    p.add_argument('--bin-dir', type=Path, required=True)
    p.add_argument('--module', type=Path, required=True)
    p.add_argument('--render-node', default='/dev/dri/renderD128')
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--only', choices=['all', *[name for name, _ in cases]], default='all')
    args = p.parse_args()
    validate_render_node(args.render_node)
    selected = [(name, run) for name, run in cases if args.only in ['all', name]]
    requested = [name for name, _ in selected]
    if not requested or len(set(requested)) != len(requested):
        raise RuntimeError('fault selection must contain distinct executable cases')
    for path in [args.module, args.bin_dir / 'neomacs', args.bin_dir / 'neomacsclient', args.bin_dir / 'bootstrap-neomacs.pdump']:
        if not path.is_file():
            raise RuntimeError(f'selected fault input missing: {path}')
    args.output.mkdir(parents=True, exist_ok=True)
    identities = {str(path): digest(path) for path in [args.module, args.bin_dir / 'neomacs', args.bin_dir / 'neomacsclient', args.bin_dir / 'bootstrap-neomacs.pdump']}
    receipt = {'input_hashes': identities, 'requested': requested, 'passed': []}
    try:
        for name, run in selected:
            root = args.output / name
            root.mkdir(exist_ok=False)
            d = None
            try:
                d = Daemon(args.bin_dir, args.module, args.render_node, root)
                run(d)
                receipt['passed'].append(name)
                (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2))
                print('PASS', name, flush=True)
            finally:
                if d:
                    d.close()
        require_coverage(requested, receipt['passed'])
        assert identities == {path: digest(Path(path)) for path in identities}
    finally:
        (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2))


if __name__ == '__main__':
    main()
