use crate::autocrap::config::ReversalFilter;

/// Holds back a worn encoder's opposing ticks until they prove to be a real reversal.
///
/// Deferred ticks are never silently dropped when they turn out to be genuine: they
/// are applied together at the moment the reversal is confirmed, so a real direction
/// change costs latency rather than travel.
#[derive(Debug)]
pub struct ReversalFilterState {
    settings: ReversalFilter,
    established_direction: i8,
    deferred_reversal_ticks: Vec<i8>,
}

impl ReversalFilterState {
    pub fn new(settings: ReversalFilter) -> ReversalFilterState {
        ReversalFilterState {
            settings,
            established_direction: 0,
            deferred_reversal_ticks: vec![],
        }
    }

    /// Returns the delta to apply now, which is zero while a reversal is still unproven.
    pub fn accept_tick(&mut self, delta: i8, millis_since_previous_tick: u64) -> i8 {
        if delta == 0 {
            return 0;
        }

        let tick_direction = if delta > 0 { 1 } else { -1 };

        // contact noise arrives in an unbroken burst, so anything after a pause is
        // the user, and is believed without question
        if millis_since_previous_tick >= self.settings.deliberate_reversal_pause_ms {
            let recovered = self.take_deferred_ticks();
            self.established_direction = tick_direction;
            return saturating_sum(recovered, delta);
        }

        if tick_direction == self.established_direction || self.established_direction == 0 {
            // the turn resumed its original direction, so anything held back was noise
            self.deferred_reversal_ticks.clear();
            self.established_direction = tick_direction;
            return delta;
        }

        self.deferred_reversal_ticks.push(delta);

        if self.deferred_reversal_ticks.len() < self.settings.ticks_to_confirm_reversal as usize {
            return 0;
        }

        let confirmed = self.take_deferred_ticks();
        self.established_direction = tick_direction;
        saturating_sum(confirmed, 0)
    }

    fn take_deferred_ticks(&mut self) -> i8 {
        let total: i32 = self.deferred_reversal_ticks.iter().map(|tick| *tick as i32).sum();
        self.deferred_reversal_ticks.clear();
        total.clamp(i8::MIN as i32, i8::MAX as i32) as i8
    }
}

fn saturating_sum(a: i8, b: i8) -> i8 {
    (a as i32 + b as i32).clamp(i8::MIN as i32, i8::MAX as i32) as i8
}

#[cfg(test)]
mod tests {
    use super::*;

    const MID_TURN: u64 = 10;
    const AFTER_A_PAUSE: u64 = 500;

    fn filter_confirming_after(ticks_to_confirm_reversal: u8) -> ReversalFilterState {
        ReversalFilterState::new(ReversalFilter {
            ticks_to_confirm_reversal,
            deliberate_reversal_pause_ms: 100,
        })
    }

    #[test]
    fn a_steady_turn_passes_through_untouched() {
        let mut filter = filter_confirming_after(5);

        let applied: Vec<i8> = (0..6).map(|_| filter.accept_tick(1, MID_TURN)).collect();

        assert_eq!(applied, vec![1, 1, 1, 1, 1, 1]);
    }

    #[test]
    fn an_isolated_false_reversal_never_reaches_the_output() {
        let mut filter = filter_confirming_after(5);
        for _ in 0..4 { filter.accept_tick(1, MID_TURN); }

        let during_the_glitch = filter.accept_tick(-1, MID_TURN);
        let after_the_glitch = filter.accept_tick(1, MID_TURN);

        assert_eq!(during_the_glitch, 0);
        assert_eq!(after_the_glitch, 1);
    }

    #[test]
    fn a_burst_shorter_than_the_confirmation_count_is_discarded_whole() {
        let mut filter = filter_confirming_after(5);
        for _ in 0..4 { filter.accept_tick(1, MID_TURN); }

        let during_the_burst: Vec<i8> = (0..4).map(|_| filter.accept_tick(-1, MID_TURN)).collect();
        let when_the_turn_resumes = filter.accept_tick(1, MID_TURN);

        assert_eq!(during_the_burst, vec![0, 0, 0, 0]);
        assert_eq!(when_the_turn_resumes, 1);
    }

    #[test]
    fn a_confirmed_reversal_recovers_every_tick_it_held_back() {
        let mut filter = filter_confirming_after(5);
        for _ in 0..4 { filter.accept_tick(1, MID_TURN); }

        let applied: Vec<i8> = (0..5).map(|_| filter.accept_tick(-1, MID_TURN)).collect();

        // nothing is lost: the four deferred ticks arrive with the fifth
        assert_eq!(applied, vec![0, 0, 0, 0, -5]);
    }

    #[test]
    fn a_turn_continues_freely_once_a_reversal_is_confirmed() {
        let mut filter = filter_confirming_after(2);
        filter.accept_tick(1, MID_TURN);
        filter.accept_tick(-1, MID_TURN);
        filter.accept_tick(-1, MID_TURN);

        assert_eq!(filter.accept_tick(-1, MID_TURN), -1);
    }

    #[test]
    fn a_reversal_after_a_pause_is_applied_immediately() {
        let mut filter = filter_confirming_after(5);
        for _ in 0..4 { filter.accept_tick(1, MID_TURN); }

        assert_eq!(filter.accept_tick(-1, AFTER_A_PAUSE), -1);
    }

    #[test]
    fn ticks_held_back_before_a_pause_are_not_lost() {
        let mut filter = filter_confirming_after(5);
        filter.accept_tick(1, MID_TURN);
        filter.accept_tick(-1, MID_TURN);
        filter.accept_tick(-1, MID_TURN);

        // the two deferred ticks are folded into the first tick after the pause
        assert_eq!(filter.accept_tick(-1, AFTER_A_PAUSE), -3);
    }

    #[test]
    fn the_very_first_tick_is_always_applied() {
        let mut filter = filter_confirming_after(5);

        assert_eq!(filter.accept_tick(-1, MID_TURN), -1);
    }

    #[test]
    fn a_zero_delta_changes_nothing() {
        let mut filter = filter_confirming_after(5);
        filter.accept_tick(1, MID_TURN);

        assert_eq!(filter.accept_tick(0, MID_TURN), 0);
        assert_eq!(filter.accept_tick(1, MID_TURN), 1);
    }
}
