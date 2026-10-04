use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceConfig {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub loader: LoaderType,
    pub loader_version: String,
    pub ram_mb: u64,
    pub java_path: Option<String>,
    pub renderer: RendererMode,
    pub auth_mode: AuthMode,
    pub jvm_args: Vec<String>,
    pub enabled_modules: Vec<String>,
    pub created_at: String,
    pub last_launched: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LoaderType {
    Fabric,
    Forge,
    Vanilla,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RendererMode {
    Auto,
    Vulkan,
    Opengl,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    Offline,
    Microsoft,
    Auto,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let toml_str = r#"
            id = "test-123"
            name = "My Instance"
            mcVersion = "1.21.11"
            loader = "fabric"
            loaderVersion = "0.16.0"
            ramMb = 4096
            renderer = "auto"
            authMode = "offline"
            jvmArgs = []
            enabledModules = []
            createdAt = "2026-01-01T00:00:00Z"
        "#;
        let config: InstanceConfig = toml::from_str(toml_str).expect("parse toml");
        assert_eq!(config.name, "My Instance");
        assert_eq!(config.mc_version, "1.21.11");
        assert!(matches!(config.loader, LoaderType::Fabric));
    }
}
