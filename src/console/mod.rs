//! Copyable no_std USB CDC console.
//!
//! Copy this directory (`src/console/`) into a new firmware crate and declare
//! `mod console;` — the module name must stay `console` (macros use
//! `$crate::console`). Then, in the new app:
//!
//! 1. Call [`usb::new`] and spawn [`usb::usb_task`] with your VID/PID/strings.
//! 2. Wrap the CDC class in [`CdcAcmTransport`].
//! 3. Implement command methods on an app type.
//! 4. Invoke [`match_commands`] to bind names to those methods.
//! 5. `Console::run` after `wait_connection`.
//!
//! Application commands live outside this directory.

mod line;
mod transport;
pub mod usb;

use heapless::String;

pub use line::{LineBuffer, LineEvent};
pub use transport::{
    CdcAcmTransport, Transport, write_newline, write_str, writeln_fmt, writeln_str,
};

/// Write a console line (CR/LF terminated). `writeln!(io)` writes a blank line.
macro_rules! writeln {
    ($io:expr) => {
        $crate::console::write_newline($io)
    };
    ($io:expr, $($arg:tt)*) => {
        $crate::console::writeln_fmt($io, ::core::format_args!($($arg)*))
    };
}
pub(crate) use writeln;

/// One application command shown by the built-in `help` listing.
pub struct Command {
    pub name: &'static str,
    /// Description for firmware that wants richer help than the name list.
    #[allow(dead_code)]
    pub summary: &'static str,
}

/// Firmware-specific commands. `help` is provided by [`Console`], not the app.
#[allow(async_fn_in_trait)]
pub(crate) trait App {
    fn commands(&self) -> &'static [Command];

    async fn handle<T: Transport>(
        &mut self,
        name: &str,
        args: &str,
        io: &mut T,
    ) -> Result<(), T::Error>;
}

/// Implement [`App`] by matching command names to methods.
///
/// Invoke from `main.rs` (or any crate-root module). `Command` holds only
/// name and summary; the match is generated here.
///
/// ```ignore
/// match_commands! {
///     <I2C: I2c> for VemlApp<'_, I2C> {
///         "scan", "probe addresses" => cmd_scan,
///         "id", "read the ID register" => cmd_id,
///     }
/// }
/// ```
macro_rules! match_commands {
    (
        <$t:ident: $bound:path> for $app:ty {
            $(
                $name:literal, $summary:literal => $method:ident
            ),+ $(,)?
        }
    ) => {
        const COMMANDS: &[$crate::console::Command] = &[
            $(
                $crate::console::Command {
                    name: $name,
                    summary: $summary,
                }
            ),+
        ];

        impl<$t: $bound> $crate::console::App for $app {
            fn commands(&self) -> &'static [$crate::console::Command] {
                COMMANDS
            }

            async fn handle<T: $crate::console::Transport>(
                &mut self,
                name: &str,
                _args: &str,
                io: &mut T,
            ) -> Result<(), T::Error> {
                match name {
                    $($name => self.$method(io).await,)+
                    _ => Ok(()),
                }
            }
        }
    };
}
pub(crate) use match_commands;

/// Interactive session: banner, prompt, line editor, dispatch, built-in help.
pub struct Console {
    banner: &'static str,
    prompt: &'static str,
}

impl Console {
    pub const fn new(banner: &'static str, prompt: &'static str) -> Self {
        Self { banner, prompt }
    }

    pub async fn run<T: Transport, A: App>(&self, io: &mut T, app: &mut A) -> Result<(), T::Error> {
        write_newline(io).await?;
        writeln_str(io, self.banner).await?;
        write_str(io, self.prompt).await?;

        let mut line = LineBuffer::<64>::new();
        let mut packet = [0u8; 64];

        loop {
            let n = io.read(&mut packet).await?;
            for &b in &packet[..n] {
                match line.feed(b) {
                    LineEvent::None => {}
                    LineEvent::Echo(c) => io.write_all(&[c]).await?,
                    LineEvent::Backspace => write_str(io, "\x08 \x08").await?,
                    LineEvent::Submit => {
                        write_newline(io).await?;
                        dispatch(io, app, line.as_str()).await?;
                        line.clear();
                        write_str(io, self.prompt).await?;
                    }
                }
            }
        }
    }
}

async fn dispatch<T: Transport, A: App>(
    io: &mut T,
    app: &mut A,
    raw: &str,
) -> Result<(), T::Error> {
    let line = raw.trim();
    if line.is_empty() {
        return Ok(());
    }

    let (name, args) = match line.split_once(char::is_whitespace) {
        Some((name, rest)) => (name, rest.trim_start()),
        None => (line, ""),
    };

    if name == "help" {
        return write_help(io, app.commands()).await;
    }

    if app.commands().iter().any(|c| c.name == name) {
        return app.handle(name, args, io).await;
    }

    writeln!(io, "unknown: {}", line).await
}

async fn write_help<T: Transport>(io: &mut T, commands: &[Command]) -> Result<(), T::Error> {
    let mut buf = String::<192>::new();
    let _ = buf.push_str("help");
    for cmd in commands {
        let _ = buf.push_str(" | ");
        let _ = buf.push_str(cmd.name);
    }
    writeln_str(io, &buf).await
}
