//! Preferences, theme, and account identity.
//!
//! `save_config` is the single writer of `config.json`; it re-reads the file
//! first so fields the running app does not own are preserved rather than
//! clobbered with empty values.

use super::*;

impl App {
    pub fn is_authenticated(&self) -> bool {
        self.authentication.is_authenticated()
    }

    /// Carrega (em background) o nome da conta, se autenticado e sem nome
    /// personalizado já definido na config.
    pub fn load_account(&mut self) {
        if !self.is_authenticated() {
            return;
        }
        self.begin_task();
        let provider = Arc::clone(&self.provider);
        let tx = self.tx.clone();
        let session_generation = self.session_generation;
        tokio::spawn(async move {
            match provider.account_name().await {
                // `None` também é enviado: toda tarefa contada precisa
                // terminar em exatamente uma mensagem (ver `begin_task`).
                Ok(name) => {
                    let _ = tx.send(Msg::AccountNameForSession {
                        session_generation,
                        name,
                    });
                }
                Err(ProviderError::SessionExpired) => {
                    let _ = tx.send(Msg::SessionExpiredForSession { session_generation });
                }
                Err(error) => {
                    let _ = tx.send(Msg::Error(format!("Could not load account: {error}")));
                }
            }
        });
    }

    /// Tema de cores ativo.
    pub fn theme(&self) -> &'static crate::theme::Theme {
        crate::theme::get(self.theme_index)
    }

    /// Alterna para o próximo tema de cores e salva a preferência.
    pub fn cycle_theme(&mut self) {
        self.theme_index = (self.theme_index + 1) % crate::theme::THEMES.len();
        self.status = format!("🎨 Tema: {}", self.theme().name);
        self.save_config();
    }

    /// Persiste as preferências atuais em disco.
    pub fn save_config(&self) {
        // Nunca apaga um caminho de cookies válido já salvo: se o app subiu
        // sem cookies (self.cookies == None), preserva o que estiver em disco.
        let saved = Config::load();
        let cookies = self.cookies.clone().or(saved.cookies);
        // Só persiste um username se for personalizado (mantém o que já existia
        // em vez de gravar o nome obtido da API automaticamente).
        let username = saved.username;
        Config {
            volume: self.player.volume(),
            shuffle: self.shuffle,
            repeat: self.repeat.as_config().to_string(),
            cookies,
            authentication: saved.authentication,
            theme: self.theme().name.to_string(),
            username,
            // Not editable at runtime yet; preserve whatever's on disk
            // rather than overwriting it with the in-memory Duration.
            sync_interval_secs: saved.sync_interval_secs,
            // Same story as `sync_interval_secs` above: these six have no
            // in-app editor yet, so whatever's on disk wins over the
            // in-memory value loaded at startup.
            artwork_mode: saved.artwork_mode,
            home_density: saved.home_density,
            visualizer: saved.visualizer,
            animation_speed: saved.animation_speed,
            reduced_motion: saved.reduced_motion,
            splash: saved.splash,
            lyrics_offset_ms: self.ui.lyrics.offset_ms(),
        }
        .save();
    }
}
