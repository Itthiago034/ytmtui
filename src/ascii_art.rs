//! Conversão de capas (imagens) em arte colorida para o terminal.
//!
//! Usamos o caractere de meio-bloco superior `▀`: a cor de frente representa o
//! pixel de cima e a cor de fundo o pixel de baixo. Assim cada célula do
//! terminal exibe dois pixels verticais, dobrando a resolução vertical.

use image::GenericImageView;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// Converte os bytes de uma imagem em linhas coloridas para o Ratatui.
///
/// `cols` = largura em células; `rows` = altura em células (cada célula
/// representa 2 pixels na vertical).
pub fn image_to_lines(bytes: &[u8], cols: u16, rows: u16) -> Option<Vec<Line<'static>>> {
    let img = image::load_from_memory(bytes).ok()?;
    let cols = cols.max(1) as u32;
    let rows = rows.max(1) as u32;

    // Redimensiona para (cols x rows*2), preservando aproximadamente o quadrado.
    let resized = img.resize_exact(cols, rows * 2, image::imageops::FilterType::Triangle);

    let mut lines = Vec::with_capacity(rows as usize);
    for y in 0..rows {
        let mut spans = Vec::with_capacity(cols as usize);
        for x in 0..cols {
            let top = resized.get_pixel(x, y * 2);
            let bottom = resized.get_pixel(x, y * 2 + 1);
            let fg = Color::Rgb(top[0], top[1], top[2]);
            let bg = Color::Rgb(bottom[0], bottom[1], bottom[2]);
            spans.push(Span::styled("▀", Style::default().fg(fg).bg(bg)));
        }
        lines.push(Line::from(spans));
    }
    Some(lines)
}

/// Arte de reserva (placeholder) exibida quando não há capa disponível.
/// `accent` colore as notas musicais (segue o tema ativo).
pub fn placeholder(rows: u16, accent: Color) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for i in 0..rows {
        if i == rows / 2 {
            lines.push(Line::from(Span::styled(
                "        ♪  ♫  ♪        ",
                Style::default().fg(accent),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "                      ",
                Style::default().fg(Color::DarkGray),
            )));
        }
    }
    lines
}
