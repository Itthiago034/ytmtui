//! Provas de fronteira: o aplicativo inteiro — busca, Início, biblioteca,
//! sign-in, erros e reprodução — funciona contra um `MusicProvider`
//! genérico (o mock), sem nenhuma dependência de YouTube. Tudo aqui passa
//! apenas pela superfície pública do crate, como um segundo provedor real
//! passaria.

use std::sync::Arc;
use std::time::Duration;

use ytmtui::app::{App, AuthState, Section};
use ytmtui::models::{HomeSection, Playlist, SearchResults, Track};
use ytmtui::provider::mock::MockProvider;
use ytmtui::provider::{Capabilities, MusicProvider};

fn track(id: &str, title: &str) -> Track {
    Track {
        video_id: id.to_string(),
        title: title.to_string(),
        artist: "Someone".to_string(),
        ..Default::default()
    }
}

fn playlist(id: &str, title: &str) -> Playlist {
    Playlist {
        browse_id: id.to_string(),
        title: title.to_string(),
        ..Default::default()
    }
}

/// Deixa as tasks do runtime rodarem e drena mensagens até o app ficar
/// ocioso (spinner apagado), com um teto para nunca travar a suíte.
async fn drain_until_idle(app: &mut App) {
    for _ in 0..500 {
        tokio::task::yield_now().await;
        app.drain_messages();
        if !app.is_loading() {
            // Última drenagem: mensagens que chegaram na mesma volta em que
            // o contador zerou.
            app.drain_messages();
            return;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    panic!("o app nunca ficou ocioso — alguma tarefa não terminou");
}

#[tokio::test]
async fn search_flows_from_a_generic_provider_into_the_mixed_view() {
    let mut mock = MockProvider::default();
    mock.search_results = SearchResults {
        songs: vec![track("s1", "One"), track("s2", "Two")],
        ..Default::default()
    };
    let mut app = App::with_provider(Arc::new(mock));

    app.query = "one".to_string();
    app.do_search();
    assert!(app.is_loading(), "a busca liga o spinner");
    drain_until_idle(&mut app).await;

    assert!(app.search_mixed, "resultados mistos ativos");
    assert_eq!(app.songs.len(), 2);
    assert_eq!(app.songs[0].title, "One");
}

#[tokio::test]
async fn home_and_library_load_from_a_generic_provider() {
    let mut mock = MockProvider::authenticated();
    mock.home_sections = vec![HomeSection {
        title: "Quick picks".to_string(),
        items: vec![playlist("VL1", "Mix 1")],
    }];
    mock.library = vec![playlist("L1", "Minhas favoritas")];
    let mut app = App::with_provider(Arc::new(mock));
    assert!(app.authentication.is_authenticated());

    app.sync_home_and_library();
    drain_until_idle(&mut app).await;

    assert_eq!(app.home.len(), 1);
    assert_eq!(app.home[0].items[0].title, "Mix 1");
    assert_eq!(app.library.len(), 1);
    assert_eq!(
        app.list_state.selected(),
        Some(0),
        "primeiro carregamento da Home seleciona o topo"
    );
}

#[tokio::test]
async fn app_prepares_then_confirms_selected_account() {
    let mut mock = MockProvider::default();
    mock.home_sections = vec![HomeSection {
        title: "Signed-in picks".to_string(),
        items: vec![playlist("H1", "Account mix")],
    }];
    mock.library = vec![playlist("L1", "Account library")];
    let mut app = App::with_provider(Arc::new(mock));
    assert_eq!(app.authentication, AuthState::Anonymous);

    app.prepare_sign_in();
    drain_until_idle(&mut app).await;
    assert!(app.sign_in_preview().is_some());
    assert_eq!(app.authentication, AuthState::Anonymous);
    assert!(app.home.is_empty());
    assert!(app.library.is_empty());

    app.select_next_sign_in_account();
    app.confirm_sign_in();
    drain_until_idle(&mut app).await;

    assert_eq!(app.account_name.as_deref(), Some("Mock Account 2"));
    assert_eq!(app.authentication, AuthState::Authenticated);
    assert_eq!(app.home[0].title, "Signed-in picks");
    assert_eq!(app.library[0].title, "Account library");
}

#[tokio::test]
async fn cancelling_a_preview_preserves_current_account_and_authentication() {
    let provider = Arc::new(MockProvider::authenticated());
    let mut app = App::with_provider(provider.clone());
    app.account_name = Some("Existing Account".to_string());
    app.cookies = Some("existing-cookies.txt".to_string());
    app.home = vec![HomeSection {
        title: "Existing home".to_string(),
        items: vec![],
    }];
    app.library = vec![playlist("OLD", "Existing library")];

    app.prepare_sign_in();
    drain_until_idle(&mut app).await;

    let (preview, selected) = app.sign_in_preview().expect("preview prepared");
    assert_eq!(selected, 0);
    assert_eq!(
        preview.current_account_name.as_deref(),
        Some("Existing Account")
    );
    assert_eq!(app.authentication, AuthState::Authenticated);
    assert_eq!(app.account_name.as_deref(), Some("Existing Account"));
    assert_eq!(app.cookies.as_deref(), Some("existing-cookies.txt"));
    assert_eq!(app.home[0].title, "Existing home");
    assert_eq!(app.library[0].title, "Existing library");

    app.cancel_sign_in();

    assert!(app.sign_in_preview().is_none());
    assert_eq!(app.authentication, AuthState::Authenticated);
    assert_eq!(app.account_name.as_deref(), Some("Existing Account"));
    assert_eq!(app.cookies.as_deref(), Some("existing-cookies.txt"));
    assert_eq!(app.home[0].title, "Existing home");
    assert_eq!(app.library[0].title, "Existing library");
    assert!(provider.is_authenticated());
}

#[tokio::test]
async fn sign_in_account_selection_wraps_in_both_directions() {
    let mut app = App::with_provider(Arc::new(MockProvider::default()));
    app.prepare_sign_in();
    drain_until_idle(&mut app).await;

    app.select_previous_sign_in_account();
    assert_eq!(app.sign_in_preview().unwrap().1, 1);
    app.select_next_sign_in_account();
    assert_eq!(app.sign_in_preview().unwrap().1, 0);

    app.cancel_sign_in();
}

#[tokio::test]
async fn prepare_cancel_preserves_old_session() {
    let provider = Arc::new(MockProvider::authenticated());

    let preview = provider.prepare_sign_in(&|_| {}).unwrap();
    provider.cancel_sign_in(preview.id);

    assert!(provider.is_authenticated());
}

#[tokio::test]
async fn activation_uses_selected_account() {
    let provider = Arc::new(MockProvider::default());

    let preview = provider.prepare_sign_in(&|_| {}).unwrap();
    let summary = provider.activate_sign_in(preview.id, 1).unwrap();

    assert_eq!(
        (summary.account_name.as_str(), summary.account_index),
        ("Mock Account 2", 1)
    );
}

#[tokio::test]
async fn invalid_preview_and_account_do_not_consume_pending_sign_in() {
    let provider = Arc::new(MockProvider::default());
    let preview = provider.prepare_sign_in(&|_| {}).unwrap();

    assert!(provider.activate_sign_in(preview.id + 1, 0).is_err());
    assert!(provider.activate_sign_in(preview.id, 9).is_err());
    assert!(!provider.is_authenticated());

    let summary = provider.activate_sign_in(preview.id, 0).unwrap();
    assert_eq!(summary.account_index, 0);
    assert!(provider.is_authenticated());
}

#[tokio::test]
async fn nonmatching_cancel_keeps_the_pending_sign_in() {
    let provider = Arc::new(MockProvider::default());
    let preview = provider.prepare_sign_in(&|_| {}).unwrap();

    provider.cancel_sign_in(preview.id + 1);

    let summary = provider.activate_sign_in(preview.id, 1).unwrap();
    assert_eq!(summary.account_name, "Mock Account 2");
}

#[tokio::test]
async fn provider_errors_surface_in_the_status_bar_and_clear_the_spinner() {
    let mut mock = MockProvider::default();
    mock.fail_with = Some("sem rede".to_string());
    let mut app = App::with_provider(Arc::new(mock));

    app.query = "x".to_string();
    app.do_search();
    drain_until_idle(&mut app).await;

    assert!(
        app.status.contains("sem rede"),
        "erro legível na status bar: {}",
        app.status
    );
    assert!(!app.is_loading(), "spinner liberado após o erro");
}

#[tokio::test]
async fn a_failed_home_refresh_keeps_cached_shelves_and_sets_home_error() {
    let mut mock = MockProvider::authenticated();
    mock.fail_with = Some("sem rede".to_string());
    let mut app = App::with_provider(Arc::new(mock));
    // Simulates shelves left over from an earlier successful load.
    app.home = vec![HomeSection {
        title: "Quick picks".to_string(),
        items: vec![playlist("VL1", "Mix 1")],
    }];

    app.load_home();
    assert!(app.is_loading(), "the refresh turns on the spinner");
    drain_until_idle(&mut app).await;

    assert_eq!(
        app.home.len(),
        1,
        "cached shelves survive a failed background refresh"
    );
    assert_eq!(app.home[0].items[0].title, "Mix 1");
    assert!(
        app.home_error.is_some(),
        "the failure is recorded for the retry banner"
    );
    assert!(!app.is_loading(), "spinner released after the failure");
}

#[tokio::test]
async fn an_expired_session_from_any_provider_flips_the_auth_state() {
    let mut mock = MockProvider::authenticated();
    mock.expire_session = true;
    let mut app = App::with_provider(Arc::new(mock));

    app.load_library();
    drain_until_idle(&mut app).await;

    assert_eq!(app.authentication, AuthState::Expired);
    assert!(
        app.status.contains("expired") || app.status.contains("expirada"),
        "status orienta a reautenticação: {}",
        app.status
    );
}

#[tokio::test]
async fn unsupported_capabilities_suppress_actions_instead_of_failing() {
    let mut mock = MockProvider::authenticated();
    mock.capabilities = Capabilities::none();
    let mut app = App::with_provider(Arc::new(mock));

    app.load_home();
    assert!(!app.is_loading(), "sem capability, nenhuma tarefa é criada");

    app.prepare_sign_in();
    assert!(!app.is_loading());
    assert!(
        app.status.contains("não tem fluxo de conexão"),
        "explica a ausência do sign-in: {}",
        app.status
    );

    app.current = Some(track("t1", "Song"));
    app.like_current();
    assert!(
        app.status.contains("não suporta curtir"),
        "explica a ausência do like: {}",
        app.status
    );
}

#[tokio::test]
async fn playback_errors_identify_the_provider_and_preserve_the_queue() {
    let mut mock = MockProvider::default();
    mock.search_results = SearchResults {
        songs: vec![track("s1", "One")],
        ..Default::default()
    };
    let mut app = App::with_provider(Arc::new(mock));

    app.query = "one".to_string();
    app.do_search();
    drain_until_idle(&mut app).await;

    // Toca a música dos resultados; o mock não resolve áudio, então o
    // download falha — o erro deve nomear o provedor e a fila sobreviver.
    app.section = Section::Buscar;
    app.list_state.select(Some(0));
    app.play_selected();
    drain_until_idle(&mut app).await;

    assert!(
        app.status.contains("Mock Provider"),
        "o erro identifica o provedor de origem: {}",
        app.status
    );
    assert!(!app.loading_audio, "estado de download liberado");
    assert_eq!(app.queue.len(), 1, "a fila é preservada após o erro");
    assert_eq!(app.queue[0].video_id, "s1");
}
