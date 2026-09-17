import unittest

from pty_screen import Screen


class ScreenTests(unittest.TestCase):
    def test_incremental_redraw_keeps_unchanged_title_characters(self):
        screen = Screen(40, 4)
        screen.feed(b"SSH Host Informatixn")
        screen.feed(b"\x1b[1;19Ho")
        self.assertIn("SSH Host Information", screen.text())

    def test_fragmented_sequences_and_utf8_address_terminal_columns(self):
        screen = Screen(20, 3)
        for byte in "\x1b[2;2H界é\x1b[2;6Hok".encode():
            screen.feed(bytes([byte]))
        self.assertEqual(screen.text().splitlines()[1], " 界é ok" + " " * 13)

    def test_erase_and_clear_remove_stale_view_text(self):
        screen = Screen(24, 3)
        screen.feed(b"SSH Host Information\x1b[1;1HCommands\x1b[K")
        self.assertIn("Commands", screen.text())
        self.assertNotIn("Host Information", screen.text())
        screen.feed(b"\x1b[2J")
        self.assertFalse(screen.text().strip())

    def test_ssh_takeover_returns_to_primary_screen(self):
        screen = Screen(24, 3)
        screen.feed(b"primary\x1b[?1049hUI title\x1b[?1049l\r\nSSH_TAKEOVER")
        self.assertIn("primary", screen.text())
        self.assertIn("SSH_TAKEOVER", screen.text())
        self.assertNotIn("UI title", screen.text())


if __name__ == "__main__":
    unittest.main()
