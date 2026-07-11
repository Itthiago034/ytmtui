//! Configuração persistente do ytmtui.
//!
//! Guarda preferências simples (volume, modos de shuffle/repeat e caminho de
//! cookies) em um arquivo JSON no diretório de configuração do usuário
//! (ex.: `~/.config/ytmtui/config.json` no Linux).

use std::path::{Path, PathBuf};

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
    /// Intervalo (segundos) entre atualizações automáticas de Início e
    /// Biblioteca em segundo plano, enquanto o app está aberto.
    pub sync_interval_secs: u64,
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
            sync_interval_secs: 300,
        }
    }
}

/// Caminho do arquivo de configuração, se o diretório for determinável.
fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ytmtui").join("config.json"))
}

impl Config {
    /// Carrega a configuração do disco; retorna o padrão em caso de erro.
    ///
    /// Se o arquivo existir mas estiver corrompido (JSON inválido), uma cópia
    /// é preservada em `config.json.bak` antes de cair no padrão, e um aviso
    /// é retornado para que o chamador possa avisar o usuário em vez de
    /// simplesmente descartar o arquivo em silêncio.
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = config_path() else {
            return (Self::default(), None);
        };
        Self::load_from(&path)
    }

    /// Core of [`Self::load`], parameterized by path so it's testable
    /// without touching the user's real config directory.
    fn load_from(path: &Path) -> (Self, Option<String>) {
        let Ok(contents) = std::fs::read_to_string(path) else {
            return (Self::default(), None);
        };
        match serde_json::from_str(&contents) {
            Ok(config) => (config, None),
            Err(e) => {
                let backup = path.with_extension("json.bak");
                let _ = std::fs::copy(path, &backup);
                (
                    Self::default(),
                    Some(format!(
                        "Configuração corrompida ({e}); revertida ao padrão. Backup salvo em {}",
                        backup.display()
                    )),
                )
            }
        }
    }

    /// Salva a configuração no disco (falhas são ignoradas silenciosamente).
    pub fn save(&self) {
        let Some(path) = config_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = crate::fs_util::atomic_write(&path, json.as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_from_missing_file_is_the_default_with_no_warning() {
        let dir = tempfile::tempdir().unwrap();
        let (config, warning) = Config::load_from(&dir.path().join("config.json"));
        assert_eq!(config.theme, Config::default().theme);
        assert!(warning.is_none());
    }

    #[test]
    fn load_from_valid_file_parses_it_with_no_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"theme": "Verde"}"#).unwrap();

        let (config, warning) = Config::load_from(&path);
        assert_eq!(config.theme, "Verde");
        assert!(warning.is_none());
    }

    #[test]
    fn load_from_corrupt_file_backs_it_up_and_warns_instead_of_failing_silently() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{ not valid json").unwrap();

        let (config, warning) = Config::load_from(&path);

        // Falls back to defaults rather than propagating the parse error.
        assert_eq!(config.theme, Config::default().theme);
        assert!(warning.is_some(), "a corrupt file must produce a warning");

        // The original corrupt contents are preserved for inspection instead
        // of being silently discarded.
        let backup = path.with_extension("json.bak");
        assert_eq!(std::fs::read_to_string(backup).unwrap(), "{ not valid json");
    }
}
