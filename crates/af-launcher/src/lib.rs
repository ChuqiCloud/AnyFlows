//! Small, versioned, local control protocol shared by the launcher and its child.
//! No public listener, arbitrary commands, source URL, or elevated permissions.

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    env,
    fs::{File, OpenOptions},
    net::SocketAddr,
    path::PathBuf,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};

pub const PROTOCOL: u32 = 1;
pub const MAX_MESSAGE_BYTES: u64 = 16 * 1024;
pub const CONTROL_ENV: [&str; 4] = [
    "ANYFLOWS_LAUNCHER_ENDPOINT",
    "ANYFLOWS_LAUNCHER_TOKEN",
    "ANYFLOWS_LAUNCHER_GENERATION",
    "ANYFLOWS_LAUNCHER_RUNTIME_LOCK",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Action {
    Status,
    Ready,
    Install {
        candidate: String,
        tag: String,
        sha256: String,
        size: u64,
    },
}

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub protocol: u32,
    pub token: String,
    pub generation: String,
    pub pid: u32,
    pub action: Action,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reply {
    pub protocol: u32,
    pub accepted: bool,
    pub launcher_version: String,
    pub phase: String,
    pub error: Option<String>,
}

impl Reply {
    pub fn new(accepted: bool, phase: &str, error: Option<&str>) -> Self {
        Self {
            protocol: PROTOCOL,
            accepted,
            launcher_version: env!("CARGO_PKG_VERSION").to_owned(),
            phase: phase.to_owned(),
            error: error.map(str::to_owned),
        }
    }
}

/// Secrets deliberately have no Debug/Serialize implementation.
#[derive(Clone)]
pub struct ManagedContext {
    pub endpoint: SocketAddr,
    token: String,
    pub generation: String,
    runtime_lock: PathBuf,
}

#[derive(Clone, Copy, Debug)]
pub struct ControlError;

impl ManagedContext {
    pub fn from_env() -> Result<Option<Self>, ControlError> {
        let values: Vec<_> = CONTROL_ENV.iter().map(env::var_os).collect();
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        let value = |index: usize| {
            values[index]
                .as_ref()
                .and_then(|v| v.to_str())
                .filter(|v| !v.is_empty())
                .ok_or(ControlError)
        };
        let endpoint: SocketAddr = value(0)?.parse().map_err(|_| ControlError)?;
        if !endpoint.ip().is_loopback() || endpoint.port() == 0 {
            return Err(ControlError);
        }
        let token = value(1)?;
        let generation = value(2)?;
        if token.len() != 64
            || !token.bytes().all(|c| c.is_ascii_hexdigit())
            || uuid::Uuid::parse_str(generation).is_err()
        {
            return Err(ControlError);
        }
        let runtime_lock = PathBuf::from(value(3)?);
        if !runtime_lock.is_absolute() {
            return Err(ControlError);
        }
        Ok(Some(Self {
            endpoint,
            token: token.to_owned(),
            generation: generation.to_owned(),
            runtime_lock,
        }))
    }

    pub async fn call(&self, action: Action) -> Result<Reply, ControlError> {
        let timeout = if matches!(&action, Action::Install { .. }) {
            30
        } else {
            5
        };
        let operation = async {
            let mut stream = TcpStream::connect(self.endpoint)
                .await
                .map_err(|_| ControlError)?;
            let request = Request {
                protocol: PROTOCOL,
                token: self.token.clone(),
                generation: self.generation.clone(),
                pid: std::process::id(),
                action,
            };
            let mut bytes = serde_json::to_vec(&request).map_err(|_| ControlError)?;
            if bytes.len() as u64 >= MAX_MESSAGE_BYTES {
                return Err(ControlError);
            }
            bytes.push(b'\n');
            stream.write_all(&bytes).await.map_err(|_| ControlError)?;
            let mut bytes = Vec::new();
            BufReader::new(stream)
                .take(MAX_MESSAGE_BYTES + 1)
                .read_until(b'\n', &mut bytes)
                .await
                .map_err(|_| ControlError)?;
            if bytes.len() as u64 > MAX_MESSAGE_BYTES {
                return Err(ControlError);
            }
            let reply: Reply = serde_json::from_slice(&bytes).map_err(|_| ControlError)?;
            if reply.protocol != PROTOCOL {
                return Err(ControlError);
            }
            Ok(reply)
        };
        tokio::time::timeout(Duration::from_secs(timeout), operation)
            .await
            .map_err(|_| ControlError)?
    }

    fn matches_shutdown(&self, line: &[u8]) -> bool {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
            return false;
        };
        value["protocol"] == PROTOCOL
            && value["token"] == self.token
            && value["command"] == "shutdown"
    }
}

/// Keep this guard in main until all HTTP, billing and database shutdown has finished.
pub struct ManagedGuard {
    _file: File,
}

pub async fn activate_child() -> Result<Option<ManagedGuard>, ControlError> {
    let Some(context) = ManagedContext::from_env()? else {
        return Ok(None);
    };
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&context.runtime_lock)
        .map_err(|_| ControlError)?;
    file.try_lock_exclusive().map_err(|_| ControlError)?;
    let reply = context.call(Action::Status).await?;
    if !reply.accepted {
        return Err(ControlError);
    }
    Ok(Some(ManagedGuard { _file: file }))
}

/// EOF is a parent-death lifeline. A managed child must drain even if its launcher crashes.
pub async fn child_shutdown_signal() {
    let Ok(Some(context)) = ManagedContext::from_env() else {
        return std::future::pending().await;
    };
    let mut input = BufReader::new(tokio::io::stdin());
    loop {
        let mut line = Vec::new();
        let result = (&mut input)
            .take(MAX_MESSAGE_BYTES + 1)
            .read_until(b'\n', &mut line)
            .await;
        match result {
            Ok(0) | Err(_) => return,
            Ok(_) if line.len() as u64 > MAX_MESSAGE_BYTES || context.matches_shutdown(&line) => {
                return;
            }
            Ok(_) => {}
        }
    }
}

pub async fn notify_ready() -> Result<(), ControlError> {
    let Some(context) = ManagedContext::from_env()? else {
        return Ok(());
    };
    let reply = context.call(Action::Ready).await?;
    if reply.accepted {
        Ok(())
    } else {
        Err(ControlError)
    }
}

pub fn in_container() -> bool {
    option_env!("AF_BUILD_DEPLOYMENT") == Some("docker")
        || std::path::Path::new("/.dockerenv").exists()
        || std::path::Path::new("/run/.containerenv").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_commands_require_the_private_token_and_protocol() {
        let context = ManagedContext {
            endpoint: "127.0.0.1:1".parse().unwrap(),
            token: "a".repeat(64),
            generation: uuid::Uuid::new_v4().to_string(),
            runtime_lock: PathBuf::from("unused"),
        };
        assert!(context.matches_shutdown(serde_json::to_vec(&serde_json::json!({"protocol":PROTOCOL,"token":"a".repeat(64),"command":"shutdown"})).unwrap().as_slice()));
        for value in [
            serde_json::json!({"protocol":2,"token":"a".repeat(64),"command":"shutdown"}),
            serde_json::json!({"protocol":PROTOCOL,"token":"b".repeat(64),"command":"shutdown"}),
            serde_json::json!({"protocol":PROTOCOL,"token":"a".repeat(64),"command":"install"}),
        ] {
            assert!(!context.matches_shutdown(&serde_json::to_vec(&value).unwrap()));
        }
    }
}
