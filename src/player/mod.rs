//! Player de áudio.
//!
//! A reprodução é feita com a crate `rodio`, rodando em uma thread dedicada
//! (a `OutputStream` do rodio não é `Send`). A resolução do stream de áudio
//! fica a cargo de cada provedor (`MusicProvider::resolve_playable`); este
//! módulo mantém apenas o playback e os utilitários de cache/preparo de
//! arquivos locais que qualquer provedor pode reutilizar.

use anyhow::{anyhow, Result};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::{Decoder, OutputStream, Sink};

mod tap;
use tap::SpectrumTap;

use crate::visualizer::SampleChunk;

/// Capacity of the sample channel: a handful of ~1024-sample chunks is
/// plenty of slack for a ~60-80ms UI tick without unbounded growth.
const SAMPLE_CHANNEL_CAPACITY: usize = 8;

/// Nome da thread de áudio. O hook de panic em `main.rs` usa esse nome para
/// ignorar panics capturados aqui (evita bagunçar o terminal em modo raw).
pub const AUDIO_THREAD_NAME: &str = "ytmtui-audio";

/// Verifica se um binário externo está disponível no `PATH`.
///
/// Considera o comando presente se conseguir iniciá-lo (qualquer código de
/// saída); só o trata como ausente quando o SO reporta `NotFound`.
fn command_exists(bin: &str) -> bool {
    match Command::new(bin)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(_) => true,
        Err(e) => e.kind() != std::io::ErrorKind::NotFound,
    }
}

/// Lista as ferramentas externas ausentes como pares `(nome, essencial)`.
///
/// - `yt-dlp` e `ffmpeg` são essenciais (sem eles a reprodução falha ou trava).
/// - `deno` é opcional: só é usado por alguns desafios de JS do `yt-dlp`.
pub fn missing_dependencies() -> Vec<(&'static str, bool)> {
    [("yt-dlp", true), ("ffmpeg", true), ("deno", false)]
        .into_iter()
        .filter(|(bin, _)| !command_exists(bin))
        .collect()
}

/// Comandos enviados para a thread de áudio.
enum Cmd {
    /// Reproduz o arquivo indicado.
    Play(PathBuf),
    Pause,
    Resume,
    Stop,
    SetVolume(f32),
    /// Salta para uma posição absoluta na faixa atual.
    Seek(Duration),
}

/// Estado compartilhado entre a thread de áudio e a interface.
#[derive(Default)]
pub struct SharedState {
    /// Posição atual da faixa em execução.
    pub position: Duration,
    /// Indica que a faixa atual terminou naturalmente.
    pub finished: bool,
    /// Há uma faixa carregada/tocando.
    pub active: bool,
}

/// Trava o estado compartilhado recuperando o guard mesmo se o mutex estiver
/// *poisoned*. Um pânico de um dos lados segurando o lock não deve derrubar
/// app + áudio em cascata: os campos de `SharedState` são valores simples
/// (posição e flags), e prosseguir com um valor possivelmente defasado é
/// estritamente melhor que propagar o pânico por todo `lock()` restante.
fn lock_state(state: &Mutex<SharedState>) -> std::sync::MutexGuard<'_, SharedState> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Handle público do player usado pela aplicação.
pub struct AudioPlayer {
    tx: Sender<Cmd>,
    state: Arc<Mutex<SharedState>>,
    volume: f32,
    paused: bool,
    /// Batches of decoded samples tapped from playback, for the Home
    /// screen's spectrum visualizer.
    sample_rx: Receiver<SampleChunk>,
}

impl AudioPlayer {
    /// Inicializa a thread de áudio e retorna o handle.
    pub fn new() -> Result<Self> {
        let (tx, rx) = mpsc::channel::<Cmd>();
        let (sample_tx, sample_rx) = mpsc::sync_channel::<SampleChunk>(SAMPLE_CHANNEL_CAPACITY);
        let state = Arc::new(Mutex::new(SharedState::default()));
        let state_thread = state.clone();

        std::thread::Builder::new()
            .name(AUDIO_THREAD_NAME.to_string())
            .spawn(move || {
                audio_thread(rx, state_thread, sample_tx);
            })
            .expect("falha ao iniciar a thread de áudio");

        Ok(Self {
            tx,
            state,
            volume: 0.8,
            paused: false,
            sample_rx,
        })
    }

    /// Drena os lotes de amostras decodificadas acumulados desde a última
    /// chamada, para alimentar o analisador de espectro (tela Home).
    pub fn drain_sample_chunks(&self) -> Vec<SampleChunk> {
        let mut chunks = Vec::new();
        while let Ok(chunk) = self.sample_rx.try_recv() {
            chunks.push(chunk);
        }
        chunks
    }

    /// Carrega e reproduz um arquivo local (já baixado).
    pub fn play_file(&mut self, path: PathBuf) {
        self.paused = false;
        {
            let mut s = lock_state(&self.state);
            s.finished = false;
            s.active = true;
            s.position = Duration::ZERO;
        }
        // Discard any sample chunks still queued from the previous track so
        // the visualizer doesn't briefly blend the old track's spectrum into
        // the new one.
        while self.sample_rx.try_recv().is_ok() {}
        let _ = self.tx.send(Cmd::Play(path));
        let _ = self.tx.send(Cmd::SetVolume(self.volume));
    }

    /// Alterna entre pausar e retomar.
    pub fn toggle_pause(&mut self) {
        if self.paused {
            self.paused = false;
            let _ = self.tx.send(Cmd::Resume);
        } else {
            self.paused = true;
            let _ = self.tx.send(Cmd::Pause);
        }
    }

    /// Interrompe a reprodução.
    pub fn stop(&mut self) {
        self.paused = false;
        {
            let mut s = lock_state(&self.state);
            s.active = false;
            s.finished = false;
            s.position = Duration::ZERO;
        }
        let _ = self.tx.send(Cmd::Stop);
    }

    /// Aumenta o volume (0.0 a 1.0).
    pub fn volume_up(&mut self) {
        self.volume = (self.volume + 0.05).min(1.0);
        let _ = self.tx.send(Cmd::SetVolume(self.volume));
    }

    /// Diminui o volume.
    pub fn volume_down(&mut self) {
        self.volume = (self.volume - 0.05).max(0.0);
        let _ = self.tx.send(Cmd::SetVolume(self.volume));
    }

    /// Define o volume diretamente (0.0 a 1.0). Usado ao carregar a config.
    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        let _ = self.tx.send(Cmd::SetVolume(self.volume));
    }

    /// Avança `secs` segundos na faixa atual.
    pub fn seek_forward(&mut self, secs: u64) {
        self.seek_to(self.position() + Duration::from_secs(secs));
    }

    /// Retrocede `secs` segundos na faixa atual.
    pub fn seek_backward(&mut self, secs: u64) {
        self.seek_to(self.position().saturating_sub(Duration::from_secs(secs)));
    }

    /// Salta para uma posição absoluta na faixa atual (seek do MPRIS).
    pub fn seek_to(&mut self, target: Duration) {
        {
            let mut s = lock_state(&self.state);
            s.position = target;
        }
        let _ = self.tx.send(Cmd::Seek(target));
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Posição atual da faixa.
    pub fn position(&self) -> Duration {
        lock_state(&self.state).position
    }

    /// Retorna `true` se a faixa terminou (e reseta a flag).
    pub fn take_finished(&self) -> bool {
        let mut s = lock_state(&self.state);
        if s.finished {
            s.finished = false;
            true
        } else {
            false
        }
    }
}

/// Loop da thread de áudio: mantém a `OutputStream` viva e processa comandos.
fn audio_thread(
    rx: Receiver<Cmd>,
    state: Arc<Mutex<SharedState>>,
    sample_tx: SyncSender<SampleChunk>,
) {
    // Cria o dispositivo de saída. Se falhar (sem áudio no sistema), a thread
    // simplesmente encerra silenciosamente.
    let (_stream, handle) = match OutputStream::try_default() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut sink: Option<Sink> = None;
    let mut volume = 0.8f32;

    loop {
        // Aguarda comandos por até 150ms para poder atualizar a posição.
        match rx.recv_timeout(Duration::from_millis(150)) {
            Ok(Cmd::Play(path)) => {
                // Descarta a faixa anterior.
                if let Some(old) = sink.take() {
                    old.stop();
                }
                // O decoder do symphonia pode entrar em pânico ao inicializar
                // certos arquivos; captura para não derrubar a thread/o app.
                let decoded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    File::open(&path)
                        .map_err(anyhow::Error::from)
                        .and_then(|f| Decoder::new(BufReader::new(f)).map_err(|e| anyhow!(e)))
                }))
                .unwrap_or_else(|_| Err(anyhow!("falha ao decodificar o áudio")));
                match decoded.and_then(|source| {
                    Sink::try_new(&handle)
                        .map_err(|e| anyhow!(e))
                        .map(|s| (source, s))
                }) {
                    Ok((source, new_sink)) => {
                        new_sink.set_volume(volume);
                        let tapped = SpectrumTap::new(source, sample_tx.clone());
                        new_sink.append(tapped);
                        new_sink.play();
                        sink = Some(new_sink);
                    }
                    Err(_) => {
                        // Decode failure or no audio output device available:
                        // mark the track as finished so the app advances or
                        // surfaces an error instead of appearing stuck on
                        // "playing" forever.
                        let mut s = lock_state(&state);
                        s.active = false;
                        s.finished = true;
                    }
                }
            }
            Ok(Cmd::Pause) => {
                if let Some(s) = &sink {
                    s.pause();
                }
            }
            Ok(Cmd::Resume) => {
                if let Some(s) = &sink {
                    s.play();
                }
            }
            Ok(Cmd::Stop) => {
                if let Some(s) = sink.take() {
                    s.stop();
                }
            }
            Ok(Cmd::SetVolume(v)) => {
                volume = v;
                if let Some(s) = &sink {
                    s.set_volume(v);
                }
            }
            Ok(Cmd::Seek(pos)) => {
                if let Some(s) = &sink {
                    // `try_seek` pode falhar para alguns formatos; ignoramos o erro.
                    if s.try_seek(pos).is_ok() {
                        let mut st = lock_state(&state);
                        st.position = pos;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        // Atualiza posição e detecta fim da faixa.
        if let Some(s) = &sink {
            let mut st = lock_state(&state);
            st.position = s.get_pos();
            if st.active && s.empty() {
                st.finished = true;
                st.active = false;
            }
        }
    }
}

/// Diretório temporário onde os áudios baixados são armazenados.
pub fn temp_dir() -> PathBuf {
    std::env::temp_dir().join("ytmtui")
}

/// Remove o diretório temporário de áudios. Chamado ao encerrar o app.
pub fn cleanup_temp_dir() {
    let _ = std::fs::remove_dir_all(temp_dir());
}

/// Extensões que o decoder (`rodio`/`symphonia`) reproduz de forma confiável.
/// O container `m4a`/`mp4` do YouTube dispara um bug de *seek* na inicialização
/// do symphonia (rodio 0.20), então é sempre remuxado antes de tocar.
fn is_playable_ext(ext: &str) -> bool {
    matches!(ext, "aac" | "mp3" | "ogg" | "oga" | "flac" | "wav")
}

/// Extensões de arquivos parciais/incompletos do yt-dlp (nunca reproduzir,
/// nunca remover: pode haver um download em andamento usando-os).
pub(crate) fn is_partial(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("part" | "ytdl")
    )
}

/// Limite de áudios mantidos no cache da sessão. O diretório fica no tmp do
/// sistema — tmpfs (RAM) em várias distros — então uma sessão longa de rádio
/// não pode acumular faixas sem limite.
const MAX_CACHED_TRACKS: usize = 24;

/// Remove os áudios menos recentes quando o cache passa de
/// [`MAX_CACHED_TRACKS`], sem tocar em `keep` (a faixa recém-resolvida) nem
/// em downloads parciais em andamento. Em Linux, apagar um arquivo ainda
/// aberto pelo player é seguro (o descritor continua válido); noutros SOs a
/// remoção falha e é ignorada.
pub(crate) fn evict_cache(dir: &std::path::Path, keep: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && !is_partial(p) && p != keep)
        .filter_map(|p| {
            let modified = std::fs::metadata(&p).ok()?.modified().ok()?;
            Some((p, modified))
        })
        .collect();
    // `keep` conta como um dos MAX_CACHED_TRACKS.
    if files.len() < MAX_CACHED_TRACKS {
        return;
    }
    files.sort_by_key(|(_, modified)| *modified);
    let excess = files.len() + 1 - MAX_CACHED_TRACKS;
    for (path, _) in files.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
}

/// Procura no cache um arquivo de áudio já pronto para tocar para o `video_id`.
pub(crate) fn find_cached(dir: &std::path::Path, video_id: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        // O arquivo é nomeado "<video_id>.<ext>"; compara o "stem".
        if path.file_stem().and_then(|s| s.to_str()) == Some(video_id) {
            let playable = path
                .extension()
                .and_then(|e| e.to_str())
                .map(is_playable_ext)
                .unwrap_or(false);
            if playable && path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

/// Prepara o arquivo baixado para reprodução. Como o symphonia entra em pânico
/// ao inicializar o decoder de alguns `m4a`/`webm` do YouTube (erro de seek),
/// remuxamos o áudio para um stream que ele decodifica sem problemas.
///
/// Estratégia (rápida primeiro):
/// 1. Se já for um formato reproduzível (`aac`, `mp3`, ...), usa direto.
/// 2. `ffmpeg -c:a copy -f adts` → AAC cru (`.aac`): apenas remux, **sem
///    re-encode**, então é praticamente instantâneo. Cobre o caso do `m4a`.
/// 3. Se a cópia falhar (ex.: áudio `opus`), transcodifica para `mp3`.
/// 4. Sem `ffmpeg` disponível, devolve o arquivo original (o player captura
///    eventual falha de decodificação sem derrubar o app).
pub(crate) fn prepare_for_playback(src: PathBuf) -> PathBuf {
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if is_playable_ext(&ext) {
        return src;
    }

    // 1) Remux sem re-encode para ADTS (rápido).
    let aac = src.with_extension("aac");
    let copied = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(&src)
        .args(["-vn", "-c:a", "copy", "-f", "adts"])
        .arg(&aac)
        .status();
    if matches!(copied, Ok(s) if s.success()) && file_is_non_empty(&aac) {
        let _ = std::fs::remove_file(&src);
        return aac;
    }
    let _ = std::fs::remove_file(&aac);

    // 2) Fallback: transcodifica para mp3 (cobre opus/webm).
    let mp3 = src.with_extension("mp3");
    let encoded = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(&src)
        .args(["-vn", "-acodec", "libmp3lame", "-q:a", "2"])
        .arg(&mp3)
        .status();
    if matches!(encoded, Ok(s) if s.success()) && file_is_non_empty(&mp3) {
        let _ = std::fs::remove_file(&src);
        return mp3;
    }
    let _ = std::fs::remove_file(&mp3);

    // 3) Sem ffmpeg: devolve o original (decodificação protegida por panic).
    src
}

fn file_is_non_empty(p: &std::path::Path) -> bool {
    std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn lock_state_recovers_from_a_poisoned_mutex() {
        let state = Arc::new(Mutex::new(SharedState::default()));
        let poisoner = Arc::clone(&state);
        // Um pânico segurando o lock envenena o mutex — exatamente o cenário
        // em que `lock().unwrap()` derrubaria todos os outros acessos.
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("envenena o mutex de propósito");
        })
        .join();
        assert!(state.lock().is_err(), "o mutex deve estar poisoned agora");

        // `lock_state` recupera o guard em vez de propagar o pânico.
        let mut guard = lock_state(&state);
        guard.position = Duration::from_secs(5);
        guard.finished = true;
        assert_eq!(guard.position, Duration::from_secs(5));
        assert!(guard.finished);
    }

    #[test]
    fn volume_is_clamped_to_the_unit_range() {
        let mut player = AudioPlayer::new().expect("cria o player");
        player.set_volume(2.5);
        assert_eq!(player.volume(), 1.0, "acima de 1.0 satura em 1.0");
        player.set_volume(-1.0);
        assert_eq!(player.volume(), 0.0, "abaixo de 0.0 satura em 0.0");
        player.volume_up();
        assert!((player.volume() - 0.05).abs() < 1e-6);
        player.volume_down();
        assert_eq!(player.volume(), 0.0, "não desce abaixo de 0.0");
    }

    #[test]
    fn playable_and_partial_extensions_are_classified() {
        for ext in ["aac", "mp3", "ogg", "oga", "flac", "wav"] {
            assert!(is_playable_ext(ext), "{ext} deveria ser reproduzível");
        }
        for ext in ["m4a", "mp4", "webm", "opus", "part"] {
            assert!(!is_playable_ext(ext), "{ext} não é reproduzível direto");
        }
        assert!(is_partial(std::path::Path::new("x.part")));
        assert!(is_partial(std::path::Path::new("x.ytdl")));
        assert!(!is_partial(std::path::Path::new("x.mp3")));
    }

    #[test]
    fn find_cached_matches_by_video_id_and_skips_unplayable() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("abc123.mp3"), b"x").unwrap();
        fs::write(dir.path().join("other.mp3"), b"x").unwrap();
        // Mesmo id, mas container que o symphonia recusa: não deve casar.
        fs::write(dir.path().join("zzz.m4a"), b"x").unwrap();

        let hit = find_cached(dir.path(), "abc123").expect("acha o mp3");
        assert_eq!(hit.file_name().unwrap(), "abc123.mp3");
        assert!(find_cached(dir.path(), "zzz").is_none(), "m4a não conta");
        assert!(find_cached(dir.path(), "missing").is_none());
    }

    #[test]
    fn evict_cache_prunes_to_the_limit_and_keeps_the_current_track() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Cria MAX+5 arquivos; `keep` é a faixa recém-resolvida.
        let total = MAX_CACHED_TRACKS + 5;
        let mut paths = Vec::new();
        for i in 0..total {
            let p = dir.path().join(format!("track{i:03}.mp3"));
            fs::write(&p, b"x").unwrap();
            paths.push(p);
        }
        let keep = paths.last().unwrap().clone();
        evict_cache(dir.path(), &keep);

        // Não asseguramos *qual* saiu (depende de mtime, que não controlamos
        // de forma portátil aqui): o invariante é o tamanho e a preservação
        // da faixa atual.
        let left = fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(left, MAX_CACHED_TRACKS, "cache podado até o limite");
        assert!(keep.exists(), "a faixa atual nunca é removida");
    }

    #[test]
    fn evict_cache_never_deletes_partial_downloads() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Um download em andamento (.part) mais lixo suficiente para exceder.
        let partial = dir.path().join("downloading.part");
        fs::write(&partial, b"x").unwrap();
        for i in 0..(MAX_CACHED_TRACKS + 3) {
            fs::write(dir.path().join(format!("t{i:03}.mp3")), b"x").unwrap();
        }
        evict_cache(dir.path(), std::path::Path::new("/nonexistent"));
        assert!(partial.exists(), "downloads parciais nunca são removidos");
    }
}
