//! Player de áudio.
//!
//! A reprodução é feita com a crate `rodio`, rodando em uma thread dedicada
//! (a `OutputStream` do rodio não é `Send`). A resolução do stream de áudio a
//! partir do YouTube Music é feita com o `yt-dlp`, que baixa a melhor faixa de
//! áudio para um arquivo temporário reproduzido em seguida.

use anyhow::{anyhow, Result};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

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

/// Tempo máximo para o download de áudio via `yt-dlp` antes de matar o
/// processo e desistir, evitando travar a reprodução indefinidamente em caso
/// de instabilidade de rede.
const YT_DLP_TIMEOUT: Duration = Duration::from_secs(120);

/// Tempo máximo para o remux/transcode via `ffmpeg`.
const FFMPEG_TIMEOUT: Duration = Duration::from_secs(60);

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

/// Trava o estado compartilhado, recuperando de um mutex envenenado em vez de
/// propagar o panic: um panic isolado enquanto a trava está ativa não deveria
/// derrubar todas as chamadas seguintes de reprodução.
fn lock_state(state: &Mutex<SharedState>) -> MutexGuard<'_, SharedState> {
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
    ///
    /// Nunca falha: se o SO não conseguir criar a thread de áudio, o handle
    /// ainda é construído (comandos enviados a ele simplesmente não terão
    /// efeito, já que a ponta receptora foi descartada junto com a thread),
    /// e a mensagem de aviso retornada explica o que aconteceu para que o
    /// chamador possa exibi-la em vez de derrubar o app inteiro.
    pub fn new() -> (Self, Option<String>) {
        let (tx, rx) = mpsc::channel::<Cmd>();
        let (sample_tx, sample_rx) = mpsc::sync_channel::<SampleChunk>(SAMPLE_CHANNEL_CAPACITY);
        let state = Arc::new(Mutex::new(SharedState::default()));
        let state_thread = state.clone();

        let warning = std::thread::Builder::new()
            .name(AUDIO_THREAD_NAME.to_string())
            .spawn(move || {
                audio_thread(rx, state_thread, sample_tx);
            })
            .err()
            .map(|e| format!("Não foi possível iniciar o áudio: {e}"));

        (
            Self {
                tx,
                state,
                volume: 0.8,
                paused: false,
                sample_rx,
            },
            warning,
        )
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
        let target = self.position() + Duration::from_secs(secs);
        {
            let mut s = lock_state(&self.state);
            s.position = target;
        }
        let _ = self.tx.send(Cmd::Seek(target));
    }

    /// Retrocede `secs` segundos na faixa atual.
    pub fn seek_backward(&mut self, secs: u64) {
        let target = self.position().saturating_sub(Duration::from_secs(secs));
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

/// Procura no cache um arquivo de áudio já pronto para tocar para o `video_id`.
fn find_cached(dir: &std::path::Path, video_id: &str) -> Option<PathBuf> {
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
fn prepare_for_playback(src: PathBuf) -> PathBuf {
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
    let mut copy_cmd = Command::new("ffmpeg");
    copy_cmd
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(&src)
        .args(["-vn", "-c:a", "copy", "-f", "adts"])
        .arg(&aac);
    let copied = status_with_timeout(&mut copy_cmd, FFMPEG_TIMEOUT);
    if matches!(copied, Ok(s) if s.success()) && file_is_non_empty(&aac) {
        let _ = std::fs::remove_file(&src);
        return aac;
    }
    let _ = std::fs::remove_file(&aac);

    // 2) Fallback: transcodifica para mp3 (cobre opus/webm).
    let mp3 = src.with_extension("mp3");
    let mut encode_cmd = Command::new("ffmpeg");
    encode_cmd
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(&src)
        .args(["-vn", "-acodec", "libmp3lame", "-q:a", "2"])
        .arg(&mp3);
    let encoded = status_with_timeout(&mut encode_cmd, FFMPEG_TIMEOUT);
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

/// Lê um stream até o fim em uma thread separada, para não travar a leitura
/// dos pipes de stdout/stderr enquanto o processo continua rodando.
fn read_to_end_in_thread<R: Read + Send + 'static>(mut r: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = r.read_to_end(&mut buf);
        buf
    })
}

/// Como `Command::output()`, mas mata o processo e retorna erro de timeout
/// se ele não terminar dentro de `timeout` — evita travar a reprodução
/// indefinidamente quando `yt-dlp`/`ffmpeg` ficam presos numa chamada de rede.
fn output_with_timeout(cmd: &mut Command, timeout: Duration) -> std::io::Result<Output> {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let stdout_handle = child.stdout.take().map(read_to_end_in_thread);
    let stderr_handle = child.stderr.take().map(read_to_end_in_thread);

    let status = wait_with_timeout(&mut child, timeout)?;

    let stdout = stdout_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    let stderr = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

/// Como `Command::status()`, mas com o mesmo limite de tempo de
/// [`output_with_timeout`].
fn status_with_timeout(cmd: &mut Command, timeout: Duration) -> std::io::Result<ExitStatus> {
    let mut child = cmd.spawn()?;
    wait_with_timeout(&mut child, timeout)
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> std::io::Result<ExitStatus> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("comando excedeu o tempo limite de {timeout:?}"),
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Resolve e baixa o áudio de uma faixa do YouTube Music para um arquivo
/// temporário, usando `yt-dlp`. Retorna o caminho do arquivo baixado.
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
pub fn download_audio(watch_url: &str, video_id: &str, cookies: Option<&str>) -> Result<PathBuf> {
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
        .arg(watch_url);

    if let Some(c) = cookies {
        cmd.arg("--cookies").arg(c);
    }

    let output = output_with_timeout(&mut cmd, YT_DLP_TIMEOUT)
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
            return Ok(prepare_for_playback(p));
        }
    }

    // Fallback 1: procura no cache pelo id da faixa.
    if !video_id.is_empty() {
        if let Some(cached) = find_cached(&dir, video_id) {
            return Ok(cached);
        }
    }

    // Fallback 2: arquivo mais recente no diretório.
    let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        let is_partial = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e == "part" || e == "ytdl")
            .unwrap_or(false);
        if path.is_file() && !is_partial {
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if newest.as_ref().map(|(_, t)| modified > *t).unwrap_or(true) {
                        newest = Some((path, modified));
                    }
                }
            }
        }
    }
    newest
        .map(|(p, _)| prepare_for_playback(p))
        .ok_or_else(|| anyhow!("arquivo de áudio não encontrado após o download"))
}
