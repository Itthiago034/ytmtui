//! Animation timing and redraw tiers.
//!
//! The app redraws on an adaptive timer (see the poll loop in `main.rs`), so
//! everything that decides *how often* to redraw — and how long a transition
//! is still in flight — is grouped here rather than scattered across the
//! state that happens to animate.

use super::*;

impl App {
    /// Whether the UI currently benefits from frequent redraws: a loading
    /// spinner is animating or playback progress is advancing. Idle frames
    /// can redraw far less often without losing feedback.
    pub fn needs_animation(&self) -> bool {
        self.is_loading() || (self.current.is_some() && !self.player.is_paused())
    }

    /// Whether the open section is actively animating and needs the fast
    /// redraw tier: the Home spectrum visualizer, the synced-lyrics karaoke
    /// wipe, or a time-based transition kicked off by
    /// [`Self::kick_animation`] (selection change, track change) — all must
    /// look like continuous motion while they're visible.
    ///
    /// `reduced_motion` puts the app in an economy mode: the two continuous
    /// drivers (visualizer/karaoke) stop requiring the 60ms tier and fall
    /// back to the 200ms one via [`Self::needs_animation`] — there is no
    /// continuous motion left to redraw quickly for. Transitions never fire
    /// under `reduced_motion` either, since [`Self::kick_animation`] is a
    /// no-op there, so `animating()` is always false in that mode.
    pub fn needs_fast_animation(&self) -> bool {
        if self.animating() {
            return true;
        }
        if self.reduced_motion {
            return false;
        }
        let animated_section = self.section == Section::Inicio
            || (self.section == Section::Letra
                && matches!(self.lyrics, crate::lyrics::LyricsState::Synced { .. }));
        animated_section && self.current.is_some() && !self.player.is_paused()
    }

    /// Extends the fast-redraw window by `base` (scaled by
    /// [`AnimationSpeed::factor`]) from now, so a just-kicked-off transition
    /// (selection change, track change) keeps drawing at the 60ms tier for
    /// exactly as long as it takes to play out — never indefinitely. A
    /// no-op under `reduced_motion`: that mode never wants the fast tier for
    /// a transition, since the transition itself is skipped (see
    /// `ui::main_panel::reveal_stage`/`ui::now_playing`'s stage functions).
    /// Calling this while an earlier animation is still running only ever
    /// extends the deadline, never shortens it (`max`), so overlapping kicks
    /// (e.g. rapid selection changes) don't cut each other's tail short.
    pub(crate) fn kick_animation(&mut self, base: std::time::Duration) {
        if self.reduced_motion {
            return;
        }
        let scaled_ms = (base.as_millis() as f64 * self.animation_speed.factor()).round() as u64;
        let candidate = std::time::Instant::now() + std::time::Duration::from_millis(scaled_ms);
        self.animate_until = Some(match self.animate_until {
            Some(existing) => existing.max(candidate),
            None => candidate,
        });
    }

    /// Whether a transition kicked off by [`Self::kick_animation`] is still
    /// in progress.
    pub(super) fn animating(&self) -> bool {
        self.animate_until
            .is_some_and(|until| std::time::Instant::now() < until)
    }

    /// Marks the Home grid's selection as just-changed (drives
    /// `ui::main_panel::draw_card`'s staged reveal of the selected card) and
    /// kicks a matching fast-redraw window.
    pub(super) fn mark_selection_changed(&mut self) {
        self.selection_changed_at = Some(std::time::Instant::now());
        self.kick_animation(std::time::Duration::from_millis(220));
    }

    /// Consumes the pending full-clear flag set by [`Self::clear_artwork`].
    /// The main loop calls this right before drawing and, if set, erases the
    /// whole terminal so leftover Kitty/Sixel graphics from the previous
    /// cover don't linger behind the next frame.
    pub fn take_clear_screen(&mut self) -> bool {
        std::mem::take(&mut self.clear_screen)
    }
}
