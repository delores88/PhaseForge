"""Deterministic taskkill races; no process is launched or unrelated PID killed."""
import subprocess
import unittest
from unittest.mock import Mock
from runtime_smoke import wait_for_windows_tree_exit


class WindowsCleanupTests(unittest.TestCase):
    def test_exited_helper_error_waits_for_owned_backend_to_finish(self):
        process = Mock()
        process.poll.return_value = None  # The old immediate poll raised here.
        process.wait.return_value = 1
        stopped = subprocess.CompletedProcess(['taskkill'], 128, '', 'Child already exited')
        wait_for_windows_tree_exit(process, stopped)
        process.wait.assert_called_once_with(timeout=10)

    def test_still_running_backend_preserves_tree_failure_and_diagnostic(self):
        process = Mock()
        process.wait.side_effect = subprocess.TimeoutExpired('owned backend', 10)
        stopped = subprocess.CompletedProcess(['taskkill'], 1, '', 'Access denied')
        with self.assertRaisesRegex(RuntimeError, 'shutdown failed: Access denied') as failure:
            wait_for_windows_tree_exit(process, stopped)
        self.assertIsInstance(failure.exception.__cause__, subprocess.TimeoutExpired)
        process.wait.assert_called_once_with(timeout=10)

    def test_successful_taskkill_does_not_hide_a_backend_that_never_exits(self):
        process = Mock()
        process.wait.side_effect = subprocess.TimeoutExpired('owned backend', 10)
        with self.assertRaises(subprocess.TimeoutExpired):
            wait_for_windows_tree_exit(process, subprocess.CompletedProcess(['taskkill'], 0, '', ''))


if __name__ == '__main__':
    unittest.main()
