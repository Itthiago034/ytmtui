//! Temas de cores da interface.
//!
//! Um `Theme` centraliza as cores de destaque usadas pela UI. O usuário pode
//! alternar entre os presets em tempo real (tecla `t`) e a escolha é salva no
//! `config.json` pelo nome do tema.
//!
//! Além das cores de destaque, cada tema carrega sua própria escala de
//! neutros (`text`, `subtext`, `muted`, `border`) tingida pelo matiz do
//! destaque — assim a interface inteira muda de personalidade junto com o
//! tema, em vez de misturar cinzas genéricos do terminal.

use ratatui::style::Color;

/// Conjunto de cores que define a identidade visual do app.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Nome exibido ao usuário e salvo na config.
    pub name: &'static str,
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
    /// usuário é preservado exatamente como antes deste campo existir.
    pub surface: Color,
    /// Fundo do card selecionado na grade da tela Início. Hoje é idêntico a
    /// `highlight_bg` em todos os presets (ver teste
    /// `every_preset_keeps_selected_card_in_sync_with_highlight_bg`); campo
    /// separado para permitir divergir no futuro sem afetar as listas.
    pub selected_card: Color,
    /// Cor do badge de provedor mostrado no card selecionado da grade. Hoje
    /// é idêntico a `secondary` em todos os presets (ver teste
    /// `every_preset_keeps_provider_badge_in_sync_with_secondary`); campo
    /// separado pelo mesmo motivo de `selected_card`.
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

/// Presets disponíveis. O primeiro é o padrão.
pub const THEMES: &[Theme] = &[
    Theme {
        name: "Roxo",
        accent: Color::Rgb(187, 134, 252),
        accent_fg: Color::Black,
        secondary: Color::Rgb(3, 218, 198),
        player: Color::Rgb(187, 134, 252),
        highlight_bg: Color::Rgb(45, 40, 65),
        surface: Color::Reset,
        selected_card: Color::Rgb(45, 40, 65),
        provider_badge: Color::Rgb(3, 218, 198),
        text: Color::Rgb(236, 231, 250),
        subtext: Color::Rgb(176, 168, 200),
        muted: Color::Rgb(118, 110, 145),
        border: Color::Rgb(76, 70, 100),
    },
    Theme {
        name: "YT Vermelho",
        accent: Color::Rgb(255, 45, 70),
        accent_fg: Color::White,
        secondary: Color::Rgb(255, 150, 150),
        player: Color::Rgb(255, 45, 70),
        highlight_bg: Color::Rgb(60, 28, 32),
        surface: Color::Reset,
        selected_card: Color::Rgb(60, 28, 32),
        provider_badge: Color::Rgb(255, 150, 150),
        text: Color::Rgb(250, 235, 236),
        subtext: Color::Rgb(198, 168, 172),
        muted: Color::Rgb(140, 106, 112),
        border: Color::Rgb(98, 68, 73),
    },
    Theme {
        name: "Verde Spotify",
        accent: Color::Rgb(30, 215, 96),
        accent_fg: Color::Black,
        secondary: Color::Rgb(130, 230, 175),
        player: Color::Rgb(30, 215, 96),
        highlight_bg: Color::Rgb(24, 54, 40),
        surface: Color::Reset,
        selected_card: Color::Rgb(24, 54, 40),
        provider_badge: Color::Rgb(130, 230, 175),
        text: Color::Rgb(232, 246, 238),
        subtext: Color::Rgb(163, 192, 175),
        muted: Color::Rgb(102, 132, 114),
        border: Color::Rgb(62, 92, 75),
    },
    Theme {
        name: "Oceano",
        accent: Color::Rgb(80, 170, 255),
        accent_fg: Color::Black,
        secondary: Color::Rgb(150, 205, 255),
        player: Color::Rgb(80, 170, 255),
        highlight_bg: Color::Rgb(24, 42, 66),
        surface: Color::Reset,
        selected_card: Color::Rgb(24, 42, 66),
        provider_badge: Color::Rgb(150, 205, 255),
        text: Color::Rgb(230, 240, 250),
        subtext: Color::Rgb(160, 182, 205),
        muted: Color::Rgb(99, 124, 152),
        border: Color::Rgb(62, 86, 115),
    },
    Theme {
        name: "Âmbar",
        accent: Color::Rgb(255, 176, 59),
        accent_fg: Color::Black,
        secondary: Color::Rgb(255, 212, 145),
        player: Color::Rgb(255, 176, 59),
        highlight_bg: Color::Rgb(58, 44, 20),
        surface: Color::Reset,
        selected_card: Color::Rgb(58, 44, 20),
        provider_badge: Color::Rgb(255, 212, 145),
        text: Color::Rgb(250, 242, 230),
        subtext: Color::Rgb(201, 182, 155),
        muted: Color::Rgb(142, 123, 95),
        border: Color::Rgb(102, 87, 62),
    },
    Theme {
        name: "Rosa",
        accent: Color::Rgb(255, 110, 180),
        accent_fg: Color::Black,
        secondary: Color::Rgb(255, 185, 218),
        player: Color::Rgb(255, 110, 180),
        highlight_bg: Color::Rgb(58, 30, 48),
        surface: Color::Reset,
        selected_card: Color::Rgb(58, 30, 48),
        provider_badge: Color::Rgb(255, 185, 218),
        text: Color::Rgb(250, 235, 243),
        subtext: Color::Rgb(200, 170, 186),
        muted: Color::Rgb(142, 108, 124),
        border: Color::Rgb(101, 71, 87),
    },
];

/// Índice do tema pelo nome (case-insensitive); 0 (padrão) se não encontrado.
pub fn index_by_name(name: &str) -> usize {
    THEMES
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(name))
        .unwrap_or(0)
}

/// Retorna o tema em `index` (com wrap seguro).
pub fn get(index: usize) -> &'static Theme {
    &THEMES[index % THEMES.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    // Esta migração introduziu `selected_card`/`provider_badge` como campos
    // separados de `highlight_bg`/`secondary`, mas todo preset existente
    // deve preservar o visual pixel a pixel: os dois pares devem continuar
    // idênticos em todo `THEMES[i]`. Se este teste quebrar, o preset mudou
    // de aparência — reverta em vez de atualizar o teste.
    #[test]
    fn every_preset_keeps_selected_card_in_sync_with_highlight_bg() {
        for theme in THEMES {
            assert_eq!(
                theme.selected_card, theme.highlight_bg,
                "preset {:?} diverged selected_card from highlight_bg",
                theme.name
            );
        }
    }

    #[test]
    fn every_preset_keeps_provider_badge_in_sync_with_secondary() {
        for theme in THEMES {
            assert_eq!(
                theme.provider_badge, theme.secondary,
                "preset {:?} diverged provider_badge from secondary",
                theme.name
            );
        }
    }
}
