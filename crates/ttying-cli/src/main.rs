mod secret;
use anyhow::{Context, Result};
use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches, Parser};
use configparser::ini::Ini;
use std::path::{Path, PathBuf};
use ttying_core::{default_shell, random_token, start_background, ServerConfig, DEFAULT_PORT};

#[cfg(windows)]
mod service_windows;

#[derive(Parser, Debug)]
#[command(name = "ttying", version, about = "Secure web terminal sharing")]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1")]
    bind: String,
    #[arg(short, long, default_value_t = DEFAULT_PORT)]
    port: u16,
    #[arg(long, help = "Fixed token. Prefer --token-file.")]
    token: Option<String>,
    #[arg(long, help = "Read or create a token file")]
    token_file: Option<PathBuf>,
    #[arg(long, help = "Print access URL and exit")]
    print_url: bool,
    #[arg(
        long,
        help = "Public base URL when behind a reverse proxy (e.g. https://tty.example.cn)"
    )]
    public_url: Option<String>,
    #[arg(short = 'W', long, help = "Allow keyboard input (default: read-only)")]
    writable: bool,
    #[arg(long, help = "Use Secure cookies behind an HTTPS reverse proxy")]
    secure_cookie: bool,
    #[arg(long, default_value_t = 10)]
    max_clients: usize,
    #[arg(long = "allowed-origin")]
    allowed_origins: Vec<String>,
    #[arg(long, hide = true)]
    service: bool,
    #[arg(
        long,
        value_name = "FILE",
        help = "Load defaults from an INI configuration file"
    )]
    config: Option<PathBuf>,
    #[arg(long)]
    install_service: bool,
    #[arg(long)]
    uninstall_service: bool,
    #[arg(long, default_value = "NT AUTHORITY\\LocalService")]
    service_account: String,
    #[arg(long)]
    service_password: Option<String>,
    #[arg(last = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

fn load_or_create_token(file: &PathBuf, _protect_existing: bool) -> Result<String> {
    if file.exists() {
        anyhow::ensure!(
            !std::fs::symlink_metadata(file)?.file_type().is_symlink(),
            "token file must not be a symlink"
        );
        let token = std::fs::read_to_string(file)
            .with_context(|| format!("read token file {}", file.display()))?;
        let token = token.trim().to_string();
        secret::validate(&token)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            anyhow::ensure!(
                std::fs::metadata(file)?.permissions().mode() & 0o077 == 0,
                "token file permissions must be 0600 (or stricter)"
            );
        }
        #[cfg(windows)]
        if _protect_existing {
            secret::write(file, &token, None)?;
        }
        return Ok(token);
    }
    let token = random_token();
    secret::create(file, &token)?;
    Ok(token)
}

fn choose_token(args: &Args) -> Result<String> {
    if let Some(token) = &args.token {
        secret::validate(token)?;
        return Ok(token.clone());
    }
    if let Some(file) = &args.token_file {
        // Printing a service URL must not replace its ACL and revoke the service
        // account's read permission. Only foreground startup hardens the file.
        return load_or_create_token(file, !args.print_url);
    }
    Ok(random_token())
}

fn parse_ini_bool(value: &str, key: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => anyhow::bail!("{key} must be true or false"),
    }
}

fn split_command(value: &str) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for ch in value.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => current.push(ch),
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                started = true;
            }
            None if ch.is_whitespace() => {
                if started {
                    result.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => {
                current.push(ch);
                started = true;
            }
        }
    }
    anyhow::ensure!(quote.is_none(), "unclosed quote in command");
    if started {
        result.push(current);
    }
    anyhow::ensure!(!result.is_empty(), "command must not be empty");
    Ok(result)
}

fn apply_config(args: &mut Args, matches: &clap::ArgMatches) -> Result<()> {
    let Some(path) = args.config.clone() else {
        return Ok(());
    };
    let mut ini = Ini::new();
    let map = ini
        .load(&path)
        .map_err(|error| anyhow::anyhow!("load config {}: {error}", path.display()))?;
    for (section, entries) in &map {
        let allowed: &[&str] = match section.as_str() {
            "server" => &["bind", "port", "public_url"],
            "security" => &[
                "token",
                "token_file",
                "secure_cookie",
                "max_clients",
                "allowed_origins",
            ],
            "terminal" => &["writable", "command"],
            other => anyhow::bail!("unknown config section [{other}]"),
        };
        for key in entries.keys() {
            anyhow::ensure!(
                allowed.contains(&key.as_str()),
                "unknown key [{section}] {key}"
            );
        }
    }
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let get = |section: &str, key: &str| {
        ini.get(section, key)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    if matches.value_source("bind") != Some(ValueSource::CommandLine) {
        if let Some(value) = get("server", "bind") {
            args.bind = value;
        }
    }
    if matches.value_source("port") != Some(ValueSource::CommandLine) {
        if let Some(value) = get("server", "port") {
            args.port = value.parse().context("[server] port must be a number")?;
        }
    }
    if matches.value_source("public_url") != Some(ValueSource::CommandLine) {
        if let Some(value) = get("server", "public_url") {
            args.public_url = Some(value);
        }
    }
    if matches.value_source("token") != Some(ValueSource::CommandLine) {
        if let Some(value) = get("security", "token") {
            args.token = Some(value);
        }
    }
    if matches.value_source("token_file") != Some(ValueSource::CommandLine) {
        if let Some(value) = get("security", "token_file") {
            let file = PathBuf::from(value);
            args.token_file = Some(if file.is_relative() {
                base.join(file)
            } else {
                file
            });
        }
    }
    if matches.value_source("max_clients") != Some(ValueSource::CommandLine) {
        if let Some(value) = get("security", "max_clients") {
            args.max_clients = value
                .parse()
                .context("[security] max_clients must be a number")?;
        }
    }
    if args.allowed_origins.is_empty() {
        if let Some(value) = get("security", "allowed_origins") {
            args.allowed_origins = value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToString::to_string)
                .collect();
        }
    }
    if let Some(value) = get("security", "secure_cookie") {
        args.secure_cookie |= parse_ini_bool(&value, "[security] secure_cookie")?;
    }
    if let Some(value) = get("terminal", "writable") {
        args.writable |= parse_ini_bool(&value, "[terminal] writable")?;
    }
    if args.command.is_empty() {
        if let Some(value) = get("terminal", "command") {
            args.command = split_command(&value)?;
        }
    }
    tracing::info!("loaded config: {}", path.display());
    Ok(())
}

fn normalize_public_url(value: &str) -> Result<String> {
    let parsed = url::Url::parse(value).context("invalid --public-url")?;
    anyhow::ensure!(
        matches!(parsed.scheme(), "http" | "https"),
        "--public-url must use http or https"
    );
    anyhow::ensure!(
        parsed.host_str().is_some(),
        "--public-url must contain a host"
    );
    anyhow::ensure!(
        parsed.username().is_empty() && parsed.password().is_none(),
        "--public-url must not contain credentials"
    );
    anyhow::ensure!(
        parsed.query().is_none() && parsed.fragment().is_none(),
        "--public-url must not contain a query or fragment"
    );
    anyhow::ensure!(
        parsed.path().is_empty() || parsed.path() == "/",
        "--public-url must point at the application root; path prefixes are not supported"
    );
    Ok(parsed.as_str().trim_end_matches('/').to_string())
}

fn token_fragment(token: &str) -> String {
    url::form_urlencoded::byte_serialize(token.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

fn access_url(bind: &str, port: u16, token: &str, public_url: Option<&str>) -> Result<String> {
    let base = if let Some(public_url) = public_url {
        normalize_public_url(public_url)?
    } else {
        let host = match bind {
            "0.0.0.0" | "::" | "[::]" => "127.0.0.1",
            other => other,
        };
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.to_string()
        };
        format!("http://{host}:{port}")
    };
    Ok(format!("{base}/#auth={}", token_fragment(token)))
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let matches = Args::command().get_matches();
    let mut args = Args::from_arg_matches(&matches)?;
    apply_config(&mut args, &matches)?;

    #[cfg(windows)]
    {
        if args.service {
            return service_windows::run_dispatcher();
        }
        if args.install_service {
            return service_windows::install(service_windows::InstallOptions {
                bind: args.bind.clone(),
                port: args.port,
                token: args.token.clone(),
                token_file: args.token_file.clone(),
                command: args.command.clone(),
                read_only: !args.writable,
                secure_cookie: args.secure_cookie
                    || args
                        .public_url
                        .as_deref()
                        .is_some_and(|u| u.starts_with("https://")),
                max_clients: args.max_clients,
                allowed_origins: args.allowed_origins.clone(),
                service_account: args.service_account.clone(),
                service_password: args.service_password.clone(),
            });
        }
        if args.uninstall_service {
            return service_windows::uninstall("ttying");
        }
    }
    #[cfg(not(windows))]
    if args.service || args.install_service || args.uninstall_service {
        anyhow::bail!("Windows service options are only available on Windows");
    }

    let token = choose_token(&args)?;
    let command = if args.command.is_empty() {
        default_shell()
    } else {
        args.command.clone()
    };

    if args.secure_cookie {
        tracing::info!("Secure cookie mode: open the public HTTPS proxy URL and use the access token from the fragment below");
    }
    if args.print_url {
        println!(
            "{}",
            access_url(&args.bind, args.port, &token, args.public_url.as_deref())?
        );
        return Ok(());
    }

    if args.bind != "127.0.0.1" && args.bind != "::1" && args.bind != "[::1]" {
        tracing::warn!(
            "binding to {} exposes the service beyond this machine",
            args.bind
        );
        tracing::warn!("token authentication does not provide transport encryption");
    }

    let public_is_https = args
        .public_url
        .as_deref()
        .is_some_and(|u| u.starts_with("https://"));
    if public_is_https && !args.secure_cookie {
        tracing::warn!("--public-url is HTTPS but --secure-cookie is not set; enabling Secure cookies for browser compatibility");
    }
    let secure_cookie = args.secure_cookie || public_is_https;
    let config = ServerConfig {
        bind: args.bind.clone(),
        port: args.port,
        token: token.clone(),
        command: command.clone(),
        read_only: !args.writable,
        max_clients: args.max_clients,
        allowed_origins: args.allowed_origins,
        session_ttl_secs: 86400,
        secure_cookie,
    };
    let handle = start_background(config)?;
    let url = access_url(
        &handle.addr.ip().to_string(),
        handle.addr.port(),
        &token,
        args.public_url.as_deref(),
    )?;
    tracing::info!("command: {:?}", command);
    tracing::info!("read_only: {}", !args.writable);
    println!("ttying listening on http://{}", handle.addr);
    println!("open: {url}");
    println!("press Ctrl+C to stop");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    });
    handle.stop();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_url_is_appended_with_fragment_token() {
        let url = access_url(
            "127.0.0.1",
            12345,
            "abc+def",
            Some("https://tty.example.cn/"),
        )
        .unwrap();
        assert_eq!(url, "https://tty.example.cn/#auth=abc%2Bdef");
    }

    #[test]
    fn public_url_rejects_paths_and_fragments() {
        assert!(normalize_public_url("https://tty.example.cn/prefix").is_err());
        assert!(normalize_public_url("https://tty.example.cn/#auth=x").is_err());
        assert!(normalize_public_url("ftp://tty.example.cn").is_err());
    }

    #[test]
    fn command_splitting_respects_quotes() {
        assert_eq!(
            split_command(r#"cmd.exe /k "C:\Program Files\tool.exe" --flag"#).unwrap(),
            vec!["cmd.exe", "/k", r#"C:\Program Files\tool.exe"#, "--flag"]
        );
        assert!(split_command(r#""unclosed"#).is_err());
    }

    #[test]
    fn ini_boolean_parsing_is_strict() {
        assert!(parse_ini_bool("true", "test").unwrap());
        assert!(!parse_ini_bool("OFF", "test").unwrap());
        assert!(parse_ini_bool("maybe", "test").is_err());
    }
}
