//! Temas de cores da interface.
//!
//! Um `Theme` centraliza as cores de destaque usadas pela UI. O usuário pode
//! alternar entre os presets em tempo real (tecla `t`) e a escolha é salva no
//! `config.json` pelo nome do tema.

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
    },
    Theme {
        name: "YT Vermelho",
        accent: Color::Rgb(255, 45, 70),
        accent_fg: Color::White,
        secondary: Color::Rgb(255, 150, 150),
        player: Color::Rgb(255, 45, 70),
        highlight_bg: Color::Rgb(60, 28, 32),
    },
    Theme {
        name: "Verde Spotify",
        accent: Color::Rgb(30, 215, 96),
        accent_fg: Color::Black,
        secondary: Color::Rgb(130, 230, 175),
        player: Color::Rgb(30, 215, 96),
        highlight_bg: Color::Rgb(24, 54, 40),
    },
    Theme {
        name: "Oceano",
        accent: Color::Rgb(80, 170, 255),
        accent_fg: Color::Black,
        secondary: Color::Rgb(150, 205, 255),
        player: Color::Rgb(80, 170, 255),
        highlight_bg: Color::Rgb(24, 42, 66),
    },
    Theme {
        name: "Âmbar",
        accent: Color::Rgb(255, 176, 59),
        accent_fg: Color::Black,
        secondary: Color::Rgb(255, 212, 145),
        player: Color::Rgb(255, 176, 59),
        highlight_bg: Color::Rgb(58, 44, 20),
    },
    Theme {
        name: "Rosa",
        accent: Color::Rgb(255, 110, 180),
        accent_fg: Color::Black,
        secondary: Color::Rgb(255, 185, 218),
        player: Color::Rgb(255, 110, 180),
        highlight_bg: Color::Rgb(58, 30, 48),
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
