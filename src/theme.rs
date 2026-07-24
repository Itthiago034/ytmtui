//! Temas de cores da interface.
//!
//! Um tema é um nome mais um [`ThemeColors`] — a paleta que a UI inteira lê.
//! A separação existe por dois motivos: `ThemeColors` é `Copy`, então uma
//! função de desenho pode receber a paleta por valor sem disputar o
//! empréstimo mutável de `App`; e o nome é uma `String`, o que permite temas
//! carregados de disco em vez de só os presets embutidos.
//!
//! Cada tema carrega sua própria escala de neutros (`text`, `subtext`,
//! `muted`, `border`) tingida pelo matiz do destaque — a interface inteira
//! muda de personalidade junto com o tema, em vez de misturar cinzas
//! genéricos do terminal. Temas do usuário que omitem esses campos os têm
//! derivados por [`derive_neutrals`], seguindo a mesma proporção dos presets.

use std::path::PathBuf;

use ratatui::style::Color;
use serde::Deserialize;

/// Paleta de um tema. `Copy`: a UI passa a paleta por valor, sem manter
/// emprestado o `App` de onde ela veio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColors {
    /// Cor de destaque principal (logo, títulos, seleção da barra lateral).
    pub accent: Color,
    /// Cor do texto sobre o fundo de destaque (item selecionado).
    pub accent_fg: Color,
    /// Cor secundária (artista, subtítulos).
    pub secondary: Color,
    /// Cor do player (borda e barra de progresso).
    pub player: Color,
    /// Fundo do item selecionado nas listas.
    pub highlight_bg: Color,
    /// Fundo dos painéis (blocks). Nenhum preset define hoje um fundo
    /// próprio, então todos usam `Color::Reset` — o fundo do terminal do
    /// usuário é preservado.
    pub surface: Color,
    /// Fundo do card selecionado na grade da tela Início.
    pub selected_card: Color,
    /// Cor do badge de provedor mostrado no card selecionado da grade.
    pub provider_badge: Color,
    /// Texto principal (títulos de faixa, conteúdo em foco).
    pub text: Color,
    /// Texto de apoio (status, tempos, descrições).
    pub subtext: Color,
    /// Texto apagado (números, durações, itens inativos, dicas).
    pub muted: Color,
    /// Bordas e réguas sem foco, trilha da barra de progresso.
    pub border: Color,
}

/// Um tema nomeado.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Nome exibido ao usuário e salvo na config.
    pub name: String,
    pub colors: ThemeColors,
}

/// Um preset escrito à mão, com toda a paleta explícita.
///
/// Os seis primeiros presets foram afinados cor a cor antes de a derivação
/// existir, e a derivação **não** os reproduz (erra até 21 pontos por canal
/// nos níveis escuros). Reescrevê-los como paletas derivadas mudaria a
/// aparência de temas que os usuários já escolheram, então eles ficam
/// literais; a derivação serve para os presets novos e para temas do
/// usuário, que nunca tiveram um valor afinado à mão para preservar.
#[allow(clippy::too_many_arguments)]
fn tuned(
    name: &str,
    accent: (u8, u8, u8),
    accent_fg: Color,
    secondary: (u8, u8, u8),
    highlight_bg: (u8, u8, u8),
    text: (u8, u8, u8),
    subtext: (u8, u8, u8),
    muted: (u8, u8, u8),
    border: (u8, u8, u8),
) -> Theme {
    let rgb = |c: (u8, u8, u8)| Color::Rgb(c.0, c.1, c.2);
    let (accent, secondary, highlight_bg) = (rgb(accent), rgb(secondary), rgb(highlight_bg));
    Theme {
        name: name.to_string(),
        colors: ThemeColors {
            accent,
            accent_fg,
            secondary,
            player: accent,
            highlight_bg,
            surface: Color::Reset,
            selected_card: highlight_bg,
            provider_badge: secondary,
            text: rgb(text),
            subtext: rgb(subtext),
            muted: rgb(muted),
            border: rgb(border),
        },
    }
}

/// Um preset derivado do destaque e da cor secundária, para paletas que
/// nunca tiveram uma versão afinada à mão.
fn derived(name: &str, accent: (u8, u8, u8), secondary: (u8, u8, u8)) -> Theme {
    Theme {
        name: name.to_string(),
        colors: derive_colors(
            Color::Rgb(accent.0, accent.1, accent.2),
            Some(Color::Rgb(secondary.0, secondary.1, secondary.2)),
        ),
    }
}

/// Presets embutidos. O primeiro é o padrão.
pub fn presets() -> Vec<Theme> {
    vec![
        // Afinados à mão — não converter para `derived`.
        tuned(
            "Roxo",
            (187, 134, 252),
            Color::Black,
            (3, 218, 198),
            (45, 40, 65),
            (236, 231, 250),
            (176, 168, 200),
            (118, 110, 145),
            (76, 70, 100),
        ),
        tuned(
            "YT Vermelho",
            (255, 45, 70),
            Color::White,
            (255, 150, 150),
            (60, 28, 32),
            (250, 235, 236),
            (198, 168, 172),
            (140, 106, 112),
            (98, 68, 73),
        ),
        tuned(
            "Verde Spotify",
            (30, 215, 96),
            Color::Black,
            (130, 230, 175),
            (24, 54, 40),
            (232, 246, 238),
            (163, 192, 175),
            (102, 132, 114),
            (62, 92, 75),
        ),
        tuned(
            "Oceano",
            (80, 170, 255),
            Color::Black,
            (150, 205, 255),
            (24, 42, 66),
            (230, 240, 250),
            (160, 182, 205),
            (99, 124, 152),
            (62, 86, 115),
        ),
        tuned(
            "Âmbar",
            (255, 176, 59),
            Color::Black,
            (255, 212, 145),
            (58, 44, 20),
            (250, 242, 230),
            (201, 182, 155),
            (142, 123, 95),
            (102, 87, 62),
        ),
        tuned(
            "Rosa",
            (255, 110, 180),
            Color::Black,
            (255, 185, 218),
            (58, 30, 48),
            (250, 235, 243),
            (200, 170, 186),
            (142, 108, 124),
            (101, 71, 87),
        ),
        // Novos: derivados do destaque pela mesma escala que os temas do
        // usuário recebem.
        derived("Catppuccin Mocha", (137, 180, 250), (148, 226, 213)),
        derived("Gruvbox", (250, 189, 47), (184, 187, 38)),
        derived("Nord", (136, 192, 208), (163, 190, 140)),
        derived("Dracula", (189, 147, 249), (80, 250, 123)),
        derived("Tokyo Night", (122, 162, 247), (125, 207, 255)),
    ]
}

/// Proporções da escala de neutros, medidas nos presets originais: cada
/// nível é uma mistura entre o texto quase-branco e uma base escura tingida
/// pelo destaque.
const SUBTEXT_MIX: f32 = 0.33;
const MUTED_MIX: f32 = 0.62;
const BORDER_MIX: f32 = 0.79;
/// Quanto do destaque entra no texto quase-branco.
const TEXT_TINT: f32 = 0.15;
/// Quanto do destaque sobra na base escura da escala.
const BASE_TINT: f32 = 0.22;
/// Quanto do destaque entra no fundo do item selecionado.
const HIGHLIGHT_TINT: f32 = 0.20;

/// Deriva a escala de neutros a partir do destaque: um texto quase-branco
/// levemente tingido, e três níveis progressivamente mais escuros na direção
/// de uma base que carrega o mesmo matiz.
pub fn derive_neutrals(accent: Color) -> (Color, Color, Color, Color) {
    let white = Color::Rgb(255, 255, 255);
    let black = Color::Rgb(0, 0, 0);
    let text = mix(white, accent, TEXT_TINT);
    let base = mix(black, accent, BASE_TINT);
    (
        text,
        mix(text, base, SUBTEXT_MIX),
        mix(text, base, MUTED_MIX),
        mix(text, base, BORDER_MIX),
    )
}

/// Paleta completa derivada do destaque. `secondary` é explícita quando o
/// tema a define; sem ela, vira um destaque clareado.
fn derive_colors(accent: Color, secondary: Option<Color>) -> ThemeColors {
    let secondary = secondary.unwrap_or_else(|| mix(accent, Color::Rgb(255, 255, 255), 0.45));
    let (text, subtext, muted, border) = derive_neutrals(accent);
    let highlight_bg = mix(Color::Rgb(0, 0, 0), accent, HIGHLIGHT_TINT);
    ThemeColors {
        accent,
        accent_fg: readable_on(accent),
        secondary,
        player: accent,
        highlight_bg,
        surface: Color::Reset,
        selected_card: highlight_bg,
        provider_badge: secondary,
        text,
        subtext,
        muted,
        border,
    }
}

/// Preto ou branco, o que tiver mais contraste sobre `background`.
/// Luminância perceptual (ITU-R BT.601): o olho pesa verde muito mais que
/// azul, então uma média simples escolheria errado em destaques saturados.
fn readable_on(background: Color) -> Color {
    let Color::Rgb(r, g, b) = background else {
        return Color::White;
    };
    let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    if luminance > 140.0 {
        Color::Black
    } else {
        Color::White
    }
}

/// Interpola de `from` a `to`, com `t` saturado em `0.0..=1.0`.
///
/// Só mistura de fato quando ambos os lados são [`Color::Rgb`] — o caso de
/// todos os presets. Com qualquer outra variante (`Reset`, cores indexadas do
/// terminal) não há canal para interpolar, então a função corta na metade:
/// devolve `from` na primeira metade e `to` na segunda. Isso degrada um fade
/// contínuo para um único passo, em vez de escolher um RGB arbitrário que
/// ignoraria a paleta do terminal do usuário.
pub fn mix(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    match (from, to) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
            Color::Rgb(lerp(r1, r2), lerp(g1, g2), lerp(b1, b2))
        }
        _ if t < 0.5 => from,
        _ => to,
    }
}

// --- temas do usuário ----------------------------------------------------

/// Um tema como escrito em `~/.config/ytmtui/themes/*.toml`.
///
/// Só `name` e `accent` são obrigatórios; o resto é derivado. Um arquivo
/// pode sobrescrever qualquer campo individualmente.
#[derive(Debug, Deserialize)]
struct ThemeFile {
    name: String,
    accent: String,
    secondary: Option<String>,
    accent_fg: Option<String>,
    player: Option<String>,
    highlight_bg: Option<String>,
    selected_card: Option<String>,
    provider_badge: Option<String>,
    text: Option<String>,
    subtext: Option<String>,
    muted: Option<String>,
    border: Option<String>,
}

/// Interpreta `#rrggbb` (ou `rrggbb`). `None` para qualquer outra coisa —
/// um campo malformado cai na derivação em vez de derrubar o tema inteiro.
pub fn parse_hex(value: &str) -> Option<Color> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(Color::Rgb(byte(0)?, byte(2)?, byte(4)?))
}

impl ThemeFile {
    /// Converte para um tema completo. `None` quando o destaque — o único
    /// campo de que tudo o mais depende — não é uma cor válida.
    fn into_theme(self) -> Option<Theme> {
        let accent = parse_hex(&self.accent)?;
        let secondary = self.secondary.as_deref().and_then(parse_hex);
        let mut colors = derive_colors(accent, secondary);
        let override_with = |slot: &mut Color, raw: Option<&String>| {
            if let Some(color) = raw.map(String::as_str).and_then(parse_hex) {
                *slot = color;
            }
        };
        override_with(&mut colors.accent_fg, self.accent_fg.as_ref());
        override_with(&mut colors.player, self.player.as_ref());
        override_with(&mut colors.highlight_bg, self.highlight_bg.as_ref());
        override_with(&mut colors.selected_card, self.selected_card.as_ref());
        override_with(&mut colors.provider_badge, self.provider_badge.as_ref());
        override_with(&mut colors.text, self.text.as_ref());
        override_with(&mut colors.subtext, self.subtext.as_ref());
        override_with(&mut colors.muted, self.muted.as_ref());
        override_with(&mut colors.border, self.border.as_ref());
        Some(Theme {
            name: self.name,
            colors,
        })
    }
}

/// Diretório de temas do usuário.
fn themes_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ytmtui").join("themes"))
}

/// Todos os temas disponíveis: os presets embutidos seguidos dos temas do
/// usuário.
#[derive(Debug, Clone)]
pub struct ThemeSet {
    themes: Vec<Theme>,
    /// Arquivos que existiam mas não puderam ser lidos, para avisar o
    /// usuário sem derrubar o app.
    rejected: Vec<String>,
}

impl ThemeSet {
    /// Só os presets embutidos. Usado nos testes, que nunca devem depender
    /// do diretório de configuração da máquina onde rodam.
    pub fn builtin() -> Self {
        Self {
            themes: presets(),
            rejected: Vec::new(),
        }
    }

    /// Presets mais qualquer `*.toml` válido no diretório de temas do
    /// usuário. Arquivos ilegíveis ou malformados são recusados
    /// individualmente — nunca impedem o app de abrir.
    pub fn load() -> Self {
        match themes_dir() {
            Some(dir) => Self::load_from(&dir),
            None => Self::builtin(),
        }
    }

    /// [`Self::load`] a partir de um diretório explícito. Um diretório
    /// ausente devolve só os presets: é o caso normal de quem nunca criou
    /// um tema.
    pub fn load_from(dir: &std::path::Path) -> Self {
        let mut set = Self::builtin();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return set;
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
            .collect();
        // Ordem alfabética: `read_dir` não garante ordem, e uma lista de
        // temas que se reordena entre execuções embaralharia o índice salvo
        // na config.
        paths.sort();

        for path in paths {
            let label = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            match std::fs::read_to_string(&path)
                .ok()
                .and_then(|raw| toml::from_str::<ThemeFile>(&raw).ok())
                .and_then(ThemeFile::into_theme)
            {
                Some(theme) => set.themes.push(theme),
                None => set.rejected.push(label),
            }
        }
        set
    }

    pub fn len(&self) -> usize {
        self.themes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.themes.is_empty()
    }

    /// O tema em `index`, com wrap seguro.
    pub fn get(&self, index: usize) -> &Theme {
        &self.themes[index % self.themes.len()]
    }

    /// Índice do tema pelo nome (case-insensitive); 0 (padrão) se não
    /// encontrado — inclusive quando um tema salvo na config foi removido
    /// do disco desde a última sessão.
    pub fn index_by_name(&self, name: &str) -> usize {
        self.themes
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(name))
            .unwrap_or(0)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.themes.iter().map(|t| t.name.as_str())
    }

    /// Nomes de arquivo que foram recusados no carregamento.
    pub fn rejected(&self) -> &[String] {
        &self.rejected
    }
}

impl Default for ThemeSet {
    fn default() -> Self {
        Self::builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colors(name: &str) -> ThemeColors {
        let set = ThemeSet::builtin();
        set.get(set.index_by_name(name)).colors
    }

    // Os seis presets originais são afinados à mão e devem continuar
    // pixel a pixel como estavam. Se um destes quebrar, um preset mudou de
    // aparência para usuários que já o escolheram — reverta em vez de
    // atualizar o valor.
    #[test]
    fn the_hand_tuned_presets_keep_their_exact_palettes() {
        let roxo = colors("Roxo");
        assert_eq!(roxo.accent, Color::Rgb(187, 134, 252));
        assert_eq!(roxo.secondary, Color::Rgb(3, 218, 198));
        assert_eq!(roxo.accent_fg, Color::Black);
        assert_eq!(roxo.surface, Color::Reset);
        assert_eq!(roxo.text, Color::Rgb(236, 231, 250));
        assert_eq!(roxo.subtext, Color::Rgb(176, 168, 200));
        assert_eq!(roxo.muted, Color::Rgb(118, 110, 145));
        assert_eq!(roxo.border, Color::Rgb(76, 70, 100));
        assert_eq!(roxo.highlight_bg, Color::Rgb(45, 40, 65));

        let oceano = colors("Oceano");
        assert_eq!(oceano.accent, Color::Rgb(80, 170, 255));
        assert_eq!(oceano.border, Color::Rgb(62, 86, 115));
        assert_eq!(oceano.highlight_bg, Color::Rgb(24, 42, 66));
    }

    // A derivação existe para temas que nunca tiveram valores afinados. Ela
    // não reproduz os presets originais (erra até 21 pontos por canal nos
    // níveis escuros), e é exatamente por isso que aqueles ficam literais.
    #[test]
    fn derivation_is_close_to_but_not_a_substitute_for_hand_tuning() {
        let (text, _, _, border) = derive_neutrals(Color::Rgb(187, 134, 252));
        assert_ne!(text, Color::Rgb(236, 231, 250));
        assert_ne!(border, Color::Rgb(76, 70, 100));
        // Perto o bastante para ser uma escala plausível, ainda assim.
        let Color::Rgb(_, _, b) = border else {
            panic!("rgb")
        };
        assert!((b as i32 - 100).abs() < 20);
    }

    // Estes dois pares eram idênticos em todo preset escrito à mão; a
    // derivação deve preservar isso.
    #[test]
    fn every_preset_keeps_selected_card_in_sync_with_highlight_bg() {
        for theme in presets() {
            assert_eq!(
                theme.colors.selected_card, theme.colors.highlight_bg,
                "preset {:?} diverged selected_card from highlight_bg",
                theme.name
            );
        }
    }

    #[test]
    fn every_preset_keeps_provider_badge_in_sync_with_secondary() {
        for theme in presets() {
            assert_eq!(
                theme.colors.provider_badge, theme.colors.secondary,
                "preset {:?} diverged provider_badge from secondary",
                theme.name
            );
        }
    }

    #[test]
    fn the_neutral_scale_darkens_monotonically() {
        // text > subtext > muted > border in brightness, for every preset —
        // the hierarchy the whole UI leans on.
        let brightness = |c: Color| match c {
            Color::Rgb(r, g, b) => r as u32 + g as u32 + b as u32,
            other => panic!("expected an rgb color, got {other:?}"),
        };
        for theme in presets() {
            let c = theme.colors;
            assert!(
                brightness(c.text) > brightness(c.subtext)
                    && brightness(c.subtext) > brightness(c.muted)
                    && brightness(c.muted) > brightness(c.border),
                "preset {:?} has a non-monotonic neutral scale",
                theme.name
            );
        }
    }

    #[test]
    fn selected_text_stays_readable_on_every_accent() {
        // A light accent needs dark text on it and vice versa; getting this
        // backwards makes the selected row unreadable.
        for theme in presets() {
            let Color::Rgb(r, g, b) = theme.colors.accent else {
                panic!("presets are rgb")
            };
            let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
            let expected = if luminance > 140.0 {
                Color::Black
            } else {
                Color::White
            };
            assert_eq!(
                theme.colors.accent_fg, expected,
                "preset {:?} picked unreadable text for its accent",
                theme.name
            );
        }
    }

    #[test]
    fn names_resolve_case_insensitively_and_fall_back_to_the_default() {
        let set = ThemeSet::builtin();
        assert_eq!(set.index_by_name("roxo"), 0);
        assert_eq!(set.index_by_name("VERDE SPOTIFY"), 2);
        // A theme that was deleted from disk since the last session must
        // not leave the app themeless.
        assert_eq!(set.index_by_name("a theme that never existed"), 0);
    }

    #[test]
    fn indexing_wraps_instead_of_panicking() {
        let set = ThemeSet::builtin();
        assert_eq!(set.get(set.len()).name, set.get(0).name);
    }

    // --- mix ---

    #[test]
    fn mix_returns_the_endpoints_exactly() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(255, 128, 64);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
    }

    #[test]
    fn mix_interpolates_each_channel_independently() {
        let a = Color::Rgb(0, 100, 200);
        let b = Color::Rgb(100, 200, 0);
        assert_eq!(mix(a, b, 0.5), Color::Rgb(50, 150, 100));
    }

    #[test]
    fn mix_saturates_out_of_range_fractions() {
        let a = Color::Rgb(10, 10, 10);
        let b = Color::Rgb(20, 20, 20);
        assert_eq!(mix(a, b, -3.0), a);
        assert_eq!(mix(a, b, 7.5), b);
    }

    #[test]
    fn mix_degrades_to_a_single_step_for_non_rgb_colors() {
        // `Reset` has no channels to interpolate, so the fade becomes one
        // switch at the midpoint rather than an invented RGB value.
        let rgb = Color::Rgb(255, 255, 255);
        assert_eq!(mix(Color::Reset, rgb, 0.25), Color::Reset);
        assert_eq!(mix(Color::Reset, rgb, 0.75), rgb);
    }

    // --- user themes ---

    #[test]
    fn hex_parsing_accepts_both_spellings_and_rejects_junk() {
        assert_eq!(parse_hex("#89b4fa"), Some(Color::Rgb(137, 180, 250)));
        assert_eq!(parse_hex("89B4FA"), Some(Color::Rgb(137, 180, 250)));
        assert_eq!(parse_hex("  #89b4fa  "), Some(Color::Rgb(137, 180, 250)));
        assert_eq!(parse_hex("#89b4f"), None, "too short");
        assert_eq!(parse_hex("#89b4faa"), None, "too long");
        assert_eq!(parse_hex("#zzzzzz"), None, "not hex");
        assert_eq!(parse_hex(""), None);
    }

    #[test]
    fn a_theme_file_needs_only_a_name_and_an_accent() {
        let file: ThemeFile = toml::from_str(
            r##"
            name = "Minimal"
            accent = "#89b4fa"
            "##,
        )
        .expect("valid toml");
        let theme = file.into_theme().expect("a valid accent is enough");
        assert_eq!(theme.name, "Minimal");
        assert_eq!(theme.colors.accent, Color::Rgb(137, 180, 250));
        // Everything else was derived rather than left unset.
        assert_ne!(theme.colors.text, theme.colors.border);
    }

    #[test]
    fn a_theme_file_can_override_individual_colors() {
        let file: ThemeFile = toml::from_str(
            r##"
            name = "Override"
            accent = "#89b4fa"
            border = "#ff0000"
            "##,
        )
        .expect("valid toml");
        let theme = file.into_theme().unwrap();
        assert_eq!(theme.colors.border, Color::Rgb(255, 0, 0));
    }

    #[test]
    fn a_malformed_override_falls_back_to_the_derived_color() {
        // One bad field must not cost the user their whole theme.
        let file: ThemeFile = toml::from_str(
            r##"
            name = "Partly broken"
            accent = "#89b4fa"
            border = "not a color"
            "##,
        )
        .expect("valid toml");
        let theme = file.into_theme().expect("the accent is still valid");
        let (_, _, _, derived_border) = derive_neutrals(Color::Rgb(137, 180, 250));
        assert_eq!(theme.colors.border, derived_border);
    }

    #[test]
    fn user_themes_are_appended_after_the_presets_in_a_stable_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Written out of order on purpose: `read_dir` gives no ordering
        // guarantee, and a list that reshuffles between runs would scramble
        // the theme index saved in the config.
        std::fs::write(
            dir.path().join("zebra.toml"),
            "name = \"Zebra\"\naccent = \"#ffffff\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("apple.toml"),
            "name = \"Apple\"\naccent = \"#ff0000\"\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("notes.txt"), "not a theme").unwrap();

        let set = ThemeSet::load_from(dir.path());

        let names: Vec<&str> = set.names().collect();
        assert_eq!(
            &names[..6],
            &presets()[..6]
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>()[..]
        );
        assert_eq!(
            &names[names.len() - 2..],
            &["Apple", "Zebra"],
            "user themes come last, alphabetically by filename"
        );
        assert!(set.rejected().is_empty(), "a .txt file is not a rejection");
    }

    #[test]
    fn a_missing_themes_directory_is_not_an_error() {
        // The normal case for anyone who never wrote a custom theme.
        let set = ThemeSet::load_from(std::path::Path::new("/nonexistent/ytmtui/themes"));
        assert_eq!(set.len(), presets().len());
        assert!(set.rejected().is_empty());
    }

    #[test]
    fn a_broken_theme_file_is_reported_without_costing_the_others() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("good.toml"),
            "name = \"Good\"\naccent = \"#00ff00\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("broken.toml"),
            "this is not toml at all {{{",
        )
        .unwrap();

        let set = ThemeSet::load_from(dir.path());

        assert!(set.names().any(|n| n == "Good"), "the valid theme loaded");
        assert_eq!(set.rejected(), ["broken.toml"]);
    }

    #[test]
    fn a_theme_without_a_usable_accent_is_rejected() {
        // Everything derives from the accent, so there is nothing to build.
        let file: ThemeFile = toml::from_str(
            r##"
            name = "Broken"
            accent = "nope"
            "##,
        )
        .expect("valid toml");
        assert!(file.into_theme().is_none());
    }
}
