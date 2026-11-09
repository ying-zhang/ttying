use jni::errors::{Result as JResult, ThrowRuntimeExAndDefault};
use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::{Env, EnvUnowned};
use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use ttying_core::{
    default_shell, random_token, start_background, ServerConfig, ServerHandle, DEFAULT_PORT,
};

struct RuntimeState {
    handle: Option<ServerHandle>,
    token: String,
    url: String,
    error: String,
}

static STATE: OnceLock<Mutex<RuntimeState>> = OnceLock::new();

fn state() -> &'static Mutex<RuntimeState> {
    STATE.get_or_init(|| {
        Mutex::new(RuntimeState {
            handle: None,
            token: String::new(),
            url: String::new(),
            error: String::new(),
        })
    })
}

#[derive(Serialize)]
struct Status {
    running: bool,
    port: u16,
    url: String,
    error: String,
    version: &'static str,
}

fn status_json() -> String {
    status_locked(&state().lock().unwrap_or_else(|p| p.into_inner()))
}

fn status_locked(guard: &RuntimeState) -> String {
    let running = guard.handle.is_some();
    let port = guard.handle.as_ref().map(|h| h.addr.port()).unwrap_or(0);
    serde_json::to_string(&Status {
        running,
        port,
        url: guard.url.clone(),
        error: guard.error.clone(),
        version: ttying_core::VERSION,
    })
    .unwrap_or_else(|_| "{\"running\":false}".to_string())
}

fn jstring_to_string(env: &mut Env<'_>, value: &JString<'_>) -> Option<String> {
    if value.is_null() {
        return None;
    }
    value.try_to_string(env).ok()
}

fn status_to_java(env: &mut Env<'_>, json: String) -> JResult<jstring> {
    Ok(env.new_string(json)?.into_raw())
}

#[no_mangle]
pub extern "system" fn Java_cn_z_ttying_TtyingNative_start<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    bind: JString<'local>,
    port: jni::sys::jint,
    command_json: JString<'local>,
    token: JString<'local>,
) -> jstring {
    unowned_env
        .with_env(|env| -> JResult<jstring> {
            let bind = jstring_to_string(env, &bind).unwrap_or_else(|| "127.0.0.1".to_string());
            let command_json = jstring_to_string(env, &command_json).unwrap_or_default();
            let token = jstring_to_string(env, &token).unwrap_or_default();
            let command: Vec<String> = if command_json.trim().is_empty() {
                default_shell()
            } else {
                serde_json::from_str(&command_json).unwrap_or_else(|_| default_shell())
            };
            let token = if token.trim().is_empty() {
                random_token()
            } else {
                token
            };
            let requested_port = port.clamp(0, u16::MAX as i32) as u16;
            let preferred_port = if requested_port == 0 {
                DEFAULT_PORT
            } else {
                requested_port
            };

            let mut guard = state().lock().unwrap_or_else(|p| p.into_inner());
            if guard.handle.is_some() {
                return status_to_java(env, status_locked(&guard));
            }
            guard.error.clear();
            guard.token.clear();
            guard.url.clear();
            let mut config = ServerConfig {
                bind,
                port: preferred_port,
                token: token.clone(),
                command,
                read_only: false,
                max_clients: 4,
                allowed_origins: vec![],
                session_ttl_secs: 86400,
                secure_cookie: false,
            };
            let mut started = start_background(config.clone());
            if requested_port == 0 && started.is_err() {
                config.port = 0;
                started = start_background(config);
            }
            match started {
                Ok(handle) => {
                    let encoded_token =
                        url::form_urlencoded::byte_serialize(token.as_bytes()).collect::<String>();
                    let url = format!(
                        "http://127.0.0.1:{}/#auth={}",
                        handle.addr.port(),
                        encoded_token
                    );
                    guard.handle = Some(handle);
                    guard.token = token;
                    guard.url = url;
                }
                Err(e) => {
                    guard.error = e.to_string();
                    tracing::error!("android server start failed: {e}");
                }
            }
            let json = status_locked(&guard);
            drop(guard);
            status_to_java(env, json)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[no_mangle]
pub extern "system" fn Java_cn_z_ttying_TtyingNative_stop<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
) -> jstring {
    unowned_env
        .with_env(|env| -> JResult<jstring> {
            let json = {
                let mut guard = state().lock().unwrap_or_else(|p| p.into_inner());
                if let Some(handle) = guard.handle.take() {
                    handle.stop();
                }
                guard.error.clear();
                guard.token.clear();
                guard.url.clear();
                status_locked(&guard)
            };
            status_to_java(env, json)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[no_mangle]
pub extern "system" fn Java_cn_z_ttying_TtyingNative_status<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
) -> jstring {
    unowned_env
        .with_env(|env| -> JResult<jstring> { status_to_java(env, status_json()) })
        .resolve::<ThrowRuntimeExAndDefault>()
}
