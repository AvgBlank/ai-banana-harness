//! The message input: text plus a cursor, and the one wrapping layout that rendering, the box's
//! height and vertical cursor movement all share, so they can never disagree.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Columns a tab occupies on screen; the stored text keeps the tab.
const TAB_WIDTH: u16 = 4;

#[derive(Debug, Default)]
pub struct InputBuffer {
    text: String,
    /// Byte offset into `text`, always on a grapheme-cluster boundary.
    cursor: usize,
    /// Bumped on every text change, so derived state (e.g. suggestion selection) can tell it's
    /// stale without every edit site having to reset it.
    revision: u64,
}

/// The input wrapped to a width.
pub struct InputLayout {
    /// Each visual row's byte range into the text (a row never includes the `'\n'` ending it).
    pub rows: Vec<Range<usize>>,
    pub cursor_row: usize,
    /// Display column of the cursor within its row.
    pub cursor_col: u16,
}

impl InputLayout {
    /// Rows the box needs, including the empty row a cursor past a full last row moves to.
    pub fn row_count(&self) -> usize {
        self.rows.len().max(self.cursor_row + 1)
    }
}

impl InputBuffer {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Replaces the text, with the cursor at the end.
    pub fn set(&mut self, text: String) {
        self.cursor = text.len();
        self.text = text;
        self.revision += 1;
    }

    pub fn clear(&mut self) {
        self.set(String::new());
    }

    pub fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.revision += 1;
    }

    /// Inserts at the cursor; pasted `\r\n` and lone `\r` line endings become `\n`.
    pub fn insert_str(&mut self, s: &str) {
        let s = s.replace("\r\n", "\n").replace('\r', "\n");
        self.text.insert_str(self.cursor, &s);
        self.cursor += s.len();
        self.revision += 1;
    }

    pub fn backspace(&mut self) {
        if let Some((start, _)) = self.grapheme_before(self.cursor) {
            self.remove(start..self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if let Some(g) = self.grapheme_after(self.cursor) {
            self.remove(self.cursor..self.cursor + g.len());
        }
    }

    pub fn move_left(&mut self) {
        if let Some((start, _)) = self.grapheme_before(self.cursor) {
            self.cursor = start;
        }
    }

    pub fn move_right(&mut self) {
        if let Some(g) = self.grapheme_after(self.cursor) {
            self.cursor += g.len();
        }
    }

    pub fn move_word_left(&mut self) {
        self.cursor = self.word_left_target();
    }

    pub fn move_word_right(&mut self) {
        self.cursor = self.word_right_target();
    }

    pub fn delete_word_left(&mut self) {
        self.remove(self.word_left_target()..self.cursor);
    }

    pub fn delete_word_right(&mut self) {
        self.remove(self.cursor..self.word_right_target());
    }

    pub fn move_line_start(&mut self) {
        self.cursor = self.line_start();
    }

    pub fn move_line_end(&mut self) {
        self.cursor = self.line_end();
    }

    pub fn delete_to_line_start(&mut self) {
        self.remove(self.line_start()..self.cursor);
    }

    pub fn delete_to_line_end(&mut self) {
        self.remove(self.cursor..self.line_end());
    }

    /// One visual row up at `width`, keeping the column; from the first row, to the start.
    pub fn move_up(&mut self, width: u16) {
        let layout = self.layout(width);
        self.cursor = match layout.cursor_row.checked_sub(1) {
            Some(row) => self.offset_in_row(&layout.rows[row], layout.cursor_col),
            None => 0,
        };
    }

    /// One visual row down at `width`, keeping the column; from the last row, to the end.
    pub fn move_down(&mut self, width: u16) {
        let layout = self.layout(width);
        self.cursor = match layout.rows.get(layout.cursor_row + 1) {
            Some(row) => self.offset_in_row(row, layout.cursor_col),
            None => self.text.len(),
        };
    }

    /// Greedy word wrap at `width` display columns (0 = don't wrap): a row breaks after the last
    /// whitespace that fits, a longer word hard-breaks, and `\n` always ends a row.
    pub fn layout(&self, width: u16) -> InputLayout {
        let mut rows = Vec::new();
        let mut line_start = 0;
        for line in self.text.split('\n') {
            wrap_line(WrapLine {
                text: &self.text,
                range: line_start..line_start + line.len(),
                width,
                rows: &mut rows,
            });
            line_start += line.len() + 1;
        }

        let mut cursor_row = rows.len().saturating_sub(1);
        let mut cursor_col = 0;
        for (i, row) in rows.iter().enumerate() {
            if self.cursor < row.start || self.cursor > row.end {
                continue;
            }
            // On a wrap boundary the cursor belongs to the start of the next row.
            if self.cursor == row.end && rows.get(i + 1).is_some_and(|n| n.start == row.end) {
                continue;
            }
            cursor_row = i;
            cursor_col = display_width(&self.text[row.start..self.cursor]);
            break;
        }
        if width > 0 && cursor_col >= width {
            if cursor_row + 1 == rows.len() {
                cursor_row += 1;
                cursor_col = 0;
            } else {
                cursor_col = width - 1;
            }
        }
        InputLayout {
            rows,
            cursor_row,
            cursor_col,
        }
    }

    fn remove(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        self.cursor = range.start;
        self.text.replace_range(range, "");
        self.revision += 1;
    }

    fn grapheme_before(&self, pos: usize) -> Option<(usize, &str)> {
        self.text[..pos].grapheme_indices(true).next_back()
    }

    fn grapheme_after(&self, pos: usize) -> Option<&str> {
        self.text[pos..].graphemes(true).next()
    }

    fn word_left_target(&self) -> usize {
        let mut pos = self.cursor;
        while let Some((start, _)) = self.grapheme_before(pos).filter(|(_, g)| !is_word(g)) {
            pos = start;
        }
        while let Some((start, _)) = self.grapheme_before(pos).filter(|(_, g)| is_word(g)) {
            pos = start;
        }
        pos
    }

    fn word_right_target(&self) -> usize {
        let mut pos = self.cursor;
        while let Some(g) = self.grapheme_after(pos).filter(|g| !is_word(g)) {
            pos += g.len();
        }
        while let Some(g) = self.grapheme_after(pos).filter(|g| is_word(g)) {
            pos += g.len();
        }
        pos
    }

    fn line_start(&self) -> usize {
        self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1)
    }

    fn line_end(&self) -> usize {
        self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |i| self.cursor + i)
    }

    /// The offset in `row` whose column is closest to `col` without passing it.
    fn offset_in_row(&self, row: &Range<usize>, col: u16) -> usize {
        let mut pos = row.start;
        let mut used = 0;
        for g in self.text[row.clone()].graphemes(true) {
            let w = grapheme_width(g);
            if used + w > col {
                break;
            }
            used += w;
            pos += g.len();
        }
        pos
    }
}

struct WrapLine<'a> {
    text: &'a str,
    range: Range<usize>,
    width: u16,
    rows: &'a mut Vec<Range<usize>>,
}

fn wrap_line(line: WrapLine<'_>) {
    let WrapLine {
        text,
        range,
        width,
        rows,
    } = line;
    let mut row_start = range.start;
    let mut row_width: u16 = 0;
    let mut last_break: Option<usize> = None;
    for (i, g) in text[range.clone()].grapheme_indices(true) {
        let at = range.start + i;
        let w = grapheme_width(g);
        if width > 0 && row_width.saturating_add(w) > width && at > row_start {
            if let Some(brk) = last_break.filter(|&b| b > row_start) {
                rows.push(row_start..brk);
                row_start = brk;
                row_width = display_width(&text[brk..at]);
            }
            if row_width.saturating_add(w) > width && at > row_start {
                rows.push(row_start..at);
                row_start = at;
                row_width = 0;
            }
            last_break = None;
        }
        row_width = row_width.saturating_add(w);
        if g.chars().all(char::is_whitespace) {
            last_break = Some(at + g.len());
        }
    }
    rows.push(row_start..range.end);
}

fn is_word(g: &str) -> bool {
    g.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn grapheme_width(g: &str) -> u16 {
    if g == "\t" {
        TAB_WIDTH
    } else {
        u16::try_from(g.width()).unwrap_or(u16::MAX)
    }
}

pub(crate) fn display_width(s: &str) -> u16 {
    s.graphemes(true)
        .map(grapheme_width)
        .fold(0, u16::saturating_add)
}
