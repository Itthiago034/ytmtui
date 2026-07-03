//! Configuração persistente do ytmtui.
//!
//! Guarda preferências simples (volume, modos de shuffle/repeat e caminho de
//! cookies) em um arquivo JSON no diretório de configuração do usuário
//! (ex.: `~/.config/ytmtui/config.json` no Linux).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Preferências persistidas entre execuções.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Volume de 0.0 a 1.0.
    pub volume: f32,
    /// Reprodução aleatória ativada.
    pub shuffle: bool,
    /// Modo de repetição: "off", "all" ou "one".
    pub repeat: String,
    /// Caminho opcional para arquivo de cookies do yt-dlp.
    pub cookies: Option<String>,
    /// Nome do tema de cores (ver `crate::theme::THEMES`).
    pub theme: String,
    /// Nome de exibição personalizado (sobrepõe o nome vindo da conta).
    pub username: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            volume: 0.8,
            shuffle: false,
            repeat: "off".to_string(),
            cookies: None,
            theme: "Roxo".to_string(),
            username: None,
        }
    }
}

/// Caminho do arquivo de configuração, se o diretório for determinável.
fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ytmtui").join("config.json"))
}

impl Config {
    /// Carrega a configuração do disco; retorna o padrão em caso de erro.
    pub fn load() -> Self {
        let Some(path) = config_path() else { return Self::default() };
        let Ok(contents) = std::fs::read_to_string(&path) else { return Self::default() };
        serde_json::from_str(&contents).unwrap_or_default()
    }

    /// Salva a configuração no disco (falhas são ignoradas silenciosamente).
    pub fn save(&self) {
        let Some(path) = config_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }
}
