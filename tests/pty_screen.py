"""Screen reader for the ASCII PTY fixtures and Ratatui's cursor-addressed output.

This bounded VT subset handles the controls used by Crossterm here, basic wide
and combining glyphs, and fragmented input. It is not a general emulator (joined
emoji, for example, are outside these fixtures). Unsupported escape controls
raise instead of silently allowing a false screen assertion.
"""
import codecs
import re
import unicodedata

CSI = re.compile(r"\x1b\[([0-?]*)([ -/]*)([@-~])")


class Screen:
    def __init__(self, width=120, height=40):
        self.width = width
        self.height = height
        self.cells = [[" "] * width for _ in range(height)]
        self.x = self.y = 0
        self.primary = None
        self.pending = ""
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")

    def feed(self, data):
        self.pending += self.decoder.decode(data)
        while self.pending:
            if self.pending.startswith("\x1b["):
                sequence = CSI.match(self.pending)
                if sequence is None:
                    return
                self.control(*sequence.groups())
                self.pending = self.pending[sequence.end():]
                continue
            if self.pending.startswith("\x1b]"):
                end = re.search(r"\x07|\x1b\\", self.pending)
                if end is None:
                    return
                self.pending = self.pending[end.end():]
                continue
            char = self.pending[0]
            if char == "\x1b":
                if len(self.pending) == 1:
                    return
                raise AssertionError(f"unsupported terminal escape: {self.pending[:20]!r}")
            self.pending = self.pending[1:]
            if char == "\r":
                self.x = 0
            elif char == "\n":
                self.y += 1
                self.scroll()
            elif char == "\b":
                self.x = max(0, self.x - 1)
            elif char == "\t":
                self.x = min(self.width - 1, (self.x // 8 + 1) * 8)
            elif char >= " ":
                columns = 0 if unicodedata.combining(char) else (2 if unicodedata.east_asian_width(char) in "WF" else 1)
                if columns == 0:
                    self.cells[self.y][max(0, self.x - 1)] += char
                    continue
                if self.x + columns > self.width:
                    self.x = 0
                    self.y += 1
                    self.scroll()
                self.cells[self.y][self.x] = char
                if columns == 2:
                    self.cells[self.y][self.x + 1] = ""
                self.x += columns

    def scroll(self):
        if self.y >= self.height:
            self.cells.pop(0)
            self.cells.append([" "] * self.width)
            self.y = self.height - 1

    def control(self, parameters, intermediate, command):
        values = [int(value or 0) for value in parameters.lstrip("?").split(";")]
        first = values[0]
        if command in "Hf":
            self.y = min(self.height - 1, max(1, first) - 1)
            self.x = min(self.width - 1, max(1, values[1] if len(values) > 1 else 1) - 1)
        elif command in "ABCD":
            amount = first or 1
            if command == "A":
                self.y = max(0, self.y - amount)
            elif command == "B":
                self.y = min(self.height - 1, self.y + amount)
            elif command == "C":
                self.x = min(self.width - 1, self.x + amount)
            else:
                self.x = max(0, self.x - amount)
        elif command == "G":
            self.x = min(self.width - 1, max(1, first) - 1)
        elif command in "JK":
            cursor = self.y * self.width + min(self.x, self.width - 1)
            start, end = (0, self.width * self.height) if command == "J" else (self.y * self.width, (self.y + 1) * self.width)
            if first == 0:
                start = cursor
            elif first == 1:
                end = cursor + 1
            elif first not in (2, 3):
                raise AssertionError(f"unsupported erase mode: {first}")
            for index in range(start, end):
                self.cells[index // self.width][index % self.width] = " "
        elif command in "hl" and parameters.startswith("?"):
            if 1049 in values:
                if command == "h" and self.primary is None:
                    self.primary = (self.cells, self.x, self.y)
                    self.cells = [[" "] * self.width for _ in range(self.height)]
                    self.x = self.y = 0
                elif command == "l" and self.primary is not None:
                    self.cells, self.x, self.y = self.primary
                    self.primary = None
        elif command == "m" or (command == "q" and intermediate == " "):
            pass  # Color, emphasis and cursor shape do not change visible text.
        else:
            raise AssertionError(f"unsupported terminal control: {parameters}{intermediate}{command}")

    def text(self):
        return "\n".join("".join(row) for row in self.cells)
