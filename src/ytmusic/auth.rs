//! Autenticação do YouTube Music via cookies do navegador.
//!
//! O YouTube Music (e demais serviços Google) autentica requisições da API
//! interna com o cabeçalho `Authorization: SAPISIDHASH <ts>_<sha1>`, derivado do
//! cookie `SAPISID`/`__Secure-3PAPISID`. Aqui lemos um arquivo de cookies no
//! formato Netscape (o mesmo usado pelo `yt-dlp` via `YTM_COOKIES`), montamos o
//! cabeçalho `Cookie` e calculamos o hash de autorização.

use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::PathBuf;

/// Origem usada no cálculo do SAPISIDHASH.
pub const ORIGIN: &str = "https://music.youtube.com";

#[derive(Debug)]
pub enum AuthError {
    ReadFile { path: PathBuf, source: io::Error },
    MissingSapisid,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadFile { path, .. } => {
                write!(f, "could not read cookie file {}", path.display())
            }
            Self::MissingSapisid => write!(f, "cookie file does not contain a SAPISID cookie"),
        }
    }
}

impl std::error::Error for AuthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadFile { source, .. } => Some(source),
            Self::MissingSapisid => None,
        }
    }
}

/// Dados de autenticação extraídos do arquivo de cookies.
#[derive(Clone)]
pub struct Auth {
    /// Valor completo do cabeçalho `Cookie` (todos os pares `nome=valor`).
    pub cookie_header: String,
    /// Valor do SAPISID usado no cálculo do hash de autorização.
    pub sapisid: String,
}

/// Prioridade de domínio ao montar o cabeçalho `Cookie`.
///
/// Em nomes duplicados (ex.: `__Secure-3PSID` em `.google.com` e `.youtube.com`),
/// preferimos o cookie do domínio do YouTube — enviar ambos ou o valor errado
/// faz a API tratar a requisição como anônima.
fn domain_priority(domain: &str) -> u8 {
    if domain.contains("youtube.com") {
        0
    } else if domain.contains("google.com.br") {
        1
    } else if domain.contains("google.com") {
        2
    } else {
        u8::MAX
    }
}

fn is_allowed_domain(domain: &str) -> bool {
    domain.contains("youtube.com") || domain.contains("google.com")
}

impl Auth {
    /// Lê e interpreta um arquivo de cookies (formato Netscape).
    ///
    /// Retorna `None` se o arquivo não puder ser lido ou não contiver os
    /// cookies necessários (em especial o SAPISID).
    pub fn from_cookie_file(path: &str) -> Result<Auth, AuthError> {
        let content =
            std::fs::read_to_string(path).map_err(|source| AuthError::ReadFile {
                path: PathBuf::from(path),
                source,
            })?;
        Self::from_cookie_text(&content)
    }

    /// Interpreta o conteúdo bruto de um arquivo Netscape (útil em testes).
    fn from_cookie_text(content: &str) -> Result<Auth, AuthError> {
        // name -> (value, domain, priority)
        let mut chosen: HashMap<String, (String, String, u8)> = HashMap::new();

        for raw in content.lines() {
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
            if name.is_empty() || !is_allowed_domain(domain) {
                continue;
            }

            let prio = domain_priority(domain);
            match chosen.get(name) {
                Some((_, _, existing_prio)) if *existing_prio <= prio => {}
                _ => {
                    chosen.insert(
                        name.to_string(),
                        (value.to_string(), domain.to_string(), prio),
                    );
                }
            }
        }

        let sapisid = chosen
            .get("__Secure-3PAPISID")
            .filter(|(_, domain, _)| domain.contains("youtube.com"))
            .map(|(v, _, _)| v.clone())
            .or_else(|| chosen.get("__Secure-3PAPISID").map(|(v, _, _)| v.clone()))
            .or_else(|| chosen.get("SAPISID").map(|(v, _, _)| v.clone()))
            .ok_or(AuthError::MissingSapisid)?;

        let mut pairs: Vec<_> = chosen
            .into_iter()
            .map(|(name, (value, _, _))| (name, value))
            .collect();
        pairs.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        let cookie_header = pairs
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");

        Ok(Auth {
            cookie_header,
            sapisid,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_youtube_cookie_over_google_for_same_name() {
        let text = "\
# Netscape HTTP Cookie File
.google.com\tTRUE\t/\tTRUE\t9999999999\t__Secure-3PSID\tgoogle_psid
.youtube.com\tTRUE\t/\tTRUE\t9999999999\t__Secure-3PSID\tyoutube_psid
.google.com\tTRUE\t/\tTRUE\t9999999999\tSID\tgoogle_sid
.youtube.com\tTRUE\t/\tTRUE\t9999999999\t__Secure-3PAPISID\tyoutube_papisid
";
        let auth = Auth::from_cookie_text(text).expect("auth");
        assert!(auth.cookie_header.contains("__Secure-3PSID=youtube_psid"));
        assert!(auth.cookie_header.contains("SID=google_sid"));
        assert!(!auth.cookie_header.contains("google_psid"));
        assert_eq!(auth.sapisid, "youtube_papisid");
    }

    #[test]
    fn falls_back_to_google_sapisid_when_youtube_missing() {
        let text = "\
.google.com\tTRUE\t/\tTRUE\t9999999999\tSAPISID\tgoogle_sapisid
.google.com\tTRUE\t/\tTRUE\t9999999999\tSID\tgoogle_sid
";
        let auth = Auth::from_cookie_text(text).expect("auth");
        assert_eq!(auth.sapisid, "google_sapisid");
        assert!(auth.cookie_header.contains("SID=google_sid"));
    }

    #[test]
    fn rejects_cookie_text_without_sapisid() {
        let text = ".youtube.com\tTRUE\t/\tTRUE\t9999999999\tSID\tsid_only\n";
        assert!(matches!(
            Auth::from_cookie_text(text),
            Err(AuthError::MissingSapisid)
        ));
    }

    #[test]
    fn rejects_cookie_file_that_cannot_be_read() {
        let error = match Auth::from_cookie_file("/path/that/does/not/exist") {
            Ok(_) => panic!("missing file must fail"),
            Err(error) => error,
        };
        assert!(matches!(error, AuthError::ReadFile { .. }));
    }
}
