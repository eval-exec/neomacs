#!/usr/bin/env python3
"""Fail-closed fault dispatch and device controls; never open host devices."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import stat
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + '.py'))
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


FAULTS = load_script('test-daemon-gui-faults')
PUBLIC = load_script('test-daemon-gui')


class FaultControls(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='gf-control-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for name in ['neomacs', 'neomacsclient', 'bootstrap-neomacs.pdump', 'module.so']:
            (self.root / name).write_text('unit-test input, never executable')
        self.output = self.root / 'output'

    def argv(self, selection='first-selection', device='/dev/dri/renderD128'):
        return ['faults', '--bin-dir', str(self.root), '--module', str(self.root / 'module.so'),
                '--output', str(self.output), '--only', selection, '--render-node', device]

    def rejected(self, selection='first-selection', device='/dev/dri/renderD128'):
        # Intercept both native owner and process boundaries. Unsafe arguments
        # must be rejected before either is reached, without device opens.
        with patch('sys.argv', self.argv(selection, device)), \
             patch.object(FAULTS, 'Daemon') as daemon, \
             patch.object(FAULTS.subprocess, 'Popen') as popen, \
             patch.object(FAULTS, 'digest', side_effect=AssertionError('premature input hashing')) as digest, \
             contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises((SystemExit, RuntimeError)):
                FAULTS.main()
            daemon.assert_not_called()
            popen.assert_not_called()
            digest.assert_not_called()
        self.assertFalse(self.output.exists())

    def test_unknown_selection_rejected_before_artifacts_or_output(self):
        self.rejected(selection='window-primary-TERMM')

    def test_unknown_selection_preserves_existing_receipt(self):
        self.output.mkdir()
        receipt = self.output / 'receipt.json'
        receipt.write_bytes(b'previous evidence')
        with patch('sys.argv', self.argv('window-primary-TERMM')), \
             patch.object(FAULTS, 'Daemon') as daemon, \
             patch.object(FAULTS.subprocess, 'Popen') as popen, \
             contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):
                FAULTS.main()
            daemon.assert_not_called()
            popen.assert_not_called()
        self.assertEqual(receipt.read_bytes(), b'previous evidence')
        self.assertEqual(list(self.output.iterdir()), [receipt])

    def test_card_input_and_ordinary_file_rejected_without_launch(self):
        for device in ['/dev/dri/card1', '/dev/input/event0', str(self.root / 'module.so'),
                       '/dev/dri/renderD128/../card1', '/dev/dri/renderD128extra']:
            with self.subTest(device=device):
                self.rejected(device=device)

    def test_render_name_requires_native_character_device(self):
        for mode, device in [(stat.S_IFREG, os.makedev(226, 128)),
                             (stat.S_IFLNK, os.makedev(226, 128)),
                             (stat.S_IFCHR, os.makedev(13, 128)),
                             (stat.S_IFCHR, os.makedev(226, 0)),
                             (stat.S_IFCHR, os.makedev(226, 129))]:
            info = SimpleNamespace(st_mode=mode, st_rdev=device)
            with self.subTest(mode=mode, device=device), patch.object(Path, 'lstat', return_value=info):
                self.rejected()

    def test_missing_render_node_rejected_before_output(self):
        with patch.object(Path, 'lstat', side_effect=FileNotFoundError):
            self.rejected()

    def test_public_boundary_rejects_card_before_oracle_or_launch(self):
        with patch.object(PUBLIC, 'checked') as checked, \
             patch.object(PUBLIC.subprocess, 'Popen') as popen:
            with self.assertRaises(RuntimeError):
                PUBLIC.acceptance(self.root, self.root / 'module.so', self.root / 'neomacsclient',
                                  '/dev/dri/card1')
            checked.assert_not_called()
            popen.assert_not_called()

    def test_empty_or_duplicate_all_catalog_rejected_before_output(self):
        render = SimpleNamespace(st_mode=stat.S_IFCHR, st_rdev=os.makedev(226, 128))
        for cases in [[], [('same', lambda d: None), ('same', lambda d: None)]]:
            with self.subTest(cases=cases), \
                 patch.object(Path, 'lstat', return_value=render), \
                 patch.object(FAULTS, 'build_cases', return_value=cases):
                self.rejected(selection='all')

    def test_valid_single_and_all_dispatch_exact_coverage(self):
        render = SimpleNamespace(st_mode=stat.S_IFCHR, st_rdev=os.makedev(226, 128))
        names = [name for name, _ in FAULTS.build_cases()]
        self.assertEqual(len(names), 30)
        self.assertEqual(len(set(names)), len(names))
        for selection in ['first-selection', 'window-primary-TERM', 'all']:
            expected = names if selection == 'all' else [selection]
            calls = []
            cases = [(name, lambda d, name=name: calls.append(name)) for name in names]
            self.output = self.root / selection
            with patch('sys.argv', self.argv(selection)), \
                 patch.object(Path, 'lstat', return_value=render), \
                 patch.object(FAULTS, 'build_cases', return_value=cases), \
                 patch.object(FAULTS, 'Daemon') as daemon, \
                 patch.object(FAULTS, 'require_coverage', wraps=FAULTS.require_coverage) as coverage, \
                 contextlib.redirect_stdout(io.StringIO()):
                FAULTS.main()
            coverage.assert_called_once_with(expected, expected)
            receipt = json.loads((self.output / 'receipt.json').read_text())
            self.assertEqual(calls, expected)
            self.assertEqual(receipt['requested'], expected)
            self.assertEqual(receipt['passed'], expected)
            self.assertEqual(daemon.call_count, len(expected))

    def test_coverage_guard_rejects_missing_extra_duplicate_or_reordered(self):
        for actual in [[], ['first'], ['first', 'second', 'extra'],
                       ['first', 'first'], ['second', 'first']]:
            with self.subTest(actual=actual), self.assertRaises(RuntimeError):
                FAULTS.require_coverage(['first', 'second'], actual)
        FAULTS.require_coverage(['first', 'second'], ['first', 'second'])


if __name__ == '__main__':
    unittest.main()
