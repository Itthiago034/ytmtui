//! Presentation state: everything the UI needs to draw a frame that is not
//! part of the domain.
//!
//! Scroll offsets, grid geometry and animation timing describe *how the app
//! is being looked at*, not what it is playing. Keeping them here means the
//! domain modules under `crate::app` never carry a field that only a
//! renderer reads, and every animation reads its clock from one place.

use std::time::{Duration, Instant};

use crate::config::AnimationSpeed;

/// Everything the renderer owns.
#[derive(Debug)]
pub struct UiState {
    pub anim: AnimationClock,
    /// State of the Lyrics panel: scroll, browsing cursor, auto-follow and
    /// the timing correction.
    pub lyrics: LyricsView,
    /// Manual scroll offset of the Help panel. Clamped at render time to the
    /// real text height, which only the renderer knows.
    pub help_scroll: u16,
    /// Number of Home card columns the current layout fits. Written by the
    /// renderer (it measures the area), read by `App::move_home` so spatial
    /// navigation matches what is actually on screen.
    pub home_columns: usize,
    /// Whether the entry animation is still running. Cleared when it
    /// finishes or when the user presses any key; `false` from the start
    /// when the animation is turned off or reduced motion is on.
    splash_running: bool,
}

impl UiState {
    pub fn new(
        speed: AnimationSpeed,
        reduced_motion: bool,
        splash: bool,
        lyrics_offset_ms: i64,
    ) -> Self {
        Self {
            anim: AnimationClock::new(speed, reduced_motion),
            lyrics: LyricsView::new(lyrics_offset_ms),
            help_scroll: 0,
            home_columns: 1,
            // Reduced motion outranks the preference: it exists to stop
            // exactly this kind of decorative motion.
            splash_running: splash && !reduced_motion,
        }
    }

    /// The entry animation's current phase, or `None` once it is over.
    ///
    /// Resolving this from the boot clock (rather than storing a phase)
    /// keeps the animation correct across a speed change mid-flight.
    pub fn splash_phase(&self) -> Option<crate::ui::splash::Phase> {
        if !self.splash_running {
            return None;
        }
        match crate::ui::splash::phase_at(self.anim.since_boot_ms()) {
            crate::ui::splash::Phase::Done => None,
            phase => Some(phase),
        }
    }

    /// Cancels the entry animation. Called on the first key press, so a
    /// user who wants to get straight to work never waits it out.
    pub fn skip_splash(&mut self) {
        self.splash_running = false;
    }
}

/// How long auto-follow stays out of the way after a manual scroll.
///
/// Long enough to read a verse ahead without the view yanking back, short
/// enough that a user who scrolled by accident isn't stranded.
const FOLLOW_SUSPEND: Duration = Duration::from_secs(4);

/// The Lyrics panel's view state.
#[derive(Debug)]
pub struct LyricsView {
    /// Scroll offset for plain (unsynced) lyrics, which have no lines to
    /// put a cursor on.
    pub scroll: u16,
    /// Line the user has browsed to in synced lyrics. `None` while
    /// auto-follow is driving the view.
    cursor: Option<usize>,
    /// When auto-follow resumes; `None` means it is following right now.
    suspended_until: Option<Instant>,
    /// When the sung line last changed, for its entry fade.
    line_changed_at: Option<Instant>,
    /// Correction applied to every lyric timestamp, in milliseconds.
    /// Positive means the lyrics were running late and are pulled earlier.
    /// InnerTube timings routinely drift a beat either way.
    offset_ms: i64,
}

impl LyricsView {
    fn new(offset_ms: i64) -> Self {
        Self {
            scroll: 0,
            cursor: None,
            suspended_until: None,
            line_changed_at: None,
            offset_ms,
        }
    }

    /// Whether the view is currently tracking playback.
    pub fn following(&self) -> bool {
        // `Option::is_none_or` would read better but is newer than this
        // crate's MSRV (1.75).
        match self.suspended_until {
            Some(until) => Instant::now() >= until,
            None => true,
        }
    }

    /// The line the user is looking at: their cursor while browsing, or
    /// `active` (the line being sung) while following.
    pub fn focused_line(&self, active: Option<usize>) -> Option<usize> {
        if self.following() {
            active
        } else {
            self.cursor.or(active)
        }
    }

    /// Moves the browsing cursor by `delta`, clamped to `len`, and holds
    /// auto-follow off while the user reads.
    pub fn browse(&mut self, delta: isize, len: usize, active: Option<usize>) {
        if len == 0 {
            return;
        }
        let from = self.focused_line(active).unwrap_or(0) as isize;
        self.cursor = Some(from.saturating_add(delta).clamp(0, len as isize - 1) as usize);
        self.suspended_until = Some(Instant::now() + FOLLOW_SUSPEND);
    }

    /// Returns to following playback immediately.
    pub fn follow_now(&mut self) {
        self.cursor = None;
        self.suspended_until = None;
    }

    /// Drops a browsing cursor left over from the previous track.
    pub fn reset(&mut self) {
        self.scroll = 0;
        self.follow_now();
    }

    /// Records that the sung line moved on.
    pub fn mark_line_changed(&mut self) {
        self.line_changed_at = Some(Instant::now());
    }

    /// How far into its entry fade the sung line is, `0.0..=1.0`. Returns
    /// `1.0` (settled) when no line change has been seen yet.
    pub fn line_entry_progress(&self, fade_ms: u128) -> f32 {
        match self.line_changed_at {
            Some(at) => progress(at.elapsed().as_millis(), fade_ms),
            None => 1.0,
        }
    }

    pub fn offset_ms(&self) -> i64 {
        self.offset_ms
    }

    /// Nudges the timing correction, clamped so a stuck key cannot push the
    /// lyrics somewhere they can never come back from.
    pub fn adjust_offset(&mut self, delta_ms: i64) {
        self.offset_ms = (self.offset_ms + delta_ms).clamp(-10_000, 10_000);
    }

    /// Playback position as the lyrics see it, with the correction applied.
    /// Saturates at zero: a negative position has no meaning.
    pub fn corrected_position_ms(&self, position_ms: u64) -> u64 {
        (position_ms as i64 + self.offset_ms).max(0) as u64
    }
}

/// The single source of truth for animation timing.
///
/// Every animated element in the app measures its progress as "milliseconds
/// elapsed, in animation time" — real elapsed time divided by
/// [`AnimationSpeed::factor`]. Dividing here rather than at each call site is
/// what makes the speed setting mean the same thing everywhere: a *faster*
/// speed makes the same fixed threshold arrive sooner, so the animation ends
/// earlier. (Durations that are *set* rather than measured, like
/// [`Self::kick`]'s window, multiply by the same factor instead — shorter
/// window, same direction.)
#[derive(Debug)]
pub struct AnimationClock {
    speed: AnimationSpeed,
    reduced_motion: bool,
    /// Deadline of the transition currently in flight; `None` when idle.
    animate_until: Option<Instant>,
    /// When the Home grid selection last moved. `None` before any navigation,
    /// which already reads as "the reveal finished long ago".
    selection_changed_at: Option<Instant>,
    /// When the current track started. Never `None`: before the first track
    /// the value is irrelevant, since there is nothing to reveal.
    track_changed_at: Instant,
    /// When the app started, for the entry animation.
    booted_at: Instant,
    /// When the open section last changed, for the staggered row reveal.
    /// `None` before the first switch, which reads as "already settled".
    section_changed_at: Option<Instant>,
}

impl AnimationClock {
    pub fn new(speed: AnimationSpeed, reduced_motion: bool) -> Self {
        let now = Instant::now();
        Self {
            speed,
            reduced_motion,
            animate_until: None,
            selection_changed_at: None,
            track_changed_at: now,
            booted_at: now,
            section_changed_at: None,
        }
    }

    pub fn speed(&self) -> AnimationSpeed {
        self.speed
    }

    pub fn reduced_motion(&self) -> bool {
        self.reduced_motion
    }

    pub fn set_speed(&mut self, speed: AnimationSpeed) {
        self.speed = speed;
    }

    /// Turning reduced motion *on* also retires any transition already in
    /// flight, so the switch takes effect on the very next frame instead of
    /// letting the current animation play itself out.
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.reduced_motion = reduced;
        if reduced {
            self.animate_until = None;
        }
    }

    /// Extends the fast-redraw window by `base` (scaled by the current speed)
    /// so a just-started transition keeps drawing at the fast tier for
    /// exactly as long as it takes to play out — never indefinitely.
    ///
    /// Overlapping kicks only ever extend the deadline, never shorten it, so
    /// rapid selection changes don't cut each other's tail short. A no-op
    /// under reduced motion: there is no transition to redraw for.
    pub fn kick(&mut self, base: Duration) {
        if self.reduced_motion {
            return;
        }
        let scaled_ms = (base.as_millis() as f64 * self.speed.factor()).round() as u64;
        let candidate = Instant::now() + Duration::from_millis(scaled_ms);
        self.animate_until = Some(match self.animate_until {
            Some(existing) => existing.max(candidate),
            None => candidate,
        });
    }

    /// Whether a transition started by [`Self::kick`] is still running.
    pub fn animating(&self) -> bool {
        self.animate_until
            .is_some_and(|until| Instant::now() < until)
    }

    /// Records that the Home grid selection moved, and kicks a matching
    /// redraw window for the card's staged reveal.
    pub fn mark_selection_changed(&mut self) {
        self.selection_changed_at = Some(Instant::now());
        self.kick(Duration::from_millis(220));
    }

    /// Records that the current track changed, and kicks a matching redraw
    /// window for the now-playing metadata fade.
    pub fn mark_track_changed(&mut self) {
        self.track_changed_at = Instant::now();
        self.kick(Duration::from_millis(300));
    }

    /// Records that the open section changed, and kicks a matching redraw
    /// window for the staggered row reveal.
    pub fn mark_section_changed(&mut self) {
        self.section_changed_at = Some(Instant::now());
        self.kick(Duration::from_millis(180));
    }

    /// Animation-time milliseconds since the open section changed, or `None`
    /// when it never has.
    pub fn since_section_change_ms(&self) -> Option<u128> {
        self.section_changed_at
            .map(|at| self.scale(at.elapsed().as_millis()))
    }

    /// Whether any selection reveal has ever started. Buffer tests that never
    /// navigate rely on `None` meaning "already at the final state".
    pub fn selection_ever_changed(&self) -> bool {
        self.selection_changed_at.is_some()
    }

    /// Animation-time milliseconds since the selection last moved, or `None`
    /// when it never has.
    pub fn since_selection_ms(&self) -> Option<u128> {
        self.selection_changed_at
            .map(|at| self.scale(at.elapsed().as_millis()))
    }

    /// Animation-time milliseconds since the current track started.
    pub fn since_track_change_ms(&self) -> u128 {
        self.scale(self.track_changed_at.elapsed().as_millis())
    }

    /// Animation-time milliseconds since the app started, for the entry
    /// animation.
    pub fn since_boot_ms(&self) -> u128 {
        self.scale(self.booted_at.elapsed().as_millis())
    }

    fn scale(&self, real_ms: u128) -> u128 {
        (real_ms as f64 / self.speed.factor()) as u128
    }

    /// Backdates the track change so a test can observe a chosen point of
    /// the fade without sleeping.
    #[cfg(test)]
    pub(crate) fn backdate_track_change(&mut self, ago: Duration) {
        self.track_changed_at = Instant::now() - ago;
    }

    /// Same, for the Home card's staged reveal.
    #[cfg(test)]
    pub(crate) fn backdate_selection_change(&mut self, ago: Duration) {
        self.selection_changed_at = Some(Instant::now() - ago);
    }

    /// Backdates the boot instant so a test can observe a chosen phase of
    /// the entry animation without sleeping through the earlier ones.
    #[cfg(test)]
    pub(crate) fn backdate_boot(&mut self, ago: Duration) {
        self.booted_at = Instant::now() - ago;
    }

    /// When the current track started, for tests asserting the mark landed.
    #[cfg(test)]
    pub(crate) fn track_changed_at(&self) -> Instant {
        self.track_changed_at
    }
}

/// Progress of an animation of length `duration_ms`, clamped to `0.0..=1.0`.
/// A zero-length animation is already finished.
pub fn progress(elapsed_ms: u128, duration_ms: u128) -> f32 {
    if duration_ms == 0 {
        return 1.0;
    }
    (elapsed_ms as f32 / duration_ms as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kick_is_a_no_op_under_reduced_motion() {
        let mut clock = AnimationClock::new(AnimationSpeed::Normal, true);
        clock.kick(Duration::from_millis(500));
        assert!(!clock.animating());
    }

    #[test]
    fn kick_scales_its_window_by_the_speed() {
        // Slow stretches the window, fast shortens it. Compare the deadlines
        // directly rather than sleeping.
        let mut fast = AnimationClock::new(AnimationSpeed::Fast, false);
        let mut slow = AnimationClock::new(AnimationSpeed::Slow, false);
        let base = Duration::from_millis(1000);
        fast.kick(base);
        slow.kick(base);
        assert!(
            fast.animate_until.unwrap() < slow.animate_until.unwrap(),
            "a faster speed must finish its window sooner"
        );
    }

    #[test]
    fn the_window_expires_on_its_own() {
        let mut clock = AnimationClock::new(AnimationSpeed::Normal, false);
        clock.kick(Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(5));
        assert!(
            !clock.animating(),
            "the fast tier must not be held open forever"
        );
    }

    #[test]
    fn overlapping_kicks_extend_rather_than_truncate() {
        let mut clock = AnimationClock::new(AnimationSpeed::Normal, false);
        clock.kick(Duration::from_millis(1000));
        let long = clock.animate_until.unwrap();
        clock.kick(Duration::from_millis(10));
        assert_eq!(
            clock.animate_until.unwrap(),
            long,
            "a short kick must not cut a longer one short"
        );
    }

    #[test]
    fn turning_on_reduced_motion_retires_the_running_transition() {
        let mut clock = AnimationClock::new(AnimationSpeed::Normal, false);
        clock.kick(Duration::from_millis(5000));
        assert!(clock.animating());
        clock.set_reduced_motion(true);
        assert!(
            !clock.animating(),
            "the switch must apply on the next frame"
        );
    }

    #[test]
    fn elapsed_time_is_scaled_so_faster_speeds_reach_thresholds_sooner() {
        let fast = AnimationClock::new(AnimationSpeed::Fast, false);
        let slow = AnimationClock::new(AnimationSpeed::Slow, false);
        // Same real elapsed time, different animation time.
        assert!(fast.scale(600) > slow.scale(600));
        assert_eq!(
            AnimationClock::new(AnimationSpeed::Normal, false).scale(600),
            600,
            "normal speed is the identity"
        );
    }

    #[test]
    fn a_never_moved_selection_reports_no_elapsed_time() {
        let clock = AnimationClock::new(AnimationSpeed::Normal, false);
        assert!(!clock.selection_ever_changed());
        assert_eq!(clock.since_selection_ms(), None);
    }

    // --- lyrics view ---

    fn lyrics() -> LyricsView {
        LyricsView::new(0)
    }

    #[test]
    fn a_fresh_view_follows_the_song() {
        let view = lyrics();
        assert!(view.following());
        assert_eq!(view.focused_line(Some(4)), Some(4), "follows the sung line");
    }

    #[test]
    fn browsing_suspends_follow_and_takes_over_the_focus() {
        let mut view = lyrics();
        view.browse(1, 10, Some(4));
        assert!(!view.following(), "manual scrolling must stop the yanking");
        assert_eq!(view.focused_line(Some(4)), Some(5), "the cursor leads now");
    }

    #[test]
    fn browsing_clamps_to_the_ends_of_the_song() {
        let mut view = lyrics();
        view.browse(-100, 10, Some(0));
        assert_eq!(view.focused_line(Some(0)), Some(0));
        view.browse(100, 10, Some(0));
        assert_eq!(view.focused_line(Some(0)), Some(9));
    }

    #[test]
    fn browsing_an_empty_lyric_is_a_no_op() {
        let mut view = lyrics();
        view.browse(1, 0, None);
        assert!(view.following(), "nothing to browse, nothing to suspend");
    }

    #[test]
    fn following_again_hands_the_view_back_to_the_song() {
        let mut view = lyrics();
        view.browse(3, 10, Some(0));
        view.follow_now();
        assert!(view.following());
        assert_eq!(view.focused_line(Some(7)), Some(7));
    }

    #[test]
    fn a_new_track_drops_a_cursor_left_over_from_the_last_one() {
        let mut view = lyrics();
        view.browse(5, 10, Some(0));
        view.scroll = 12;
        view.reset();
        assert!(view.following());
        assert_eq!(view.scroll, 0);
    }

    #[test]
    fn a_line_that_never_changed_is_already_settled() {
        // Buffer tests that never tick must see the final style, not a
        // line frozen mid-fade.
        assert_eq!(lyrics().line_entry_progress(180), 1.0);
    }

    #[test]
    fn a_just_changed_line_starts_its_entry_fade() {
        let mut view = lyrics();
        view.mark_line_changed();
        assert!(view.line_entry_progress(10_000) < 1.0);
    }

    #[test]
    fn the_offset_shifts_the_position_in_both_directions() {
        let mut view = lyrics();
        assert_eq!(view.corrected_position_ms(1000), 1000);
        view.adjust_offset(250);
        assert_eq!(view.corrected_position_ms(1000), 1250);
        view.adjust_offset(-500);
        assert_eq!(view.corrected_position_ms(1000), 750);
    }

    #[test]
    fn a_negative_offset_never_produces_a_negative_position() {
        let mut view = lyrics();
        view.adjust_offset(-5000);
        assert_eq!(view.corrected_position_ms(100), 0);
    }

    #[test]
    fn the_offset_is_bounded_so_a_stuck_key_cannot_strand_the_lyrics() {
        let mut view = lyrics();
        for _ in 0..200 {
            view.adjust_offset(250);
        }
        assert_eq!(view.offset_ms(), 10_000);
        for _ in 0..400 {
            view.adjust_offset(-250);
        }
        assert_eq!(view.offset_ms(), -10_000);
    }

    #[test]
    fn progress_clamps_at_both_ends() {
        assert_eq!(progress(0, 1000), 0.0);
        assert_eq!(progress(500, 1000), 0.5);
        assert_eq!(progress(4000, 1000), 1.0);
        // A zero-length animation never leaves the caller mid-transition.
        assert_eq!(progress(0, 0), 1.0);
    }
}
