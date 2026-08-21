//! Byte transport for the console, plus a CDC-ACM adapter.

use core::fmt::{self, Write as _};

use embassy_usb::class::cdc_acm::CdcAcmClass;
use embassy_usb::driver::{Driver, EndpointError};
use heapless::String;

const CRLF: &str = "\r\n";

/// Async byte pipe used by the line editor and command handlers.
///
/// Futures are not `Send`: embassy-usb CDC uses `RefCell` internally.
#[allow(async_fn_in_trait)]
pub trait Transport {
    type Error;

    async fn write_all(&mut self, data: &[u8]) -> Result<(), Self::Error>;
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error>;
}

/// Write a UTF-8 string through `io`.
pub async fn write_str<T: Transport>(io: &mut T, s: &str) -> Result<(), T::Error> {
    io.write_all(s.as_bytes()).await
}

/// Write a CR/LF line terminator.
pub async fn write_newline<T: Transport>(io: &mut T) -> Result<(), T::Error> {
    io.write_all(CRLF.as_bytes()).await
}

/// Write `s` followed by CR/LF.
pub async fn writeln_str<T: Transport>(io: &mut T, s: &str) -> Result<(), T::Error> {
    write_str(io, s).await?;
    write_newline(io).await
}

/// Format into a 192-byte stack buffer, then write. Overlong output is truncated.
async fn write_fmt<T: Transport>(io: &mut T, args: fmt::Arguments<'_>) -> Result<(), T::Error> {
    let mut buf = String::<192>::new();
    let _ = buf.write_fmt(args);
    io.write_all(buf.as_bytes()).await
}

/// Format a line into a 192-byte stack buffer, append CR/LF, then write.
pub async fn writeln_fmt<T: Transport>(
    io: &mut T,
    args: fmt::Arguments<'_>,
) -> Result<(), T::Error> {
    write_fmt(io, args).await?;
    write_newline(io).await
}

/// CDC-ACM transport. Writes are chunked at 63 bytes to stay under the 64-byte
/// max packet and avoid a zero-length terminator.
pub struct CdcAcmTransport<'d, D: Driver<'d>> {
    class: CdcAcmClass<'d, D>,
}

impl<'d, D: Driver<'d>> CdcAcmTransport<'d, D> {
    pub fn new(class: CdcAcmClass<'d, D>) -> Self {
        Self { class }
    }

    pub async fn wait_connection(&mut self) {
        self.class.wait_connection().await
    }
}

impl<'d, D: Driver<'d>> Transport for CdcAcmTransport<'d, D> {
    type Error = EndpointError;

    async fn write_all(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        for chunk in data.chunks(63) {
            self.class.write_packet(chunk).await?;
        }
        Ok(())
    }

    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.class.read_packet(buf).await
    }
}
