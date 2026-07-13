//! Two-phase authentication coordination for [`App`].
//!
//! Preparation is deliberately separate from activation: the provider keeps
//! prepared credentials private while the active account and App state stay
//! unchanged until the user confirms one of the safe account previews.

use std::sync::Arc;

use crate::provider::SignInPreview;

use super::{App, AuthState, Msg};

/// Current phase of the interactive authentication workflow.
#[derive(Debug, Clone)]
pub enum AuthenticationFlow {
    Idle,
    Preparing,
    AwaitingConfirmation {
        preview: SignInPreview,
        selected: usize,
    },
    Activating,
}

impl App {
    /// Prepares browser credentials and account choices without changing the
    /// currently active account, cookies, or authenticated App data.
    pub fn prepare_sign_in(&mut self) {
        if !self.provider.capabilities().sign_in {
            self.status = format!(
                "{} não tem fluxo de conexão interativo.",
                self.provider.display_name()
            );
            return;
        }
        if !matches!(self.authentication_flow, AuthenticationFlow::Idle) {
            self.status = "Aguarde: a conexão anterior ainda está em andamento.".to_string();
            return;
        }

        self.begin_task();
        self.authentication_flow = AuthenticationFlow::Preparing;
        self.status = format!("Preparando conexão com {}…", self.provider.display_name());

        let provider = Arc::clone(&self.provider);
        let tx = self.tx.clone();
        tokio::task::spawn_blocking(move || {
            let progress_tx = tx.clone();
            let progress = move |message: String| {
                let _ = progress_tx.send(Msg::Status(message));
            };
            match provider.prepare_sign_in(&progress) {
                Ok(preview) => {
                    let _ = tx.send(Msg::SignInPrepared(preview));
                }
                Err(message) => {
                    let _ = tx.send(Msg::SignInFailed {
                        message: format!("Falha ao preparar conexão — {message}"),
                        preview_id: None,
                    });
                }
            }
        });
    }

    /// Returns the safe preview and selected list position while confirmation
    /// is pending. Provider-owned credential paths never enter this state.
    pub fn sign_in_preview(&self) -> Option<(&SignInPreview, usize)> {
        match &self.authentication_flow {
            AuthenticationFlow::AwaitingConfirmation { preview, selected } => {
                Some((preview, *selected))
            }
            _ => None,
        }
    }

    /// Moves the preview selection down, wrapping at the end of the list.
    pub fn select_next_sign_in_account(&mut self) {
        let AuthenticationFlow::AwaitingConfirmation { preview, selected } =
            &mut self.authentication_flow
        else {
            return;
        };
        if !preview.accounts.is_empty() {
            *selected = (*selected + 1) % preview.accounts.len();
        }
    }

    /// Moves the preview selection up, wrapping at the start of the list.
    pub fn select_previous_sign_in_account(&mut self) {
        let AuthenticationFlow::AwaitingConfirmation { preview, selected } =
            &mut self.authentication_flow
        else {
            return;
        };
        if !preview.accounts.is_empty() {
            *selected = selected
                .checked_sub(1)
                .unwrap_or(preview.accounts.len() - 1);
        }
    }

    /// Activates the selected account. This is the only asynchronous phase
    /// that may commit provider credentials and later update App account data.
    pub fn confirm_sign_in(&mut self) {
        let selection = match &self.authentication_flow {
            AuthenticationFlow::AwaitingConfirmation { preview, selected } => preview
                .accounts
                .get(*selected)
                .map(|account| (preview.id, account.index)),
            _ => return,
        };
        let Some((preview_id, account_index)) = selection else {
            self.status = "Nenhuma conta disponível para conectar.".to_string();
            return;
        };

        self.begin_task();
        self.authentication_flow = AuthenticationFlow::Activating;
        self.status = format!("Conectando ao {}…", self.provider.display_name());

        let provider = Arc::clone(&self.provider);
        let tx = self.tx.clone();
        tokio::task::spawn_blocking(move || {
            match provider.activate_sign_in(preview_id, account_index) {
                Ok(summary) => {
                    let _ = tx.send(Msg::SignedIn {
                        method: summary.method,
                        credentials_path: summary.credentials_path,
                        account_name: summary.account_name,
                    });
                }
                Err(message) => {
                    let _ = tx.send(Msg::SignInFailed {
                        message: format!("Falha ao conectar — {message}"),
                        preview_id: Some(preview_id),
                    });
                }
            }
        });
    }

    /// Discards a prepared sign-in while preserving all active account state.
    pub fn cancel_sign_in(&mut self) {
        let previous = std::mem::replace(&mut self.authentication_flow, AuthenticationFlow::Idle);
        if let AuthenticationFlow::AwaitingConfirmation { preview, .. } = previous {
            self.provider.cancel_sign_in(preview.id);
            self.status = "Conexão cancelada; a conta atual foi preservada.".to_string();
        } else {
            self.authentication_flow = previous;
        }
    }

    #[cfg(test)]
    pub(crate) fn set_sign_in_preview_for_test(&mut self, preview: SignInPreview) {
        self.authentication_flow = AuthenticationFlow::AwaitingConfirmation {
            preview,
            selected: 0,
        };
    }

    pub(super) fn handle_sign_in_prepared(&mut self, mut preview: SignInPreview) {
        self.finish_task();
        if !matches!(self.authentication_flow, AuthenticationFlow::Preparing) {
            self.provider.cancel_sign_in(preview.id);
            return;
        }

        preview.current_account_name.clone_from(&self.account_name);
        self.status = "Escolha uma conta para concluir a conexão.".to_string();
        self.authentication_flow = AuthenticationFlow::AwaitingConfirmation {
            preview,
            selected: 0,
        };
    }

    pub(super) fn handle_signed_in(
        &mut self,
        method: String,
        credentials_path: Option<String>,
        account_name: String,
    ) {
        self.finish_task();
        self.authentication_flow = AuthenticationFlow::Idle;
        self.authentication = AuthState::Authenticated;
        if credentials_path.is_some() {
            self.cookies = credentials_path;
        }
        self.account_name = Some(account_name);
        self.status = format!("✔ Conectado via {method}. Carregando suas músicas…");

        // Account-only data is reloaded only after activation has committed.
        self.load_account();
        self.load_home();
        self.load_library();
    }

    pub(super) fn handle_sign_in_failed(&mut self, message: String, preview_id: Option<u64>) {
        self.finish_task();
        if let Some(preview_id) = preview_id {
            self.provider.cancel_sign_in(preview_id);
        }
        self.authentication_flow = AuthenticationFlow::Idle;
        self.status = format!("⚠ {message}");
    }
}
