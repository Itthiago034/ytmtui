//! Funções auxiliares para navegar na resposta JSON (InnerTube) do YouTube Music.
//!
//! As respostas da API interna do YouTube Music são profundamente aninhadas e
//! mudam com frequência. Para tornar o parsing resiliente, usamos uma busca
//! recursiva por chaves em vez de caminhos fixos sempre que possível.

use serde_json::Value;

/// Procura recursivamente pela primeira ocorrência de uma chave em um `Value`.
pub fn find_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            if let Some(v) = map.get(key) {
                return Some(v);
            }
            for v in map.values() {
                if let Some(found) = find_key(v, key) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(arr) => {
            for v in arr {
                if let Some(found) = find_key(v, key) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

/// Coleta recursivamente todas as ocorrências de uma chave em um `Value`.
pub fn collect_key<'a>(value: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            if let Some(v) = map.get(key) {
                out.push(v);
            }
            for v in map.values() {
                collect_key(v, key, out);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_key(v, key, out);
            }
        }
        _ => {}
    }
}

/// Concatena o texto de um objeto `{ "runs": [{ "text": ... }] }`.
pub fn join_runs(text_obj: &Value) -> String {
    if let Some(runs) = text_obj.get("runs").and_then(|r| r.as_array()) {
        runs.iter()
            .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("")
    } else if let Some(simple) = text_obj.get("simpleText").and_then(|s| s.as_str()) {
        simple.to_string()
    } else {
        String::new()
    }
}

/// Extrai o nome (ou handle) da conta a partir de `account/account_menu`.
pub fn parse_account_name(data: &Value) -> Option<String> {
    if let Some(header) = find_key(data, "activeAccountHeaderRenderer") {
        for key in ["accountName", "channelHandle"] {
            if let Some(field) = header.get(key) {
                let text = join_runs(field);
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    if let Some(name) = find_key(data, "accountName") {
        let text = join_runs(name);
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

/// Extrai a URL da thumbnail em melhor resolução de um item.
pub fn extract_thumbnail(item: &Value) -> Option<String> {
    let thumbs = find_key(item, "thumbnails")?.as_array()?;
    thumbs
        .last()
        .and_then(|t| t.get("url"))
        .and_then(|u| u.as_str())
        .map(|s| s.to_string())
}

/// Converte uma duração no formato "m:ss" ou "h:mm:ss" para segundos.
pub fn parse_duration(text: &str) -> u64 {
    let parts: Vec<&str> = text.trim().split(':').collect();
    let mut secs: u64 = 0;
    for p in &parts {
        if let Ok(n) = p.trim().parse::<u64>() {
            secs = secs * 60 + n;
        } else {
            return 0;
        }
    }
    secs
}

/// Extrai os textos de cada `flexColumn` de um `musicResponsiveListItemRenderer`.
pub fn flex_texts(renderer: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(cols) = renderer.get("flexColumns").and_then(|c| c.as_array()) {
        for col in cols {
            if let Some(text) = col
                .get("musicResponsiveListItemFlexColumnRenderer")
                .and_then(|r| r.get("text"))
            {
                out.push(join_runs(text));
            }
        }
    }
    out
}

/// Extrai o `browseId` do `navigationEndpoint` de nível superior do item.
///
/// Importante: NÃO usar busca recursiva aqui, pois colunas de texto podem
/// conter links para o autor/canal (que sobrescreveriam o id da playlist).
pub fn top_browse_id(renderer: &Value) -> String {
    renderer
        .get("navigationEndpoint")
        .and_then(|n| n.get("browseEndpoint"))
        .and_then(|b| b.get("browseId"))
        .and_then(|b| b.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

/// Extrai um token de continuação (paginação) de uma resposta InnerTube.
///
/// Suporta o formato novo (`continuationCommand.token`) e o antigo
/// (`nextContinuationData.continuation`). Retorna `None` quando não há mais
/// páginas a carregar.
pub fn extract_continuation(value: &Value) -> Option<String> {
    if let Some(cmd) = find_key(value, "continuationCommand") {
        if let Some(t) = cmd.get("token").and_then(|t| t.as_str()) {
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    if let Some(data) = find_key(value, "nextContinuationData") {
        if let Some(t) = data.get("continuation").and_then(|t| t.as_str()) {
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

/// Converte um `playlistPanelVideoRenderer` (item de fila/rádio) em `Track`.
pub fn parse_panel_video(r: &Value) -> Option<crate::ytmusic::Track> {
    let video_id = r
        .get("videoId")
        .and_then(|v| v.as_str())
        .or_else(|| {
            find_key(r, "watchEndpoint")
                .and_then(|w| w.get("videoId"))
                .and_then(|v| v.as_str())
        })?
        .to_string();
    if video_id.is_empty() {
        return None;
    }
    let title = r.get("title").map(join_runs).unwrap_or_default();
    let byline = r
        .get("longBylineText")
        .or_else(|| r.get("shortBylineText"))
        .map(join_runs)
        .unwrap_or_default();
    let segments: Vec<&str> = byline.split('•').map(|s| s.trim()).collect();
    let artist = segments.first().map(|s| s.to_string()).unwrap_or_default();
    let album = segments
        .iter()
        .skip(1)
        .find(|s| !s.contains(':') && !s.ends_with("views") && !s.contains("visualiz"))
        .map(|s| s.to_string())
        .unwrap_or_default();
    let duration = r.get("lengthText").map(join_runs).unwrap_or_default();
    Some(crate::ytmusic::Track {
        video_id,
        title,
        artist,
        album,
        duration_secs: parse_duration(&duration),
        duration,
        thumbnail: extract_thumbnail(r),
    })
}

/// Extrai a duração (texto) de um `fixedColumn`, comum em itens de playlist.
pub fn fixed_duration(renderer: &Value) -> Option<String> {
    let cols = renderer.get("fixedColumns")?.as_array()?;
    for col in cols {
        if let Some(text) = col
            .get("musicResponsiveListItemFixedColumnRenderer")
            .and_then(|r| r.get("text"))
        {
            let s = join_runs(text);
            if s.contains(':') {
                return Some(s);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_duration_handles_formats() {
        assert_eq!(parse_duration("4:27"), 267);
        assert_eq!(parse_duration("0:05"), 5);
        assert_eq!(parse_duration("1:02:03"), 3723);
        assert_eq!(parse_duration(""), 0);
        assert_eq!(parse_duration("abc"), 0);
        assert_eq!(parse_duration("3"), 3);
    }

    #[test]
    fn join_runs_concatenates_and_handles_simple_text() {
        let runs = json!({ "runs": [{ "text": "Hello " }, { "text": "World" }] });
        assert_eq!(join_runs(&runs), "Hello World");

        let simple = json!({ "simpleText": "Só texto" });
        assert_eq!(join_runs(&simple), "Só texto");

        let empty = json!({ "foo": "bar" });
        assert_eq!(join_runs(&empty), "");
    }

    #[test]
    fn find_key_searches_recursively() {
        let v = json!({ "a": { "b": [ { "target": 42 } ] } });
        assert_eq!(find_key(&v, "target"), Some(&json!(42)));
        assert!(find_key(&v, "missing").is_none());
    }

    #[test]
    fn extract_thumbnail_returns_highest_resolution() {
        let v = json!({
            "thumbnails": [
                { "url": "small", "width": 60 },
                { "url": "large", "width": 544 }
            ]
        });
        assert_eq!(extract_thumbnail(&v).as_deref(), Some("large"));
    }

    #[test]
    fn flex_texts_extracts_all_columns() {
        let renderer = json!({
            "flexColumns": [
                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Título" }] } } },
                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "simpleText": "Artista • Álbum" } } }
            ]
        });
        let texts = flex_texts(&renderer);
        assert_eq!(
            texts,
            vec!["Título".to_string(), "Artista • Álbum".to_string()]
        );
    }

    #[test]
    fn extract_continuation_supports_both_formats() {
        let new_fmt = json!({
            "continuationItemRenderer": {
                "continuationEndpoint": {
                    "continuationCommand": { "token": "TOKEN_NOVO" }
                }
            }
        });
        assert_eq!(
            extract_continuation(&new_fmt).as_deref(),
            Some("TOKEN_NOVO")
        );

        let old_fmt = json!({
            "continuations": [{ "nextContinuationData": { "continuation": "TOKEN_ANTIGO" } }]
        });
        assert_eq!(
            extract_continuation(&old_fmt).as_deref(),
            Some("TOKEN_ANTIGO")
        );

        let none = json!({ "foo": "bar" });
        assert!(extract_continuation(&none).is_none());
    }
}
