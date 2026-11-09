#[cfg(windows)]
mod platform {
    use anyhow::{Context, Result};
    use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
    use std::io::{Read, Write};

    pub struct PtySession {
        master: Box<dyn MasterPty + Send>,
        reader: Option<Box<dyn Read + Send>>,
        writer: Option<Box<dyn Write + Send>>,
        child: Box<dyn Child + Send + Sync>,
    }

    impl PtySession {
        pub fn open(command: &[String], cols: u16, rows: u16) -> Result<Self> {
            let pty_system = native_pty_system();
            let pair = pty_system
                .openpty(PtySize {
                    rows: rows.max(1),
                    cols: cols.max(1),
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .context("openpty")?;
            let mut cmd = CommandBuilder::new(&command[0]);
            for arg in &command[1..] {
                cmd.arg(arg);
            }
            cmd.env("TERM", "xterm-256color");
            cmd.env("COLORTERM", "truecolor");
            let child = pair.slave.spawn_command(cmd).context("spawn command")?;
            drop(pair.slave);
            let reader = pair.master.try_clone_reader().context("pty reader")?;
            let mut writer = pair.master.take_writer().context("pty writer")?;
            // ConPTY requests an inherited cursor position before the child can run.
            // Complete that transport handshake even with read-only/no browser input.
            writer
                .write_all(b"\x1b[1;1R")
                .context("ConPTY cursor handshake")?;
            writer.flush()?;
            Ok(Self {
                master: pair.master,
                reader: Some(reader),
                writer: Some(writer),
                child,
            })
        }

        pub fn take_reader(&mut self) -> Box<dyn Read + Send> {
            self.reader.take().expect("reader already taken")
        }

        pub fn take_writer(&mut self) -> Box<dyn Write + Send> {
            self.writer.take().expect("writer already taken")
        }

        pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
            self.master.resize(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })?;
            Ok(())
        }

        pub fn has_exited(&mut self) -> bool {
            self.child.try_wait().map(|s| s.is_some()).unwrap_or(true)
        }

        pub fn kill(&mut self) {
            let _ = self.child.kill();
        }
    }
}

#[cfg(unix)]
mod platform {
    use anyhow::{Context, Result};
    use std::ffi::{CStr, CString};
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command, Stdio};

    // Darwin and the BSDs type the ioctl request differently from Linux, so
    // each family gets its own call instead of a single shared constant.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn set_controlling_tty() -> libc::c_int {
        unsafe { libc::ioctl(0, libc::TIOCSCTTY, 0) }
    }

    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    fn set_controlling_tty() -> libc::c_int {
        // Darwin declares TIOCSCTTY as u32 while ioctl takes a c_ulong request.
        #[allow(clippy::unnecessary_cast)]
        let request = libc::TIOCSCTTY as libc::c_ulong;
        unsafe { libc::ioctl(0, request, 0) }
    }

    /// Slave device path for a PTY master. The caller passes it to `open`
    /// immediately, so the name is copied out of any static buffer here.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn slave_name(fd: libc::c_int) -> Result<CString> {
        let mut name = [0 as libc::c_char; 1024];
        let rc = unsafe { libc::ptsname_r(fd, name.as_mut_ptr(), name.len()) };
        if rc != 0 {
            anyhow::bail!("ptsname_r: {rc}");
        }
        Ok(unsafe { CStr::from_ptr(name.as_ptr()) }.to_owned())
    }

    /// `ptsname` returns a pointer into a static buffer and is not
    /// documented as thread-safe, so copy under a lock.
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    fn slave_name(fd: libc::c_int) -> Result<CString> {
        static NAME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = NAME_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let ptr = unsafe { libc::ptsname(fd) };
        if ptr.is_null() {
            return Err(std::io::Error::last_os_error()).context("ptsname");
        }
        Ok(unsafe { CStr::from_ptr(ptr) }.to_owned())
    }

    pub struct PtySession {
        master: File,
        child: Child,
        reader: Option<File>,
        writer: Option<File>,
    }

    impl PtySession {
        pub fn open(command: &[String], cols: u16, rows: u16) -> Result<Self> {
            anyhow::ensure!(!command.is_empty(), "empty command");
            // All PTY descriptors are close-on-exec, including clones made by File.
            let fd = unsafe {
                libc::posix_openpt(
                    libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC | libc::O_NONBLOCK,
                )
            };
            if fd < 0 {
                return Err(std::io::Error::last_os_error()).context("posix_openpt");
            }
            let master = unsafe { File::from_raw_fd(fd) };
            if unsafe { libc::grantpt(fd) } != 0 || unsafe { libc::unlockpt(fd) } != 0 {
                return Err(std::io::Error::last_os_error()).context("prepare PTY");
            }
            let name = slave_name(fd)?;
            let slave_fd = unsafe {
                libc::open(
                    name.as_ptr(),
                    libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
                )
            };
            if slave_fd < 0 {
                return Err(std::io::Error::last_os_error()).context("open PTY slave");
            }
            let slave = unsafe { File::from_raw_fd(slave_fd) };
            let size = libc::winsize {
                ws_row: rows.max(1),
                ws_col: cols.max(1),
                ws_xpixel: 0,
                ws_ypixel: 0,
            };
            if unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &size) } != 0 {
                return Err(std::io::Error::last_os_error()).context("initial PTY size");
            }
            let reader = master.try_clone()?;
            let writer = master.try_clone()?;
            let mut cmd = Command::new(&command[0]);
            cmd.args(&command[1..])
                .env("TERM", "xterm-256color")
                .env("COLORTERM", "truecolor")
                .stdin(Stdio::from(slave.try_clone()?))
                .stdout(Stdio::from(slave.try_clone()?))
                .stderr(Stdio::from(slave));
            // Environment and allocation happen before spawning. Only syscalls here.
            unsafe {
                cmd.pre_exec(|| {
                    if libc::setsid() < 0 || set_controlling_tty() < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let child = cmd.spawn().context("spawn PTY command")?;
            Ok(Self {
                master,
                child,
                reader: Some(reader),
                writer: Some(writer),
            })
        }
        pub fn take_reader(&mut self) -> Box<dyn Read + Send> {
            Box::new(self.reader.take().expect("reader already taken"))
        }
        pub fn take_writer(&mut self) -> Box<dyn Write + Send> {
            Box::new(self.writer.take().expect("writer already taken"))
        }
        pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
            let size = libc::winsize {
                ws_row: rows.max(1),
                ws_col: cols.max(1),
                ws_xpixel: 0,
                ws_ypixel: 0,
            };
            if unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &size) } != 0 {
                return Err(std::io::Error::last_os_error()).context("resize PTY");
            }
            Ok(())
        }
        pub fn has_exited(&mut self) -> bool {
            self.child.try_wait().map(|s| s.is_some()).unwrap_or(true)
        }
        pub fn kill(&mut self) {
            // Do not signal a process group after its leader was reaped (PID reuse).
            if matches!(self.child.try_wait(), Ok(None)) {
                unsafe {
                    libc::kill(-(self.child.id() as i32), libc::SIGKILL);
                }
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
    }
    impl Drop for PtySession {
        fn drop(&mut self) {
            self.kill();
        }
    }
}

pub use platform::PtySession;
