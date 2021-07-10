pub mod error;
pub mod packet;

use std::time::Duration;

use crate::error::{Error, Result};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::timeout,
};
use tokio_serial::{ClearBuffer, Serial, SerialPort, SerialPortSettings};

/// Interface for interacting with ELM327.
pub struct Elm327 {
    port: Serial,
}

impl Elm327 {
    /// Builds a new ELM327 interface with an already connected serial port.
    /// It is recommendede to use the [`from_path`](Elm327::from_path) function instead.
    pub fn new(port: Serial) -> Self {
        Self { port }
    }

    /// Builds a new ELM327 interface by connecting to the given port.
    ///
    /// # Parameters
    ///
    /// * `path` - Serial port path.
    /// * `settings` - Serial port settings.
    /// * `retry` - Number of times to re-attempt reconnection before failing.
    pub async fn from_path(
        path: impl AsRef<str>,
        settings: &SerialPortSettings,
        retry: Option<u32>,
    ) -> Result<Self> {
        let path = path.as_ref();
        let retry = retry.unwrap_or(3);
        let mut n = 0;

        loop {
            if n >= retry {
                break Err(Error::TimedOut);
            }

            let port = if let Ok(port) =
                Serial::from_path(path, settings).map_err(|e| Error::Serial(e.into()))
            {
                port
            } else {
                n += 1;
                continue;
            };

            let mut elm = Self::new(port);

            for _ in 0..n {
                let _ = timeout(Duration::from_millis(500), elm.read(|_| true)).await;
            }

            match elm.write_timeout("ATZ", Duration::from_secs(3)).await {
                Ok(r) => {
                    dbg!(r);
                    break Ok(elm);
                }
                Err(_) => {
                    n += 1;
                    continue;
                }
            }
        }
    }

    /// Sends a command to the ELM327 device, returning a vector of strings as response.
    /// A carraige return will automatically be appended to the command.
    /// The function will only return once the `>` character has been seen or one of the steps
    /// results in an error.
    ///
    /// # Parameters
    ///
    /// * `command` - Command to send.
    pub async fn write(&mut self, command: impl AsRef<str>) -> Result<Vec<String>> {
        self.write_no_resp(command).await?;
        self.read(|_| true).await
    }

    /// Sends a command to the ELM327 device, returning a vector of strings as response.
    ///
    /// Similar to the [`write`](Elm327::write) function except that after a given duration the
    /// future will resolve as a timed out error.
    ///
    /// # Parameters
    ///
    /// * `command` - Command to send.
    /// * `time` - Timeout to use.
    pub async fn write_timeout(
        &mut self,
        command: impl AsRef<str>,
        time: Duration,
    ) -> Result<Vec<String>> {
        tokio::time::timeout(time, self.write(command))
            .await
            .map_err(|_| Error::TimedOut)?
    }

    /// Sends a command to the ELM327 device, returning a vector of strings as response.
    ///
    /// Similar to the [`write_timeout`](Elm327::write_timeout) function except that after the
    /// timeout, it will retry up to `retry` given times.
    ///
    /// # Parameters
    ///
    /// * `command` - Command to send.
    /// * `time` - Timeout to use.
    /// * `retry` - Number of times to retry.
    pub async fn write_retry(
        &mut self,
        command: impl AsRef<str>,
        time: Duration,
        retry: u32,
    ) -> Result<Vec<String>> {
        let mut n = 1;
        let command = command.as_ref();
        loop {
            match self.write_timeout(command, time).await {
                Ok(x) => return Ok(x),
                Err(e) => match e {
                    Error::TimedOut => {
                        if n >= retry {
                            return Err(Error::TimedOut);
                        }

                        n += 1;
                    }
                    e => return Err(e),
                },
            }
        }
    }

    /// Sends a command to the ELM327 device, but does not return a response.
    ///
    /// # Parameters
    ///
    /// * `command` - The command to send.
    pub async fn write_no_resp(&mut self, command: impl AsRef<str>) -> Result<()> {
        let cmd = format!("{}\r", command.as_ref());

        self.port
            .clear(ClearBuffer::Output)
            .map_err(|_| Error::Clear)?;
        self.port
            .write(cmd.as_bytes())
            .await
            .map_err(|_| Error::Write)?;
        self.port.flush().await.map_err(|_| Error::Flush)?;

        Ok(())
    }

    /// Runs the 'Monitor all' command on the ELM327 device.
    ///
    /// You must provide a function `on_str` to be called each time a string is read from the
    /// device. The function must return a boolean which will decide if the monitoring will
    /// continue. If the function returns `true`, it will keep listening for messages from the
    /// device until it sees a '>' character. If it returns `false`, it will return a vector
    /// of strings that it has seen.
    ///
    /// # Parameters
    ///
    /// * `on_str` - The function to call when a string is received from the device.
    pub async fn monitor_all(&mut self, on_str: impl Fn(&str) -> bool) -> Result<Vec<String>> {
        self.write_no_resp("ATMA").await?;
        let resp = self.read(on_str).await;
        self.write("").await?; // stop monitoring
        resp
    }

    /// Reads from the serial port. Returns a vector of strings which have been read from the
    /// device. Returns once the '>' character has been read from the device.
    ///
    /// You must provide a function `on_str` to be called each time a string is read from the
    /// device. The function must return a boolean which will decide if the monitoring will
    /// continue. If the function returns `true`, it will keep listening for messages from the
    /// device until it sees a '>' character. If it returns `false`, it will return a vector
    /// of strings that it has seen.
    ///
    /// # Parameters
    ///
    /// * `on_str` - The function to call when a string has been seen.
    pub async fn read(&mut self, on_str: impl Fn(&str) -> bool) -> Result<Vec<String>> {
        self.port
            .clear(ClearBuffer::Input)
            .map_err(|_| Error::Clear)?;

        let mut buf = vec![];
        let mut strs = vec![];
        let mut char = [0u8];

        loop {
            self.port.read(&mut char).await.map_err(|_| Error::Read)?;

            match char[0] {
                b'\r' | b'\n' | b'>' => {
                    if !buf.is_empty() {
                        if let Ok(str) = String::from_utf8(buf) {
                            strs.push(str.clone());

                            if !on_str(&str) {
                                break;
                            }
                        }
                    }

                    buf = vec![];

                    match char[0] {
                        b'>' => break,
                        _ => continue,
                    }
                }
                _ => buf.push(char[0]),
            }
        }

        Ok(strs)
    }
}
