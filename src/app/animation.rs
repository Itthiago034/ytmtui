//! Redraw tiers.
//!
//! The app redraws on an adaptive timer (see the poll loop in `main.rs`).
//! This module answers the only question that loop asks: *how often should
//! the next frame come?* The timing of individual animations belongs to
//! [`crate::ui::state::AnimationClock`], which `App` reaches through
//! `self.ui.anim`.

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
    /// [`AnimationClock::kick`] (selection change, track change) — all must
    /// look like continuous motion while they're visible.
    ///
    /// `reduced_motion` puts the app in an economy mode: the two continuous
    /// drivers (visualizer/karaoke) stop requiring the 60ms tier and fall
    /// back to the 200ms one via [`Self::needs_animation`] — there is no
    /// continuous motion left to redraw quickly for. Transitions never fire
    /// under `reduced_motion` either, since [`AnimationClock::kick`] is a
    /// no-op there, so `animating()` is always false in that mode.
    pub fn needs_fast_animation(&self) -> bool {
        // The entry animation is continuous motion covering the whole
        // frame; nothing else on screen matters while it runs.
        if self.ui.splash_phase().is_some() {
            return true;
        }
        if self.ui.anim.animating() {
            return true;
        }
        if self.ui.anim.reduced_motion() {
            return false;
        }
        let animated_section = self.section == Section::Inicio
            || (self.section == Section::Letra
                && matches!(self.lyrics, crate::lyrics::LyricsState::Synced { .. }));
        animated_section && self.current.is_some() && !self.player.is_paused()
    }

    /// Consumes the pending full-clear flag set by `clear_artwork`. The main
    /// loop calls this right before drawing and, if set, erases the whole
    /// terminal so leftover Kitty/Sixel graphics from the previous cover
    /// don't linger behind the next frame.
    pub fn take_clear_screen(&mut self) -> bool {
        std::mem::take(&mut self.clear_screen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::app::testing::*;

    #[test]
    fn animation_is_only_needed_while_loading_or_playing() {
        let mut app = App::new_for_tests();
        assert!(!app.needs_animation(), "idle app needs no animation");

        app.begin_task();
        assert!(app.needs_animation(), "loading shows the spinner");
        app.finish_task();

        app.current = Some(crate::models::Track::default());
        assert!(app.needs_animation(), "playback progress animates");
    }

    #[test]
    fn needs_fast_animation_is_true_while_animating_even_with_nothing_playing() {
        let mut app = App::new_for_tests();
        assert!(!app.needs_fast_animation(), "idle app needs no animation");
        app.ui.anim.kick(std::time::Duration::from_millis(500));
        assert!(
            app.needs_fast_animation(),
            "a kicked-off transition holds the fast tier even without playback"
        );
    }

    #[test]
    fn reduced_motion_drops_the_fast_tier_even_while_the_visualizer_would_animate() {
        let mut app = App::new_for_tests();
        app.ui.anim.set_reduced_motion(true);
        app.section = Section::Inicio;
        app.current = Some(Track::default());
        // Not paused: without `reduced_motion` this would need the fast tier
        // (Home visualizer). Under `reduced_motion`, it must not.
        assert!(!app.player.is_paused());
        assert!(
            !app.needs_fast_animation(),
            "reduced motion falls back to the 200ms tier for continuous drivers"
        );
        assert!(
            app.needs_animation(),
            "playback progress still animates at the economy tier"
        );
    }

    #[test]
    fn moving_the_home_selection_marks_the_change_and_kicks_the_fast_tier() {
        let mut app = App::new_for_tests();
        app.section = Section::Inicio;
        app.home = home_sections();
        app.list_state.select(Some(0));
        assert!(!app.ui.anim.selection_ever_changed());

        app.move_home(HomeDirection::Down);

        assert!(
            app.ui.anim.selection_ever_changed(),
            "move_home marks the selection as just-changed"
        );
        assert!(
            app.needs_fast_animation(),
            "the selection-change kick holds the fast tier"
        );
    }

    #[tokio::test]
    async fn starting_a_track_marks_track_changed_at_and_kicks_the_fast_tier() {
        // `start_current` spawns background tasks (audio resolution, lyrics,
        // artwork), so this needs a real Tokio runtime, like
        // `entering_a_recent_home_card_preserves_history_order_and_selected_index`
        // above.
        let mut app = App::new_for_tests();
        app.queue = vec![track("a")];
        app.queue_index = Some(0);
        let before = std::time::Instant::now();

        app.start_current();

        assert!(app.ui.anim.track_changed_at() >= before);
        assert!(
            app.needs_fast_animation(),
            "starting a track kicks the fast tier for the metadata fade-in"
        );
    }
}
