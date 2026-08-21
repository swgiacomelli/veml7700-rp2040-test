//! CR/LF line editor with local echo and backspace.

/// Result of feeding one input byte into the line buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEvent {
    None,
    Echo(u8),
    Backspace,
    Submit,
}

/// Printable-ASCII line buffer with CRLF coalescing.
pub struct LineBuffer<const N: usize> {
    buf: [u8; N],
    len: usize,
    saw_cr: bool,
}

impl<const N: usize> LineBuffer<N> {
    pub const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
            saw_cr: false,
        }
    }

    pub fn feed(&mut self, byte: u8) -> LineEvent {
        match byte {
            b'\r' => {
                self.saw_cr = true;
                LineEvent::Submit
            }
            b'\n' => {
                if self.saw_cr {
                    self.saw_cr = false;
                    LineEvent::None
                } else {
                    LineEvent::Submit
                }
            }
            0x08 | 0x7f => {
                self.saw_cr = false;
                if self.len > 0 {
                    self.len -= 1;
                    LineEvent::Backspace
                } else {
                    LineEvent::None
                }
            }
            0x20..=0x7e => {
                self.saw_cr = false;
                if self.len < N {
                    self.buf[self.len] = byte;
                    self.len += 1;
                    LineEvent::Echo(byte)
                } else {
                    LineEvent::None
                }
            }
            _ => {
                self.saw_cr = false;
                LineEvent::None
            }
        }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    /// Clear the collected line. Leaves the CRLF flag so a following LF is
    /// not treated as a second submit.
    pub fn clear(&mut self) {
        self.len = 0;
    }
}
