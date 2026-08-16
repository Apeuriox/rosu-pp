use std::f64::consts::PI;

use crate::{
    any::difficulty::object::IDifficultyObject,
    osu::difficulty::object::OsuDifficultyObject,
    util::difficulty::{norm, reverse_lerp, smootherstep},
};

pub struct ReadingEvaluator;

impl ReadingEvaluator {
    const WINDOW_SIZE: f64 = 3000.0;
    const DIST_INFLUENCE_THRESHOLD: f64 = OsuDifficultyObject::NORMALIZED_DIAMETER as f64 * 1.5;

    pub fn evaluate_diff_of<'a>(
        curr: &'a OsuDifficultyObject<'a>,
        objects: &'a [OsuDifficultyObject<'a>],
        hidden: bool,
    ) -> f64 {
        if curr.base.is_spinner() || curr.idx == 0 {
            return 0.0;
        }

        let velocity = (curr.lazy_jump_dist / curr.adjusted_delta_time).max(1.0);
        let visible_density = Self::current_visible_object_density(curr, objects);
        let past_influence = Self::past_object_difficulty_influence(curr, objects);
        let angle_nerf = Self::constant_angle_nerf_factor(curr, objects);
        let future_influence = curr
            .next(0, objects)
            .map_or(visible_density.sqrt(), |next| {
                visible_density.sqrt()
                    * smootherstep(next.lazy_jump_dist, 15.0, Self::DIST_INFLUENCE_THRESHOLD)
            });
        let density_base =
            (past_influence + future_influence).powf(1.7) * 0.4 * angle_nerf * velocity;
        let density = (density_base - 2.5).max(0.0).powf(0.45) * 2.4;
        let preempt_base =
            ((500.0 - curr.preempt + (curr.preempt - 500.0).abs()) / 2.0).powf(2.5) / 140_000.0;
        let preempt = preempt_base * angle_nerf * velocity;
        let hidden = if hidden {
            Self::hidden_difficulty(
                curr,
                objects,
                past_influence,
                visible_density,
                velocity,
                angle_nerf,
            )
        } else {
            0.0
        };
        let difficulty = norm(1.5, [preempt, hidden, density]);

        difficulty * (1.0 - 0.8_f64.powf(curr.adjusted_delta_time / 1000.0)).recip()
    }

    fn hidden_difficulty(
        curr: &OsuDifficultyObject<'_>,
        objects: &[OsuDifficultyObject<'_>],
        past_influence: f64,
        visible_density: f64,
        velocity: f64,
        angle_nerf: f64,
    ) -> f64 {
        let preempt_factor = curr.preempt.powf(2.2) * 0.01;
        let density_factor = (visible_density + past_influence).powf(3.3) * 3.0;
        let mut difficulty =
            ((preempt_factor + density_factor) * angle_nerf * velocity * 0.01).powf(0.4) * 0.28;

        if let Some(previous) = curr.previous(0, objects) {
            if curr.lazy_jump_dist == 0.0
                && curr.opacity_at_adjusted(previous.start_time, true) == 0.0
                && previous.start_time > curr.start_time - curr.preempt
            {
                difficulty += 0.28 * 2500.0 / curr.adjusted_delta_time.powf(1.5);
            }
        }

        difficulty
    }

    fn past_object_difficulty_influence(
        curr: &OsuDifficultyObject<'_>,
        objects: &[OsuDifficultyObject<'_>],
    ) -> f64 {
        let mut influence = 0.0;

        for idx in 0..curr.idx {
            let Some(previous) = curr.previous(idx, objects) else {
                break;
            };

            if curr.start_time - previous.start_time > Self::WINDOW_SIZE
                || previous.start_time < curr.start_time - curr.preempt
            {
                break;
            }

            let mut value = curr.opacity_at_adjusted(previous.start_time, false);
            value *= smootherstep(
                previous.lazy_jump_dist,
                15.0,
                Self::DIST_INFLUENCE_THRESHOLD,
            );
            value *= Self::time_nerf_factor(curr.start_time - previous.start_time);
            influence += value;
        }

        influence
    }

    fn current_visible_object_density(
        curr: &OsuDifficultyObject<'_>,
        objects: &[OsuDifficultyObject<'_>],
    ) -> f64 {
        let mut density = 0.0;
        let mut idx = 0;

        while let Some(next) = curr.next(idx, objects) {
            if next.start_time - curr.start_time > Self::WINDOW_SIZE
                || curr.start_time < next.start_time - next.preempt
            {
                break;
            }

            density += next.opacity_at_adjusted(curr.start_time, false)
                * Self::time_nerf_factor(next.start_time - curr.start_time);
            idx += 1;
        }

        density
    }

    fn constant_angle_nerf_factor(
        curr: &OsuDifficultyObject<'_>,
        objects: &[OsuDifficultyObject<'_>],
    ) -> f64 {
        let mut constant_angle_count = 0.0;
        let mut idx = 0;
        let mut current_time_gap = 0.0;
        let mut prev0 = curr;
        let mut prev1: Option<&OsuDifficultyObject<'_>> = None;
        let mut prev2: Option<&OsuDifficultyObject<'_>> = None;

        while current_time_gap < 2000.0 {
            let Some(loop_obj) = curr.previous(idx, objects) else {
                break;
            };
            let long_interval_factor =
                1.0 - reverse_lerp(loop_obj.adjusted_delta_time, 200.0, 2000.0);

            if let Some((curr_angle, loop_angle)) = curr.angle.zip(loop_obj.angle) {
                let angle_diff = (curr_angle - loop_angle).abs();
                let mut alternating_diff = PI;

                if let (Some(prev0_angle), Some(prev1_angle), Some(prev2_angle)) = (
                    prev0.angle,
                    prev1.and_then(|obj| obj.angle),
                    prev2.and_then(|obj| obj.angle),
                ) {
                    alternating_diff =
                        (prev1_angle - loop_angle).abs() + (prev2_angle - prev0_angle).abs();
                    let mut weight =
                        reverse_lerp(loop_angle.min(prev0_angle).to_degrees(), 20.0, 5.0);
                    weight *= reverse_lerp(loop_angle.max(prev0_angle).to_degrees(), 60.0, 120.0);
                    alternating_diff = PI + (0.1 * alternating_diff - PI) * weight;
                }

                let stack_factor = smootherstep(
                    loop_obj.lazy_jump_dist,
                    0.0,
                    f64::from(OsuDifficultyObject::NORMALIZED_RADIUS),
                );
                constant_angle_count += (3.0
                    * 30_f64
                        .to_radians()
                        .min(angle_diff.min(alternating_diff) * stack_factor))
                .cos()
                    * long_interval_factor;
            }

            current_time_gap = curr.start_time - loop_obj.start_time;
            idx += 1;
            prev2 = prev1;
            prev1 = Some(prev0);
            prev0 = loop_obj;
        }

        (2.0 / constant_angle_count).clamp(0.2, 1.0)
    }

    fn time_nerf_factor(delta_time: f64) -> f64 {
        (2.0 - delta_time / (Self::WINDOW_SIZE / 2.0)).clamp(0.0, 1.0)
    }
}
