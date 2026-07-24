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
    fn kick_animation_is_a_no_op_under_reduced_motion() {
        let mut app = App::new_for_tests();
        app.reduced_motion = true;
        app.kick_animation(std::time::Duration::from_millis(500));
        assert!(
            !app.animating(),
            "reduced motion must never hold the fast redraw tier open"
        );
    }
    #[test]
    fn kick_animation_scales_the_window_by_animation_speed() {
        // Same base duration, three speeds: the resulting deadline must
        // order Slow > Normal > Fast, matching `AnimationSpeed::factor`.
        let base = std::time::Duration::from_millis(200);
        let deadline_for = |speed: AnimationSpeed| {
            let mut app = App::new_for_tests();
            app.animation_speed = speed;
            let before = std::time::Instant::now();
            app.kick_animation(base);
            app.animate_until.expect("kick sets a deadline") - before
        };
        let fast = deadline_for(AnimationSpeed::Fast);
        let normal = deadline_for(AnimationSpeed::Normal);
        let slow = deadline_for(AnimationSpeed::Slow);
        assert!(
            fast < normal,
            "fast ({fast:?}) must be shorter than normal ({normal:?})"
        );
        assert!(
            normal < slow,
            "normal ({normal:?}) must be shorter than slow ({slow:?})"
        );
    }
    #[test]
    fn animating_expires_after_the_kicked_window_elapses() {
        let mut app = App::new_for_tests();
        // A 1ms kick is effectively already expired by the time the assert
        // below runs — no sleep needed in the test.
        app.kick_animation(std::time::Duration::from_millis(1));
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(!app.animating(), "the animation window must expire");
    }
    #[test]
    fn needs_fast_animation_is_true_while_animating_even_with_nothing_playing() {
        let mut app = App::new_for_tests();
        assert!(!app.needs_fast_animation(), "idle app needs no animation");
        app.kick_animation(std::time::Duration::from_millis(500));
        assert!(
            app.needs_fast_animation(),
            "a kicked-off transition holds the fast tier even without playback"
        );
    }
    #[test]
    fn reduced_motion_drops_the_fast_tier_even_while_the_visualizer_would_animate() {
        let mut app = App::new_for_tests();
        app.reduced_motion = true;
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
        assert!(app.selection_changed_at.is_none());

        app.move_home(HomeDirection::Down);

        assert!(
            app.selection_changed_at.is_some(),
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

        assert!(app.track_changed_at >= before);
        assert!(
            app.needs_fast_animation(),
            "starting a track kicks the fast tier for the metadata fade-in"
        );
    }
}
