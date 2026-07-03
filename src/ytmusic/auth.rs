//! Autenticação do YouTube Music via cookies do navegador.
//!
//! O YouTube Music (e demais serviços Google) autentica requisições da API
//! interna com o cabeçalho `Authorization: SAPISIDHASH <ts>_<sha1>`, derivado do
//! cookie `SAPISID`/`__Secure-3PAPISID`. Aqui lemos um arquivo de cookies no
//! formato Netscape (o mesmo usado pelo `yt-dlp` via `YTM_COOKIES`), montamos o
//! cabeçalho `Cookie` e calculamos o hash de autorização.

use sha1::{Digest, Sha1};

/// Origem usada no cálculo do SAPISIDHASH.
pub const ORIGIN: &str = "https://music.youtube.com";

/// Dados de autenticação extraídos do arquivo de cookies.
#[derive(Debug, Clone)]
pub struct Auth {
    /// Valor completo do cabeçalho `Cookie` (todos os pares `nome=valor`).
    pub cookie_header: String,
    /// Valor do SAPISID usado no cálculo do hash de autorização.
    pub sapisid: String,
}

impl Auth {
    /// Lê e interpreta um arquivo de cookies (formato Netscape).
    ///
    /// Retorna `None` se o arquivo não puder ser lido ou não contiver os
    /// cookies necessários (em especial o SAPISID).
    pub fn from_cookie_file(path: &str) -> Option<Auth> {
        let content = std::fs::read_to_string(path).ok()?;
        let mut pairs = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut sapisid: Option<String> = None;

        for raw in content.lines() {
            // Cookies "HttpOnly" são escritos com o prefixo "#HttpOnly_".
            let line = raw.strip_prefix("#HttpOnly_").unwrap_or(raw);
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }

            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 7 {
                continue;
            }
            let domain = fields[0].trim();
            let name = fields[5].trim();
            let value = fields[6].trim();
            if name.is_empty() {
                continue;
            }

            // Envia apenas cookies do domínio youtube.com. Arquivos exportados
            // do navegador contêm cookies de muitos sites (o que estoura o
            // cabeçalho — HTTP 413) e cookies homônimos de outros domínios
            // Google (`.google.com`), com valores de sessão diferentes, que
            // fazem a API tratar a requisição como anônima.
            if !domain.contains("youtube.com") {
                continue;
            }

            // Evita nomes duplicados no cabeçalho Cookie.
            if !seen.insert(name.to_string()) {
                continue;
            }

            pairs.push(format!("{name}={value}"));

            // Prefere __Secure-3PAPISID; cai para SAPISID quando ausente.
            if name == "__Secure-3PAPISID" {
                sapisid = Some(value.to_string());
            } else if name == "SAPISID" && sapisid.is_none() {
                sapisid = Some(value.to_string());
            }
        }

        let sapisid = sapisid?;
        if pairs.is_empty() {
            return None;
        }

        Some(Auth { cookie_header: pairs.join("; "), sapisid })
    }

    /// Calcula o cabeçalho `Authorization: SAPISIDHASH ...` para o instante atual.
    pub fn authorization_header(&self) -> String {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut hasher = Sha1::new();
        hasher.update(format!("{ts} {} {ORIGIN}", self.sapisid).as_bytes());
        let digest = hasher.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();

        format!("SAPISIDHASH {ts}_{hex}")
    }
}
