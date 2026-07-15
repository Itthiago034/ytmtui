//! Resolução de áudio do YouTube Music: baixa a melhor faixa de áudio com o
//! `yt-dlp` para o cache local e a prepara para reprodução. É a
//! implementação de `MusicProvider::resolve_playable` deste provedor — o
//! resto do app só vê "faixa entra, arquivo tocável sai".

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::Command;

use crate::player::{evict_cache, find_cached, is_partial, prepare_for_playback, temp_dir};

/// URL de reprodução no YouTube Music para uma faixa.
fn watch_url(video_id: &str) -> String {
    format!("https://music.youtube.com/watch?v={video_id}")
}

/// Resolve e baixa o áudio de uma faixa para um arquivo temporário, usando
/// `yt-dlp`. Retorna o caminho do arquivo pronto para tocar.
///
/// Otimizações:
/// - **Sem transcodificação**: baixa preferencialmente o formato `m4a` (AAC),
///   que o `symphonia` decodifica nativamente, evitando a etapa lenta de
///   conversão para mp3 via ffmpeg.
/// - **Cache**: se o áudio da faixa já foi baixado nesta sessão, reutiliza o
///   arquivo em vez de baixar de novo (replay instantâneo / prefetch).
///
/// Esta função é bloqueante e deve ser executada em uma task dedicada.
/// `cookies` é o caminho opcional para um arquivo de cookies (contorna a
/// verificação anti-bot do YouTube em alguns ambientes/IPs).
pub fn download_audio(video_id: &str, cookies: Option<&str>) -> Result<PathBuf> {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir)?;

    // Cache: reutiliza o arquivo já baixado para esta faixa.
    if !video_id.is_empty() {
        if let Some(cached) = find_cached(&dir, video_id) {
            return Ok(cached);
        }
    }

    let out_template = dir.join("%(id)s.%(ext)s");

    let mut cmd = Command::new("yt-dlp");
    cmd.arg("--no-playlist")
        .arg("--quiet")
        .arg("--no-warnings")
        // Prefere m4a/AAC (decodificável direto pelo symphonia), sem re-encode.
        .arg("-f")
        .arg("bestaudio[ext=m4a]/bestaudio")
        // Usa deno como runtime JS e baixa o solver de desafios quando preciso.
        .arg("--js-runtimes")
        .arg("deno")
        .arg("--remote-components")
        .arg("ejs:github")
        .arg("-o")
        .arg(&out_template)
        .arg("--print")
        .arg("after_move:filepath")
        .arg(watch_url(video_id));

    if let Some(c) = cookies {
        cmd.arg("--cookies").arg(c);
    }

    let output = cmd
        .output()
        .map_err(|e| anyhow!("não foi possível executar o yt-dlp ({e}). Ele está instalado?"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "yt-dlp falhou: {}",
            err.lines().last().unwrap_or("erro desconhecido")
        ));
    }

    // O caminho final é impresso na última linha do stdout.
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(path) = stdout.lines().rev().find(|l| !l.trim().is_empty()) {
        let p = PathBuf::from(path.trim());
        if p.exists() {
            let ready = prepare_for_playback(p);
            evict_cache(&dir, &ready);
            return Ok(ready);
        }
    }

    // Fallback: o `--print` não deu um caminho utilizável, mas o arquivo da
    // faixa pode ter chegado ao disco (com qualquer extensão) — localiza
    // pelo stem, que é o próprio video_id. Nunca "adivinhar" pelo arquivo
    // mais recente do diretório: um prefetch concorrente de outra faixa
    // pode ser o mais novo, e a música errada tocaria.
    if !video_id.is_empty() {
        if let Some(entry) = std::fs::read_dir(&dir)?.flatten().find(|e| {
            let path = e.path();
            path.is_file()
                && !is_partial(&path)
                && path.file_stem().and_then(|s| s.to_str()) == Some(video_id)
        }) {
            let ready = prepare_for_playback(entry.path());
            evict_cache(&dir, &ready);
            return Ok(ready);
        }
    }
    Err(anyhow!("arquivo de áudio não encontrado após o download"))
}
