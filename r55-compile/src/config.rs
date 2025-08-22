use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use eyre::Result;

/// Configuration for R55 compilation
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct R55Config {
    /// Source directories for contracts
    #[serde(default = "default_src_dirs")]
    pub src: Vec<String>,
    
    /// Output directory for compiled bytecode
    #[serde(default = "default_out_dir")]
    pub out: String,
    
    /// Library/dependency directories
    #[serde(default = "default_lib_dirs")]
    pub libs: Vec<String>,
    
    /// Test contract directories
    #[serde(default = "default_test_dirs")]
    pub test: Vec<String>,
    
    /// Script directories
    #[serde(default = "default_script_dirs")]
    pub script: Vec<String>,
    
    /// Path remappings for imports
    #[serde(default)]
    pub remappings: Vec<String>,
    
    /// Exclude patterns (glob patterns)
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Default for R55Config {
    fn default() -> Self {
        Self {
            src: default_src_dirs(),
            out: default_out_dir(),
            libs: default_lib_dirs(),
            test: default_test_dirs(),
            script: default_script_dirs(),
            remappings: vec![],
            exclude: vec![],
        }
    }
}

impl R55Config {
    /// Load configuration from r55.toml file
    pub fn load() -> Result<Self> {
        let config_path = Self::find_config_file()?;
        
        if let Some(path) = config_path {
            let content = fs::read_to_string(&path)?;
            let config: R55Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            // No config file found, use defaults
            Ok(Self::default())
        }
    }
    
    /// Load configuration from a specific path
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())?;
        let config: R55Config = toml::from_str(&content)?;
        Ok(config)
    }
    
    /// Find r55.toml in current directory or parent directories
    fn find_config_file() -> Result<Option<PathBuf>> {
        let current_dir = std::env::current_dir()?;
        let mut dir = current_dir.as_path();
        
        loop {
            let config_path = dir.join("r55.toml");
            if config_path.exists() {
                return Ok(Some(config_path));
            }
            
            // Also check for Foundry-style config for compatibility
            let foundry_config = dir.join("foundry.toml");
            if foundry_config.exists() {
                // We could potentially parse foundry.toml and adapt it
                // For now, we'll just ignore it
            }
            
            // Check parent directory
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
        
        Ok(None)
    }
    
    /// Get all source directories as absolute paths
    pub fn get_src_paths(&self, project_root: &Path) -> Vec<PathBuf> {
        self.src.iter()
            .map(|dir| {
                if Path::new(dir).is_absolute() {
                    PathBuf::from(dir)
                } else {
                    project_root.join(dir)
                }
            })
            .filter(|path| path.exists())
            .collect()
    }
    
    /// Get output directory as absolute path
    pub fn get_out_path(&self, project_root: &Path) -> PathBuf {
        if Path::new(&self.out).is_absolute() {
            PathBuf::from(&self.out)
        } else {
            project_root.join(&self.out)
        }
    }
    
    /// Get library directories as absolute paths
    pub fn get_lib_paths(&self, project_root: &Path) -> Vec<PathBuf> {
        self.libs.iter()
            .map(|dir| {
                if Path::new(dir).is_absolute() {
                    PathBuf::from(dir)
                } else {
                    project_root.join(dir)
                }
            })
            .filter(|path| path.exists())
            .collect()
    }
    
    /// Parse remappings into a HashMap
    pub fn get_remappings(&self) -> HashMap<String, String> {
        let mut mappings = HashMap::new();
        
        for remapping in &self.remappings {
            if let Some((from, to)) = remapping.split_once('=') {
                mappings.insert(from.to_string(), to.to_string());
            }
        }
        
        mappings
    }
    
    /// Check if a path should be excluded based on exclude patterns
    pub fn should_exclude(&self, path: &Path) -> bool {
        for pattern in &self.exclude {
            if let Ok(glob) = glob::Pattern::new(pattern) {
                if glob.matches_path(path) {
                    return true;
                }
            }
        }
        false
    }
}

// Default functions for serde
fn default_src_dirs() -> Vec<String> {
    vec!["src".to_string(), "contracts".to_string()]
}

fn default_out_dir() -> String {
    "out".to_string()
}

fn default_lib_dirs() -> Vec<String> {
    vec!["lib".to_string()]
}

fn default_test_dirs() -> Vec<String> {
    vec!["test".to_string()]
}

fn default_script_dirs() -> Vec<String> {
    vec!["script".to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = R55Config::default();
        assert_eq!(config.src, vec!["src", "contracts"]);
        assert_eq!(config.out, "out");
        assert_eq!(config.libs, vec!["lib"]);
    }
    
    #[test]
    fn test_parse_remappings() {
        let mut config = R55Config::default();
        config.remappings = vec![
            "@openzeppelin/=lib/openzeppelin-contracts/".to_string(),
            "@chainlink/=lib/chainlink/".to_string(),
        ];
        
        let mappings = config.get_remappings();
        assert_eq!(mappings.get("@openzeppelin/"), Some(&"lib/openzeppelin-contracts/".to_string()));
        assert_eq!(mappings.get("@chainlink/"), Some(&"lib/chainlink/".to_string()));
    }
}
