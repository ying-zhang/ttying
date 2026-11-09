#[cfg(windows)]
use anyhow::Context;
use anyhow::Result;
use std::io::Write;
use std::path::Path;

pub fn protect(path: &Path, account: Option<&str>) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = account;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(windows)]
    {
        let system = std::path::PathBuf::from(
            std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()),
        )
        .join("System32");
        let current = std::process::Command::new(system.join("whoami.exe")).output()?;
        anyhow::ensure!(current.status.success(), "whoami failed");
        let current = String::from_utf8(current.stdout)?;
        let mut cmd = std::process::Command::new(system.join("icacls.exe"));
        cmd.arg(path)
            .args([
                "/inheritance:r",
                "/grant:r",
                "*S-1-5-18:(F)",
                "*S-1-5-32-544:(F)",
            ])
            .arg(format!("{}:(F)", current.trim()));
        if let Some(account) = account {
            cmd.arg(format!("{account}:(R)"));
        }
        let output = cmd.output().context("set secret ACL")?;
        anyhow::ensure!(
            output.status.success(),
            "icacls failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[cfg(windows)]
pub fn write(path: &Path, content: &str, account: Option<&str>) -> Result<()> {
    write_impl(path, content, account, true)
}

pub fn create(path: &Path, content: &str) -> Result<()> {
    write_impl(path, content, None, false)
}

fn write_impl(path: &Path, content: &str, account: Option<&str>, replace: bool) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".ttying-{}.tmp", ttying_core::random_token()));
    let result = (|| {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&temp)?;
        protect(&temp, account)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);
        if replace {
            std::fs::rename(&temp, path)?;
        } else {
            // Atomic no-clobber publication: concurrent startups cannot overwrite
            // the credential another running server has already selected.
            std::fs::hard_link(&temp, path)?;
            std::fs::remove_file(&temp)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

pub fn validate(token: &str) -> Result<()> {
    anyhow::ensure!(
        token.len() >= 16 && token.bytes().all(|b| b.is_ascii_graphic()),
        "token must contain at least 16 printable ASCII bytes without whitespace"
    );
    Ok(())
}
