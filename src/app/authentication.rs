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
    Preparing {
        operation_id: u64,
    },
    AwaitingConfirmation {
        operation_id: u64,
        preview: SignInPreview,
        selected: usize,
    },
    Activating {
        operation_id: u64,
        preview_id: u64,
    },
}

impl App {
    /// Leaves only when no credential-changing task can still race with
    /// configuration persistence during shutdown.
    pub fn request_quit(&mut self) {
        if matches!(
            self.authentication_flow,
            AuthenticationFlow::Preparing { .. } | AuthenticationFlow::Activating { .. }
        ) {
            self.status = "Aguarde a conexão em andamento terminar antes de sair.".to_string();
            return;
        }
        self.running = false;
    }

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

        let operation_id = self.next_authentication_operation;
        self.next_authentication_operation = self.next_authentication_operation.wrapping_add(1);
        self.begin_task();
        self.authentication_flow = AuthenticationFlow::Preparing { operation_id };
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
                    let _ = tx.send(Msg::SignInPrepared {
                        operation_id,
                        preview,
                    });
                }
                Err(message) => {
                    let _ = tx.send(Msg::SignInFailed {
                        operation_id,
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
            AuthenticationFlow::AwaitingConfirmation {
                preview, selected, ..
            } => Some((preview, *selected)),
            _ => None,
        }
    }

    /// Moves the preview selection down, wrapping at the end of the list.
    pub fn select_next_sign_in_account(&mut self) {
        let AuthenticationFlow::AwaitingConfirmation {
            preview, selected, ..
        } = &mut self.authentication_flow
        else {
            return;
        };
        if !preview.accounts.is_empty() {
            *selected = (*selected + 1) % preview.accounts.len();
        }
    }

    /// Moves the preview selection up, wrapping at the start of the list.
    pub fn select_previous_sign_in_account(&mut self) {
        let AuthenticationFlow::AwaitingConfirmation {
            preview, selected, ..
        } = &mut self.authentication_flow
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
            AuthenticationFlow::AwaitingConfirmation {
                operation_id,
                preview,
                selected,
            } => preview
                .accounts
                .get(*selected)
                .map(|account| (*operation_id, preview.id, account.index)),
            _ => return,
        };
        let Some((operation_id, preview_id, account_index)) = selection else {
            self.status = "Nenhuma conta disponível para conectar.".to_string();
            return;
        };

        self.begin_task();
        // Retire every in-flight account-scoped response from the active
        // generation before the provider may swap cookies. This closes the
        // interval between the provider commit and `Msg::SignedIn` reaching
        // the UI loop.
        self.session_generation = self.session_generation.wrapping_add(1);
        self.authentication_flow = AuthenticationFlow::Activating {
            operation_id,
            preview_id,
        };
        self.status = format!("Conectando ao {}…", self.provider.display_name());

        let provider = Arc::clone(&self.provider);
        let tx = self.tx.clone();
        tokio::task::spawn_blocking(move || {
            match provider.activate_sign_in(preview_id, account_index) {
                Ok(summary) => {
                    let _ = tx.send(Msg::SignedIn {
                        operation_id,
                        preview_id,
                        method: summary.method,
                        credentials_path: summary.credentials_path,
                        account_name: summary.account_name,
                    });
                }
                Err(message) => {
                    let _ = tx.send(Msg::SignInFailed {
                        operation_id,
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
            operation_id: 0,
            preview,
            selected: 0,
        };
    }

    pub(super) fn handle_sign_in_prepared(
        &mut self,
        operation_id: u64,
        mut preview: SignInPreview,
    ) {
        if !matches!(
            self.authentication_flow,
            AuthenticationFlow::Preparing { operation_id: expected } if expected == operation_id
        ) {
            self.provider.cancel_sign_in(preview.id);
            return;
        }
        self.finish_task();

        preview.current_account_name.clone_from(&self.account_name);
        self.status = "Escolha uma conta para concluir a conexão.".to_string();
        self.authentication_flow = AuthenticationFlow::AwaitingConfirmation {
            operation_id,
            preview,
            selected: 0,
        };
    }

    pub(super) fn handle_signed_in(
        &mut self,
        operation_id: u64,
        preview_id: u64,
        method: String,
        credentials_path: Option<String>,
        account_name: String,
    ) {
        if !matches!(
            self.authentication_flow,
            AuthenticationFlow::Activating {
                operation_id: expected_operation,
                preview_id: expected_preview,
            } if expected_operation == operation_id && expected_preview == preview_id
        ) {
            return;
        }
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

    pub(super) fn handle_sign_in_failed(
        &mut self,
        operation_id: u64,
        message: String,
        preview_id: Option<u64>,
    ) {
        let matches_preparing = matches!(
            self.authentication_flow,
            AuthenticationFlow::Preparing { operation_id: expected } if expected == operation_id
        ) && preview_id.is_none();
        let matches_activating = matches!(
            self.authentication_flow,
            AuthenticationFlow::Activating {
                operation_id: expected_operation,
                preview_id: expected_preview,
            } if expected_operation == operation_id && preview_id == Some(expected_preview)
        );
        if !matches_preparing && !matches_activating {
            return;
        }

        self.finish_task();
        if let Some(preview_id) = preview_id {
            self.provider.cancel_sign_in(preview_id);
        }
        self.authentication_flow = AuthenticationFlow::Idle;
        self.status = format!("⚠ {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // This module imports from `super` by name rather than glob, so the
    // model types the fixtures build are not already in scope here.
    #[allow(unused_imports)]
    use crate::app::testing::*;
    use crate::models::Playlist;

    #[test]
    fn test_preview_helper_installs_the_first_account_selection() {
        let mut app = App::new_for_tests();
        app.set_sign_in_preview_for_test(SignInPreview {
            id: 7,
            method: "mock".to_string(),
            profile_label: None,
            accounts: vec![crate::provider::SignInAccount {
                index: 3,
                name: "Preview Account".to_string(),
                handle: None,
            }],
            current_account_name: None,
        });

        let (preview, selected) = app.sign_in_preview().expect("preview installed");
        assert_eq!(preview.id, 7);
        assert_eq!(selected, 0);
    }

    #[test]
    fn stale_sign_in_messages_cannot_publish_or_cancel_another_operation() {
        let mut app = App::new_for_tests();
        app.authentication = AuthState::Authenticated;
        app.account_name = Some("Existing Account".to_string());
        app.authentication_flow = AuthenticationFlow::Activating {
            operation_id: 4,
            preview_id: 7,
        };

        app.tx
            .send(Msg::SignedIn {
                operation_id: 99,
                preview_id: 7,
                method: "firefox".to_string(),
                credentials_path: Some("wrong-cookies.txt".to_string()),
                account_name: "Wrong Account".to_string(),
            })
            .unwrap();
        app.tx
            .send(Msg::SignInFailed {
                operation_id: 99,
                message: "stale failure".to_string(),
                preview_id: Some(7),
            })
            .unwrap();
        app.drain_messages();

        assert!(matches!(
            app.authentication_flow,
            AuthenticationFlow::Activating {
                operation_id: 4,
                preview_id: 7
            }
        ));
        assert_eq!(app.authentication, AuthState::Authenticated);
        assert_eq!(app.account_name.as_deref(), Some("Existing Account"));
        assert_ne!(app.cookies.as_deref(), Some("wrong-cookies.txt"));
    }

    #[test]
    fn stale_session_payloads_cannot_overwrite_a_newly_activated_account() {
        let mut app = App::new_for_tests();
        app.session_generation = 2;
        app.authentication = AuthState::Authenticated;
        app.account_name = Some("New Account".to_string());
        app.home = vec![crate::models::HomeSection {
            title: "New home".to_string(),
            items: vec![],
        }];
        app.library = vec![Playlist {
            title: "New library".to_string(),
            ..Default::default()
        }];

        app.tx
            .send(Msg::HomeSectionsForSession {
                session_generation: 1,
                sections: vec![],
            })
            .unwrap();
        app.tx
            .send(Msg::LibraryPlaylistsForSession {
                session_generation: 1,
                playlists: vec![],
            })
            .unwrap();
        app.tx
            .send(Msg::AccountNameForSession {
                session_generation: 1,
                name: Some("Old Account".to_string()),
            })
            .unwrap();
        app.tx
            .send(Msg::SessionExpiredForSession {
                session_generation: 1,
            })
            .unwrap();
        app.drain_messages();

        assert_eq!(app.authentication, AuthState::Authenticated);
        assert_eq!(app.account_name.as_deref(), Some("New Account"));
        assert_eq!(app.home[0].title, "New home");
        assert_eq!(app.library[0].title, "New library");
    }

    #[tokio::test]
    async fn a_session_expiry_queued_before_sign_in_cannot_expire_the_new_account() {
        let mut app = App::new_for_tests();
        // `confirm_sign_in` advances this before the provider can commit.
        app.session_generation = 1;
        app.authentication_flow = AuthenticationFlow::Activating {
            operation_id: 4,
            preview_id: 7,
        };
        app.tx
            .send(Msg::SessionExpiredForSession {
                session_generation: 0,
            })
            .unwrap();
        app.tx
            .send(Msg::SignedIn {
                operation_id: 4,
                preview_id: 7,
                method: "firefox".to_string(),
                credentials_path: None,
                account_name: "New Account".to_string(),
            })
            .unwrap();

        app.drain_messages();

        assert_eq!(app.authentication, AuthState::Authenticated);
        assert_eq!(app.account_name.as_deref(), Some("New Account"));
    }

    #[tokio::test]
    async fn confirming_sign_in_retires_the_previous_session_before_activation_runs() {
        let mut app = App::new_for_tests();
        app.set_sign_in_preview_for_test(SignInPreview {
            id: 7,
            method: "firefox".to_string(),
            profile_label: None,
            accounts: vec![crate::provider::SignInAccount {
                index: 0,
                name: "New Account".to_string(),
                handle: None,
            }],
            current_account_name: None,
        });

        app.confirm_sign_in();

        assert_eq!(app.session_generation, 1);
    }
}
