//! The editable preferences behind the Settings section.
//!
//! Every preference is one [`SettingRow`]. A row knows its label, how to
//! render its current value, and how to step that value in either direction
//! — so adding a preference means adding a variant, not touching a renderer
//! and an event handler and a serializer.
//!
//! Changes apply the moment they are made: the interface behind the panel
//! *is* the preview, which is why there is no "apply" step to forget.

use super::*;
use crate::config::{AnimationSpeed, ArtworkMode, HomeDensity, VisualizerStyle};

/// One editable preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingRow {
    Theme,
    Artwork,
    HomeDensity,
    Visualizer,
    AnimationSpeed,
    ReducedMotion,
    Splash,
    SyncInterval,
    LyricsOffset,
}

impl SettingRow {
    /// Display order in the Settings panel: appearance first (the reason
    /// most people open this screen), then motion, then behavior.
    pub const ALL: [SettingRow; 9] = [
        SettingRow::Theme,
        SettingRow::Artwork,
        SettingRow::HomeDensity,
        SettingRow::Visualizer,
        SettingRow::AnimationSpeed,
        SettingRow::ReducedMotion,
        SettingRow::Splash,
        SettingRow::SyncInterval,
        SettingRow::LyricsOffset,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Theme => "Tema",
            Self::Artwork => "Capa do álbum",
            Self::HomeDensity => "Densidade da Início",
            Self::Visualizer => "Visualizador",
            Self::AnimationSpeed => "Velocidade das animações",
            Self::ReducedMotion => "Reduzir movimento",
            Self::Splash => "Animação de entrada",
            Self::SyncInterval => "Atualizar em segundo plano",
            Self::LyricsOffset => "Sincronia da letra",
        }
    }

    /// A one-line explanation of what the row affects, shown under the
    /// cursor. Settings that only say their value leave the user guessing
    /// what changing it will cost them.
    pub fn hint(self) -> &'static str {
        match self {
            Self::Theme => "Presets embutidos + os seus em ~/.config/ytmtui/themes/*.toml",
            Self::Artwork => "\"auto\" consulta o protocolo do terminal; \"blocos\" nunca consulta",
            Self::HomeDensity => "Cards compactos cabem mais linhas na tela",
            Self::Visualizer => "Espectro em tempo real na Início e no Now Playing",
            Self::AnimationSpeed => "Escala toda animação: marquee, revelações, entrada",
            Self::ReducedMotion => "Desliga animações não essenciais e economiza redraws",
            Self::Splash => "Ignorada quando \"Reduzir movimento\" está ligado",
            Self::SyncInterval => "De quanto em quanto tempo Início e Biblioteca recarregam",
            Self::LyricsOffset => "Corrige letras adiantadas ou atrasadas (também com < e >)",
        }
    }
}

/// Background-sync intervals offered, in seconds. A fixed ladder rather
/// than free entry: the useful range is narrow, and every step here is one
/// keypress.
const SYNC_STEPS: [u64; 5] = [60, 120, 300, 600, 900];

impl App {
    /// The current value of `row`, as shown in the panel.
    pub fn setting_value(&self, row: SettingRow) -> String {
        match row {
            SettingRow::Theme => self.theme_name().to_string(),
            SettingRow::Artwork => match self.artwork_mode {
                ArtworkMode::Auto => "auto".into(),
                ArtworkMode::HalfBlocks => "blocos Unicode".into(),
                ArtworkMode::Off => "desligada".into(),
            },
            SettingRow::HomeDensity => match self.home_density {
                HomeDensity::Comfortable => "confortável".into(),
                HomeDensity::Compact => "compacta".into(),
            },
            SettingRow::Visualizer => match self.visualizer_style {
                VisualizerStyle::Gradient => "gradiente".into(),
                VisualizerStyle::Mono => "cor única".into(),
                VisualizerStyle::Off => "desligado".into(),
            },
            SettingRow::AnimationSpeed => match self.ui.anim.speed() {
                AnimationSpeed::Fast => "rápida".into(),
                AnimationSpeed::Normal => "normal".into(),
                AnimationSpeed::Slow => "lenta".into(),
            },
            SettingRow::ReducedMotion => yes_no(self.ui.anim.reduced_motion()),
            SettingRow::Splash => yes_no(self.splash_enabled),
            SettingRow::SyncInterval => format_interval(self.sync_interval.as_secs()),
            SettingRow::LyricsOffset => {
                let offset = self.ui.lyrics.offset_ms();
                if offset == 0 {
                    "original".into()
                } else {
                    format!("{:+.2}s", offset as f64 / 1000.0)
                }
            }
        }
    }

    /// Steps `row` by `delta` (`+1` / `-1`) and persists the result.
    ///
    /// Every change takes effect immediately — the screen behind the panel
    /// is the preview.
    pub fn step_setting(&mut self, row: SettingRow, delta: i32) {
        match row {
            SettingRow::Theme => {
                let count = self.themes.len();
                self.theme_index = wrap(self.theme_index, delta, count);
            }
            SettingRow::Artwork => {
                self.artwork_mode = cycle(
                    &[ArtworkMode::Auto, ArtworkMode::HalfBlocks, ArtworkMode::Off],
                    self.artwork_mode,
                    delta,
                );
                self.apply_artwork_mode();
            }
            SettingRow::HomeDensity => {
                self.home_density = cycle(
                    &[HomeDensity::Comfortable, HomeDensity::Compact],
                    self.home_density,
                    delta,
                );
            }
            SettingRow::Visualizer => {
                self.visualizer_style = cycle(
                    &[
                        VisualizerStyle::Gradient,
                        VisualizerStyle::Mono,
                        VisualizerStyle::Off,
                    ],
                    self.visualizer_style,
                    delta,
                );
            }
            SettingRow::AnimationSpeed => {
                let speed = cycle(
                    &[
                        AnimationSpeed::Slow,
                        AnimationSpeed::Normal,
                        AnimationSpeed::Fast,
                    ],
                    self.ui.anim.speed(),
                    delta,
                );
                self.ui.anim.set_speed(speed);
            }
            SettingRow::ReducedMotion => {
                let reduced = !self.ui.anim.reduced_motion();
                self.ui.anim.set_reduced_motion(reduced);
            }
            SettingRow::Splash => self.splash_enabled = !self.splash_enabled,
            SettingRow::SyncInterval => {
                let current = self.sync_interval.as_secs();
                let index = SYNC_STEPS
                    .iter()
                    .position(|s| *s == current)
                    .unwrap_or(SYNC_STEPS.len() / 2);
                let next = wrap(index, delta, SYNC_STEPS.len());
                self.sync_interval = std::time::Duration::from_secs(SYNC_STEPS[next]);
            }
            SettingRow::LyricsOffset => {
                self.ui.lyrics.adjust_offset(250 * delta as i64);
            }
        }
        self.save_config();
    }

    /// Rebuilds (or discards) the album-art picker after the artwork mode
    /// changed. Switching to "off" must also drop the cover already on
    /// screen, or the setting appears not to have taken.
    fn apply_artwork_mode(&mut self) {
        self.picker = crate::artwork::build_picker(self.artwork_mode);
        if self.picker.is_none() {
            self.artwork = None;
            self.artwork_source = None;
        } else {
            self.rebuild_artwork();
        }
        self.clear_screen = true;
    }
}

fn yes_no(value: bool) -> String {
    if value { "sim" } else { "não" }.to_string()
}

fn format_interval(secs: u64) -> String {
    if secs % 60 == 0 {
        format!("{} min", secs / 60)
    } else {
        format!("{secs}s")
    }
}

/// Advances `index` by `delta` within `len`, wrapping at both ends. A
/// setting that stops at its last value leaves the user pressing a key that
/// does nothing.
fn wrap(index: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let len = len as i32;
    (((index as i32 + delta) % len + len) % len) as usize
}

/// The value `delta` steps away from `current` in `options`, wrapping.
fn cycle<T: Copy + PartialEq>(options: &[T], current: T, delta: i32) -> T {
    let index = options.iter().position(|o| *o == current).unwrap_or(0);
    options[wrap(index, delta, options.len())]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_covers_both_directions_and_both_edges() {
        assert_eq!(wrap(0, 1, 3), 1);
        assert_eq!(wrap(2, 1, 3), 0, "forward past the end wraps to the start");
        assert_eq!(
            wrap(0, -1, 3),
            2,
            "backward past the start wraps to the end"
        );
        // A settings list can never be empty in practice, but indexing an
        // empty one must not panic.
        assert_eq!(wrap(0, 1, 0), 0);
    }

    #[test]
    fn every_row_round_trips_through_next_and_previous() {
        // Stepping forward then back must land exactly where it started, or
        // a user who overshoots can never get their old value again.
        for row in SettingRow::ALL {
            let mut app = App::new_for_tests();
            let before = app.setting_value(row);
            app.step_setting(row, 1);
            app.step_setting(row, -1);
            assert_eq!(
                app.setting_value(row),
                before,
                "{} did not round-trip",
                row.label()
            );
        }
    }

    #[test]
    fn every_row_actually_changes_when_stepped() {
        // A row that renders the same value after a step is either broken or
        // has nothing to offer, and either way should not be on screen.
        for row in SettingRow::ALL {
            let mut app = App::new_for_tests();
            let before = app.setting_value(row);
            app.step_setting(row, 1);
            assert_ne!(
                app.setting_value(row),
                before,
                "{} does nothing when stepped",
                row.label()
            );
        }
    }

    #[test]
    fn stepping_a_row_all_the_way_around_returns_to_the_start() {
        // Wrapping, checked through the real values rather than the index.
        let mut app = App::new_for_tests();
        let start = app.setting_value(SettingRow::Visualizer);
        for _ in 0..3 {
            app.step_setting(SettingRow::Visualizer, 1);
        }
        assert_eq!(app.setting_value(SettingRow::Visualizer), start);
    }

    #[test]
    fn turning_off_the_artwork_drops_the_cover_already_on_screen() {
        let mut app = App::new_for_tests();
        app.artwork_mode = ArtworkMode::HalfBlocks;
        app.artwork_source = Some(image::DynamicImage::new_rgb8(2, 2));

        // HalfBlocks -> Off is one step forward.
        app.step_setting(SettingRow::Artwork, 1);

        assert_eq!(app.artwork_mode, ArtworkMode::Off);
        assert!(
            app.artwork_source.is_none(),
            "the setting must visibly take effect, not just change a flag"
        );
    }

    #[test]
    fn reduced_motion_and_splash_are_independent_toggles() {
        let mut app = App::new_for_tests();
        app.step_setting(SettingRow::Splash, 1);
        assert_eq!(app.setting_value(SettingRow::Splash), "sim");
        assert_eq!(
            app.setting_value(SettingRow::ReducedMotion),
            "não",
            "toggling one must not move the other"
        );
    }

    #[test]
    fn the_sync_interval_walks_a_fixed_ladder() {
        let mut app = App::new_for_tests();
        app.sync_interval = std::time::Duration::from_secs(60);
        app.step_setting(SettingRow::SyncInterval, 1);
        assert_eq!(app.sync_interval.as_secs(), 120);
        app.step_setting(SettingRow::SyncInterval, -1);
        assert_eq!(app.sync_interval.as_secs(), 60);
        // Below the first step it wraps to the last rather than sticking.
        app.step_setting(SettingRow::SyncInterval, -1);
        assert_eq!(app.sync_interval.as_secs(), 900);
    }

    #[test]
    fn an_unrecognized_sync_interval_lands_on_a_real_step() {
        // A hand-edited config can hold any number; stepping must snap it
        // onto the ladder instead of doing nothing.
        let mut app = App::new_for_tests();
        app.sync_interval = std::time::Duration::from_secs(437);
        app.step_setting(SettingRow::SyncInterval, 1);
        assert!(SYNC_STEPS.contains(&app.sync_interval.as_secs()));
    }
}
