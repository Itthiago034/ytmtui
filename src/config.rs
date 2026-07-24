//! Configuração persistente do ytmtui.
//!
//! Guarda preferências simples (volume, modos de shuffle/repeat e caminho de
//! cookies) em um arquivo JSON no diretório de configuração do usuário
//! (ex.: `~/.config/ytmtui/config.json` no Linux).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthenticationConfig {
    pub browser: Option<String>,
    pub profile: Option<String>,
    pub auth_user: u8,
}

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
    /// Preferências da conta usada na autenticação do YouTube Music.
    pub authentication: AuthenticationConfig,
    /// Nome do tema de cores (ver `crate::theme::THEMES`).
    pub theme: String,
    /// Nome de exibição personalizado (sobrepõe o nome vindo da conta).
    pub username: Option<String>,
    /// Intervalo (segundos) entre atualizações automáticas de Início e
    /// Biblioteca em segundo plano, enquanto o app está aberto.
    pub sync_interval_secs: u64,
    /// Modo de exibição da capa do álbum: "auto" (consulta o protocolo real
    /// do terminal), "halfblocks" (pula a consulta e usa blocos Unicode) ou
    /// "off" (nenhuma capa é baixada/desenhada). Ver [`ArtworkMode`].
    pub artwork_mode: String,
    /// Densidade dos cards da grade da tela Início: "comfortable" (título +
    /// subtítulo + rodapé) ou "compact" (título + rodapé, sem subtítulo).
    /// Ver [`HomeDensity`].
    pub home_density: String,
    /// Estilo do visualizador de espectro do player: "gradient" (cor muda
    /// com a altura da barra), "mono" (cor única de `theme.player`) ou
    /// "off" (nenhuma barra desenhada). Ver [`VisualizerStyle`].
    pub visualizer: String,
    /// Velocidade das animações (marquee, wipe do karaokê, revelação de
    /// seleção/metadados, janela de `App::kick_animation`): "normal", "fast"
    /// ou "slow". Ver [`AnimationSpeed`].
    pub animation_speed: String,
    /// Reduz/desativa animações não essenciais: marquee de títulos longos
    /// (volta a truncar com '…'), wipe por caractere do karaokê (linha ativa
    /// já inteira "cantada"), e a revelação em estágios do card
    /// selecionado/metadados do now-playing (pula direto ao estado final).
    pub reduced_motion: bool,
    /// Exibe a animação de entrada (wordmark montando) ao abrir o app.
    /// Ignorada quando `reduced_motion` está ligado.
    pub splash: bool,
    /// Correção global de sincronia das letras, em milissegundos. Positivo
    /// adianta as linhas (a letra estava atrasada). Ajustável com `<`/`>`.
    pub lyrics_offset_ms: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            volume: 0.8,
            shuffle: false,
            repeat: "off".to_string(),
            cookies: None,
            authentication: AuthenticationConfig::default(),
            theme: "Roxo".to_string(),
            username: None,
            sync_interval_secs: 300,
            artwork_mode: "auto".to_string(),
            home_density: "comfortable".to_string(),
            visualizer: "gradient".to_string(),
            animation_speed: "normal".to_string(),
            reduced_motion: false,
            splash: true,
            lyrics_offset_ms: 0,
        }
    }
}

/// Modo de exibição da capa do álbum (config: `artwork_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtworkMode {
    /// Consulta o terminal pelo protocolo real de imagem (Kitty/Sixel/
    /// iTerm2); cai para blocos Unicode quando não suportado.
    Auto,
    /// Nunca consulta o terminal: sempre usa blocos Unicode (half-blocks).
    HalfBlocks,
    /// Nenhuma capa é baixada nem desenhada.
    Off,
}

impl ArtworkMode {
    /// Interpreta o valor salvo na config; qualquer string desconhecida cai
    /// no padrão (`Auto`) em vez de falhar.
    pub fn from_config(s: &str) -> Self {
        match s {
            "halfblocks" => Self::HalfBlocks,
            "off" => Self::Off,
            _ => Self::Auto,
        }
    }

    /// Valor persistido na config.
    pub fn as_config(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::HalfBlocks => "halfblocks",
            Self::Off => "off",
        }
    }
}

/// Densidade dos cards da grade da tela Início (config: `home_density`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeDensity {
    /// Três linhas por card: título, subtítulo, rodapé.
    Comfortable,
    /// Duas linhas por card: título, rodapé (sem subtítulo).
    Compact,
}

impl HomeDensity {
    /// Interpreta o valor salvo na config; qualquer string desconhecida cai
    /// no padrão (`Comfortable`) em vez de falhar.
    pub fn from_config(s: &str) -> Self {
        match s {
            "compact" => Self::Compact,
            _ => Self::Comfortable,
        }
    }

    /// Valor persistido na config.
    pub fn as_config(self) -> &'static str {
        match self {
            Self::Comfortable => "comfortable",
            Self::Compact => "compact",
        }
    }
}

/// Estilo do visualizador de espectro do player (config: `visualizer`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizerStyle {
    /// Gradiente por altura da célula (cor muda de `player` a `accent`).
    Gradient,
    /// Cor única (`theme.player`) para todas as células, sem gradiente.
    Mono,
    /// Nenhuma barra é desenhada (só o título da faixa).
    Off,
}

impl VisualizerStyle {
    /// Interpreta o valor salvo na config; qualquer string desconhecida cai
    /// no padrão (`Gradient`) em vez de falhar.
    pub fn from_config(s: &str) -> Self {
        match s {
            "mono" => Self::Mono,
            "off" => Self::Off,
            _ => Self::Gradient,
        }
    }

    /// Valor persistido na config.
    pub fn as_config(self) -> &'static str {
        match self {
            Self::Gradient => "gradient",
            Self::Mono => "mono",
            Self::Off => "off",
        }
    }
}

/// Velocidade das animações (config: `animation_speed`). Consumida pela
/// Etapa 6: escala a janela de `App::kick_animation` (mantém o tier rápido de
/// redraw), os estágios de revelação/fade-in (seleção da Home, metadados do
/// now-playing) e o intervalo de passo do marquee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationSpeed {
    Normal,
    Fast,
    Slow,
}

impl AnimationSpeed {
    /// Interpreta o valor salvo na config; qualquer string desconhecida cai
    /// no padrão (`Normal`) em vez de falhar.
    pub fn from_config(s: &str) -> Self {
        match s {
            "fast" => Self::Fast,
            "slow" => Self::Slow,
            _ => Self::Normal,
        }
    }

    /// Valor persistido na config.
    pub fn as_config(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Fast => "fast",
            Self::Slow => "slow",
        }
    }

    /// Fator multiplicativo aplicado a durações/instantes de animação:
    /// `Fast` encurta, `Slow` alonga, `Normal` é a identidade. Único ponto de
    /// verdade para a escala de velocidade — usado tanto por
    /// `App::kick_animation` (janela do tier rápido de redraw) quanto pelas
    /// funções puras de estágio (`ui::main_panel::reveal_stage`,
    /// metadados do now-playing) e pelo intervalo do marquee.
    pub fn factor(self) -> f64 {
        match self {
            Self::Fast => 0.6,
            Self::Normal => 1.0,
            Self::Slow => 1.6,
        }
    }
}

/// Caminho do arquivo de configuração, se o diretório for determinável.
fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ytmtui").join("config.json"))
}

impl Config {
    /// Returns the persisted configuration path when the platform provides one.
    pub fn path() -> Option<PathBuf> {
        config_path()
    }

    /// Carrega a configuração do disco; retorna o padrão em caso de erro.
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        let Ok(contents) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&contents).unwrap_or_default()
    }

    /// Salva a configuração de forma atômica, propagando falhas ao chamador.
    pub fn try_save(&self) -> anyhow::Result<()> {
        let path = config_path()
            .ok_or_else(|| anyhow::anyhow!("não foi possível localizar o diretório de config"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            let mut temporary = NamedTempFile::new_in(parent)?;
            serde_json::to_writer_pretty(temporary.as_file_mut(), self)?;
            temporary.as_file().sync_all()?;
            temporary.persist(&path)?;
        }
        Ok(())
    }

    /// Salva a configuração no disco (falhas são ignoradas silenciosamente).
    pub fn save(&self) {
        let _ = self.try_save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Estes testes desserializam JSON diretamente com `serde_json`, nunca
    // tocando `Config::load`/`save` (que leriam/escreveriam o config.json
    // real do usuário).

    #[test]
    fn legacy_config_defaults_to_account_zero() {
        let config: Config = serde_json::from_str(r#"{"cookies":"/tmp/cookies"}"#).unwrap();
        assert_eq!(config.authentication, AuthenticationConfig::default());
        assert_eq!(config.authentication.auth_user, 0);
    }

    #[test]
    fn authentication_preference_roundtrips() {
        let config = Config {
            authentication: AuthenticationConfig {
                browser: Some("firefox".into()),
                profile: Some("default-release".into()),
                auth_user: 2,
            },
            ..Config::default()
        };
        let decoded: Config =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(decoded.authentication, config.authentication);
    }

    #[test]
    fn old_config_without_the_new_fields_deserializes_with_defaults() {
        let json = r#"{
            "volume": 0.5,
            "shuffle": true,
            "repeat": "all",
            "cookies": null,
            "theme": "Oceano",
            "username": null,
            "sync_interval_secs": 120
        }"#;
        let config: Config = serde_json::from_str(json).expect("old config should deserialize");
        assert_eq!(config.artwork_mode, "auto");
        assert_eq!(config.home_density, "comfortable");
        assert_eq!(config.visualizer, "gradient");
        assert_eq!(config.animation_speed, "normal");
        assert!(!config.reduced_motion);
        assert!(config.splash, "the entry animation is on by default");
        assert_eq!(config.lyrics_offset_ms, 0);
        // Campos antigos continuam lidos normalmente.
        assert_eq!(config.volume, 0.5);
        assert_eq!(config.theme, "Oceano");
    }

    #[test]
    fn invalid_enum_strings_fall_back_to_the_default_variant() {
        assert_eq!(ArtworkMode::from_config("xyz"), ArtworkMode::Auto);
        assert_eq!(HomeDensity::from_config("xyz"), HomeDensity::Comfortable);
        assert_eq!(
            VisualizerStyle::from_config("xyz"),
            VisualizerStyle::Gradient
        );
        assert_eq!(AnimationSpeed::from_config("xyz"), AnimationSpeed::Normal);
        // Vazio ou com caixa diferente também são "desconhecidos".
        assert_eq!(ArtworkMode::from_config(""), ArtworkMode::Auto);
        assert_eq!(ArtworkMode::from_config("Off"), ArtworkMode::Auto);
    }

    #[test]
    fn invalid_artwork_mode_in_json_falls_back_to_default_via_parse() {
        let json = r#"{"artwork_mode": "xyz"}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        // O valor bruto é preservado pelo serde (é só uma String); é
        // `ArtworkMode::from_config` que tolera o valor inválido.
        assert_eq!(config.artwork_mode, "xyz");
        assert_eq!(
            ArtworkMode::from_config(&config.artwork_mode),
            ArtworkMode::Auto
        );
    }

    #[test]
    fn artwork_mode_roundtrips_through_config_strings() {
        for mode in [ArtworkMode::Auto, ArtworkMode::HalfBlocks, ArtworkMode::Off] {
            assert_eq!(ArtworkMode::from_config(mode.as_config()), mode);
        }
    }

    #[test]
    fn home_density_roundtrips_through_config_strings() {
        for density in [HomeDensity::Comfortable, HomeDensity::Compact] {
            assert_eq!(HomeDensity::from_config(density.as_config()), density);
        }
    }

    #[test]
    fn visualizer_style_roundtrips_through_config_strings() {
        for style in [
            VisualizerStyle::Gradient,
            VisualizerStyle::Mono,
            VisualizerStyle::Off,
        ] {
            assert_eq!(VisualizerStyle::from_config(style.as_config()), style);
        }
    }

    #[test]
    fn animation_speed_roundtrips_through_config_strings() {
        for speed in [
            AnimationSpeed::Normal,
            AnimationSpeed::Fast,
            AnimationSpeed::Slow,
        ] {
            assert_eq!(AnimationSpeed::from_config(speed.as_config()), speed);
        }
    }

    #[test]
    fn animation_speed_factor_orders_slow_above_normal_above_fast() {
        assert!(AnimationSpeed::Slow.factor() > AnimationSpeed::Normal.factor());
        assert!(AnimationSpeed::Normal.factor() > AnimationSpeed::Fast.factor());
        assert_eq!(AnimationSpeed::Normal.factor(), 1.0);
    }
}
