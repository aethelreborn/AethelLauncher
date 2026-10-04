
pub mod types;
pub use types::*;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn default_jar_path() -> Option<PathBuf> {
    std::env::var("MCLAUNCHER_JAR")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| {
            let manifest_dir = env!("CARGO_MANIFEST_DIR");
            Some(Path::new(manifest_dir)
                .join("resources")
                .join("java")
                .join("mclauncher-api-0.3.2.jar"))
        })
        .filter(|p| p.exists())
}

#[derive(Debug, Clone)]
pub struct JavaBridge {
    pub jar_path: PathBuf,
    pub minecraft_dir: PathBuf,
    pub java_bin: Option<PathBuf>,
}

impl JavaBridge {
    pub fn new_default() -> Option<Self> {
        let jar = default_jar_path()?;
        Some(Self {
            jar_path: jar,
            minecraft_dir: PathBuf::from("."),
            java_bin: None,
        })
    }

    pub fn new(jar_path: impl Into<PathBuf>, minecraft_dir: impl Into<PathBuf>) -> Self {
        Self {
            jar_path: jar_path.into(),
            minecraft_dir: minecraft_dir.into(),
            java_bin: None,
        }
    }

    pub fn with_java_bin(mut self, path: impl Into<PathBuf>) -> Self {
        self.java_bin = Some(path.into());
        self
    }

    fn run_cmd(&self, cmd_args: &[&str]) -> Result<String> {
        let java = self
            .java_bin
            .as_ref()
            .map(|p| p.as_os_str())
            .unwrap_or_else(|| std::ffi::OsStr::new("java"));

        let proc = Command::new(java)
            .arg("-cp")
            .arg(&self.jar_path)
            .args(cmd_args)
            .current_dir(&self.minecraft_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| BridgeError::JvmSpawn(e, java.to_string_lossy().to_string()))?;

        let result = proc
            .wait_with_output()
            .map_err(|e| BridgeError::ReadOutput(e))?;

        let status = result.status;
        let stdout = String::from_utf8(result.stdout)
            .map_err(BridgeError::Utf8)?;
        let stderr = String::from_utf8_lossy(&result.stderr);

        if !status.success() {
            let code = status.code().unwrap_or(-1);
            return Err(BridgeError::JvmExit(code, stderr.trim().to_string()));
        }
        Ok(stdout)
    }

    pub fn list_versions(&self) -> Result<Vec<String>> {
        let output = self.run_cmd(&["bridge.Main", "versions"])?;
        Ok(output
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.trim().to_string())
            .collect())
    }

    pub fn latest_version(&self) -> Result<String> {
        let output = self.run_cmd(&["bridge.Main", "latest"])?;
        output
            .lines()
            .next()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .ok_or(BridgeError::EmptyResponse)
    }

    pub fn login(
        &self,
        username: &str,
        password: &str,
        client_token: Option<&str>,
    ) -> Result<MCLauncherSession> {
        let cmd = if let Some(ct) = client_token {
            vec!["bridge.Main", "login", username, password, ct]
        } else {
            vec!["bridge.Main", "login", username, password]
        };
        let output = self.run_cmd(&cmd)?;
        parse_session_json(&output)
    }

    pub fn build_launch_command(
        &self,
        version_id: &str,
        access_token: &str,
        uuid: &str,
        username: &str,
    ) -> Result<Vec<String>> {
        let output = self.run_cmd(&[
            "bridge.Main",
            "launchcmd",
            version_id,
            access_token,
            uuid,
            username,
        ])?;
        parse_string_array_json(&output)
    }
}

fn parse_session_json(s: &str) -> Result<MCLauncherSession> {
    let v: serde_json::Value =
        serde_json::from_str(s).map_err(|e| BridgeError::JsonParse(e.to_string(), s.to_string()))?;
    let obj = v
        .as_object()
        .ok_or_else(|| BridgeError::JsonParse("expected object".to_string(), s.to_string()))?;
    Ok(MCLauncherSession {
        access_token: obj
            .get("accessToken")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        username: obj
            .get("username")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        uuid: obj
            .get("uuid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        user_type: obj
            .get("userType")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        client_token: obj
            .get("clientToken")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

fn parse_string_array_json(s: &str) -> Result<Vec<String>> {
    let v: Vec<String> =
        serde_json::from_str(s).map_err(|e| BridgeError::JsonParse(e.to_string(), s.to_string()))?;
    Ok(v)
}


#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("cannot find `java` at `{1}`: {0}")]
    JvmSpawn(#[source] std::io::Error, String),
    #[error("failed to read JVM output: {0}")]
    ReadOutput(#[source] std::io::Error),
    #[error("JVM exited with code {0}: {1}")]
    JvmExit(i32, String),
    #[error("empty response from JVM")]
    EmptyResponse,
    #[error("JSON parse error: {0} (input: {1})")]
    JsonParse(String, String),
    #[error("invalid UTF-8 in JVM output: {0}")]
    Utf8(#[source] std::string::FromUtf8Error),
}

pub type Result<T> = std::result::Result<T, BridgeError>;


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_session_json() {
        let input = r#"{"accessToken":"abc","username":"Steve","uuid":"123","userType":"msa","clientToken":"tok"}"#;
        let s = parse_session_json(input).unwrap();
        assert_eq!(s.access_token, "abc");
        assert_eq!(s.username, "Steve");
    }

    #[test]
    fn test_parse_string_array() {
        let input = r#"["java","-Xmx4G","net.minecraft.client.main.Main","--foo"]"#;
        let arr = parse_string_array_json(input).unwrap();
        assert_eq!(arr.len(), 4);
        assert_eq!(arr[0], "java");
    }
}
