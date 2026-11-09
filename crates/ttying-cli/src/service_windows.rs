use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;
use ttying_core::{default_shell, random_token, start_background, ServerConfig};
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::{define_windows_service, service_dispatcher};

const DEFAULT_SERVICE_NAME: &str = "ttying";

#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub bind: String,
    pub port: u16,
    pub token_file: PathBuf,
    pub command: Vec<String>,
    pub read_only: bool,
    #[serde(default)]
    pub secure_cookie: bool,
    pub max_clients: usize,
    pub allowed_origins: Vec<String>,
}

pub struct InstallOptions {
    pub bind: String,
    pub port: u16,
    pub token: Option<String>,
    pub token_file: Option<PathBuf>,
    pub command: Vec<String>,
    pub read_only: bool,
    pub secure_cookie: bool,
    pub max_clients: usize,
    pub allowed_origins: Vec<String>,

    pub service_account: String,
    pub service_password: Option<String>,
}

pub fn program_data_dir() -> PathBuf {
    let base = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    base.join("ttying")
}

pub fn install(opts: InstallOptions) -> Result<()> {
    let dir = program_data_dir();
    std::fs::create_dir_all(&dir)?;
    let token_file =
        std::path::absolute(opts.token_file.clone().unwrap_or_else(|| dir.join("token")))?;
    if !token_file.exists() || opts.token.is_some() {
        let token = opts.token.clone().unwrap_or_else(random_token);
        super::secret::validate(&token)?;
        super::secret::write(&token_file, &token, Some(&opts.service_account))?;
    }
    let existing_token = std::fs::read_to_string(&token_file)?.trim().to_string();
    super::secret::validate(&existing_token)?;
    super::secret::write(&token_file, &existing_token, Some(&opts.service_account))?;
    let command = if opts.command.is_empty() {
        default_shell()
    } else {
        opts.command.clone()
    };
    let config = ServiceConfig {
        bind: opts.bind,
        port: opts.port,
        token_file: token_file.clone(),
        command,
        read_only: opts.read_only,
        secure_cookie: opts.secure_cookie,
        max_clients: opts.max_clients,
        allowed_origins: opts.allowed_origins,
    };
    let config_path = dir.join("config.json");
    super::secret::write(
        &config_path,
        &serde_json::to_string_pretty(&config)?,
        Some(&opts.service_account),
    )?;

    let exe = std::env::current_exe().context("current_exe")?;
    let bin_path = format!(
        "\"{}\" --service --config \"{}\"",
        exe.display(),
        config_path.display()
    );
    let display = "ttying Web Terminal";
    let mut cmd = Command::new("sc.exe");
    cmd.args([
        "create",
        DEFAULT_SERVICE_NAME,
        "binPath=",
        &bin_path,
        "start=",
        "auto",
        "DisplayName=",
        display,
        "obj=",
        &opts.service_account,
    ]);
    if let Some(password) = &opts.service_password {
        cmd.args(["password=", password]);
    }
    let status = cmd.status().context("sc create")?;
    if !status.success() {
        anyhow::bail!("sc create failed with {status}");
    }
    let _ = Command::new("sc.exe")
        .args([
            "description",
            DEFAULT_SERVICE_NAME,
            "ttying web terminal service",
        ])
        .status();
    println!("service installed: {}", DEFAULT_SERVICE_NAME);
    println!("token file: {}", token_file.display());
    println!("config: {}", config_path.display());
    println!("start with: sc.exe start {}", DEFAULT_SERVICE_NAME);
    Ok(())
}

pub fn uninstall(service_name: &str) -> Result<()> {
    let _ = Command::new("sc.exe").args(["stop", service_name]).status();
    let status = Command::new("sc.exe")
        .args(["delete", service_name])
        .status()
        .context("sc delete")?;
    if !status.success() {
        anyhow::bail!("sc delete failed with {status}");
    }
    println!("service removed: {service_name}");
    Ok(())
}

pub fn run_dispatcher() -> Result<()> {
    service_dispatcher::start(DEFAULT_SERVICE_NAME, ffi_service_main)
        .context("service dispatcher")?;
    Ok(())
}

define_windows_service!(ffi_service_main, service_main);

fn service_main(_arguments: Vec<OsString>) {
    if let Err(e) = run_service_inner() {
        eprintln!("service error: {e}");
    }
}

fn load_config() -> Result<ServiceConfig> {
    let args: Vec<OsString> = std::env::args_os().collect();
    let mut path: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--config" && i + 1 < args.len() {
            path = Some(PathBuf::from(&args[i + 1]));
            i += 2;
            continue;
        }
        i += 1;
    }
    let path = path.context("service started without --config")?;
    let text = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&text)?)
}

fn run_service_inner() -> Result<()> {
    let config = load_config()?;
    let token = std::fs::read_to_string(&config.token_file)?
        .trim()
        .to_string();
    super::secret::validate(&token)?;
    let (stop_tx, stop_rx) = mpsc::channel();
    let handler =
        service_control_handler::register(DEFAULT_SERVICE_NAME, move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = stop_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;
    let running = ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    };
    handler.set_service_status(running)?;
    let handle = start_background(ServerConfig {
        bind: config.bind,
        port: config.port,
        token,
        command: config.command,
        read_only: config.read_only,
        max_clients: config.max_clients,
        allowed_origins: config.allowed_origins,
        session_ttl_secs: 86400,
        secure_cookie: config.secure_cookie,
    })?;
    let _ = stop_rx.recv();
    handle.stop();
    let stopped = ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    };
    handler.set_service_status(stopped)?;
    Ok(())
}
