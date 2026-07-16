use crate::{
    any::difficulty::object::IDifficultyObject,
    osu::difficulty::object::OsuDifficultyObject,
    util::difficulty::{logistic, reverse_lerp, smoothstep_bell_curve},
};

pub struct RhythmEvaluator;

impl RhythmEvaluator {
    #[expect(clippy::too_many_lines, reason = "staying in-sync with lazer")]
    pub fn evaluate_diff_of<'a>(
        curr: &'a OsuDifficultyObject<'a>,
        objects: &'a [OsuDifficultyObject<'a>],
        hit_window: f64,
    ) -> f64 {
        if curr.base.is_spinner() {
            return 0.0;
        }

        let mut complexity = 0.0;
        let epsilon = hit_window * 0.3;
        let mut island = Island::new(i32::MAX);
        let mut previous_island = Island::new(i32::MAX);
        let mut islands = Vec::<Island>::new();
        let mut start_difficulty = 0.0;
        let mut first_delta_switch = false;
        let historical_note_count = curr.idx.min(32);
        let mut rhythm_start = 0;

        while rhythm_start + 2 < historical_note_count
            && curr
                .previous(rhythm_start, objects)
                .is_some_and(|prev| curr.start_time - prev.start_time < 5000.0)
        {
            rhythm_start += 1;
        }

        let Some(mut prev_obj) = curr.previous(rhythm_start, objects) else {
            return 1.0;
        };
        let Some(mut prev_prev_obj) = curr.previous(rhythm_start + 1, objects) else {
            return 1.0;
        };

        for idx in (1..=rhythm_start).rev() {
            let Some(curr_obj) = curr.previous(idx - 1, objects) else {
                break;
            };

            if curr_obj.base.is_spinner() {
                continue;
            }

            let time_decay = (5000.0 - (curr.start_time - curr_obj.start_time)) / 5000.0;
            let note_decay = (historical_note_count - idx) as f64 / historical_note_count as f64;
            let historical_decay = note_decay.min(time_decay);
            let curr_delta = curr_obj.delta_time.max(1e-7);
            let prev_delta = prev_obj.delta_time.max(1e-7);
            let delta_diff = (prev_delta - curr_delta).abs();

            if island.delta == i32::MAX {
                island = Island::new(curr_delta as i32);
            }

            let ratio = prev_delta.max(curr_delta) / prev_delta.min(curr_delta);
            let difference_multiplier = (2.0 - ratio / 8.0).clamp(0.0, 1.0);
            let window_penalty = ((delta_diff - epsilon) / epsilon).clamp(0.0, 1.0);
            let mut effective =
                Self::effective_difficulty(ratio) * window_penalty * difference_multiplier;

            if prev_obj.base.is_slider() {
                let lazy_ratio = curr_obj.min_jump_time.max(curr_delta)
                    / curr_obj.min_jump_time.min(curr_delta);
                let real_ratio = curr_obj.last_object_end_delta_time.max(curr_delta)
                    / curr_obj.last_object_end_delta_time.min(curr_delta);
                effective = effective.min(
                    Self::effective_difficulty(lazy_ratio)
                        .min(Self::effective_difficulty(real_ratio)),
                );
            }

            if delta_diff < epsilon {
                island.add_delta(curr_delta as i32);
            }

            if first_delta_switch {
                if delta_diff > epsilon {
                    if curr_obj.base.is_slider() {
                        effective *= 0.5;
                    }

                    if island.is_similar_polarity(&previous_island, epsilon) {
                        effective *= 0.5;
                    }

                    if prev_prev_obj.delta_time.max(1e-7) > prev_delta + epsilon
                        && prev_delta > curr_delta + epsilon
                    {
                        effective *= 0.125;
                    }

                    if previous_island.delta_count == island.delta_count {
                        effective *= 0.5;
                    }

                    if prev_delta > curr_delta + epsilon {
                        effective *= 0.65;
                    }

                    if let Some(existing) = islands
                        .iter_mut()
                        .find(|existing| existing.almost_equals(&island, epsilon))
                    {
                        if previous_island.almost_equals(&island, epsilon) {
                            existing.occurrences += 1;
                        }

                        let power = logistic(
                            f64::from(island.delta),
                            58.33,
                            0.24,
                            Some(2.75),
                        );
                        effective *= (3.0 / existing.occurrences as f64)
                            .min((existing.occurrences as f64).recip().powf(power));
                    } else if island.delta_count > 0 {
                        islands.push(island);
                    }

                    effective *=
                        1.0 - prev_obj.doubletap_feasibility(Some(curr_obj), hit_window) * 0.75;

                    complexity += if island.delta_count > 1 {
                        (effective * start_difficulty).sqrt() * historical_decay
                    } else {
                        0.7 * historical_decay
                    };
                    start_difficulty = effective;

                    if prev_delta + epsilon < curr_delta {
                        first_delta_switch = false;
                    }

                    previous_island = island;
                    island = Island::new(curr_delta as i32);
                }
            } else if prev_delta > curr_delta + epsilon {
                first_delta_switch = true;

                if curr_obj.base.is_slider() {
                    effective *= 0.6;
                }

                if prev_obj.base.is_slider() {
                    effective *= 0.6;
                }

                start_difficulty = effective;
                island = Island::new(curr_delta as i32);
            }

            prev_prev_obj = prev_obj;
            prev_obj = curr_obj;
        }

        complexity *= reverse_lerp(f64::from(island.delta_count), 22.0, 3.0);

        (4.0 + complexity * 0.95).sqrt() / 2.0
    }

    fn effective_difficulty(ratio: f64) -> f64 {
        let fraction = ratio - ratio.trunc();

        1.0 + 26.0 * smoothstep_bell_curve(fraction, 0.5, 0.5).min(0.5)
    }
}

#[derive(Copy, Clone)]
struct Island {
    delta: i32,
    delta_count: i32,
    occurrences: usize,
}

impl Island {
    fn new(delta: i32) -> Self {
        Self {
            delta: delta.max(OsuDifficultyObject::MIN_DELTA_TIME as i32),
            delta_count: 1,
            occurrences: 1,
        }
    }

    fn add_delta(&mut self, delta: i32) {
        if self.delta == i32::MAX {
            self.delta = delta.max(OsuDifficultyObject::MIN_DELTA_TIME as i32);
        }

        self.delta_count += 1;
    }

    fn is_similar_polarity(&self, other: &Self, epsilon: f64) -> bool {
        self.delta_count > 1
            && other.delta_count > 1
            && f64::from((self.delta - other.delta).abs()) < epsilon
            && self.delta_count % 2 == other.delta_count % 2
    }

    fn almost_equals(&self, other: &Self, epsilon: f64) -> bool {
        f64::from((self.delta - other.delta).abs()) < epsilon
            && self.delta_count == other.delta_count
    }
}
