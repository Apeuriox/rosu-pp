use crate::{
    any::difficulty::object::IDifficultyObject,
    osu::difficulty::object::OsuDifficultyObject,
    util::difficulty::{
        milliseconds_to_bpm, reverse_lerp, smootherstep, smoothstep,
    },
};

pub struct SnapAimEvaluator;

impl SnapAimEvaluator {
    const WIDE_ANGLE_MULTIPLIER: f64 = 9.67;
    const ACUTE_ANGLE_MULTIPLIER: f64 = 2.41;
    const SLIDER_MULTIPLIER: f64 = 1.5;
    const VELOCITY_CHANGE_MULTIPLIER: f64 = 0.9;
    const WIGGLE_MULTIPLIER: f64 = 1.02;

    #[expect(clippy::too_many_lines, reason = "staying in-sync with lazer")]
    pub fn evaluate_diff_of<'a>(
        curr: &'a OsuDifficultyObject<'a>,
        objects: &'a [OsuDifficultyObject<'a>],
        with_slider_travel_dist: bool,
    ) -> f64 {
        if curr.base.is_spinner() || curr.idx <= 1 {
            return 0.0;
        }

        let Some(last) = curr.previous(0, objects) else {
            return 0.0;
        };

        if last.base.is_spinner() {
            return 0.0;
        }

        let last2 = curr.previous(2, objects);
        let radius = f64::from(OsuDifficultyObject::NORMALIZED_RADIUS);
        let diameter = f64::from(OsuDifficultyObject::NORMALIZED_DIAMETER);

        let curr_dist = if with_slider_travel_dist {
            curr.lazy_jump_dist
        } else {
            curr.jump_dist
        };
        let mut curr_vel = curr_dist / curr.adjusted_delta_time;

        if last.base.is_slider() && with_slider_travel_dist {
            let slider_dist = last.lazy_travel_dist + curr.lazy_jump_dist;
            curr_vel = curr_vel.max(slider_dist / curr.adjusted_delta_time);
        }

        let prev_dist = if with_slider_travel_dist {
            last.lazy_jump_dist
        } else {
            last.jump_dist
        };
        let prev_vel = prev_dist / last.adjusted_delta_time;
        let mut difficulty = curr_vel * Self::vector_angle_repetition(curr, last, objects);

        if let Some((curr_angle, last_angle)) = curr.angle.zip(last.angle) {
            let vel_influence = curr_vel.min(prev_vel);
            let mut acute_bonus = 0.0;

            if curr.adjusted_delta_time.max(last.adjusted_delta_time)
                < 1.25 * curr.adjusted_delta_time.min(last.adjusted_delta_time)
            {
                acute_bonus = Self::calc_angle_acuteness(curr_angle);
                acute_bonus *= 0.08
                    + 0.92
                        * (1.0
                            - acute_bonus
                                .min(Self::calc_angle_acuteness(last_angle).powi(3)));
                acute_bonus *= vel_influence
                    * smootherstep(
                        milliseconds_to_bpm(curr.adjusted_delta_time, Some(2)),
                        300.0,
                        400.0,
                    )
                    * smootherstep(curr_dist, 0.0, diameter * 2.0);
            }

            let mut wide_bonus = Self::calc_angle_wideness(curr_angle);
            wide_bonus *= 0.25
                + 0.75
                    * (1.0
                        - wide_bonus.min(Self::calc_angle_wideness(last_angle).powi(3)));

            const WIDE_ANGLE_TIME_SCALE: f64 = 1.45;
            let mut wide_curr_vel = curr_dist / curr.adjusted_delta_time.powf(WIDE_ANGLE_TIME_SCALE);
            let wide_prev_vel = prev_dist / last.adjusted_delta_time.powf(WIDE_ANGLE_TIME_SCALE);

            if last.base.is_slider() && with_slider_travel_dist {
                let slider_dist = last.lazy_travel_dist + curr.lazy_jump_dist;
                wide_curr_vel = wide_curr_vel
                    .max(slider_dist / curr.adjusted_delta_time.powf(WIDE_ANGLE_TIME_SCALE));
            }

            wide_bonus *= wide_curr_vel.min(wide_prev_vel);

            if let Some(last2) = last2 {
                let dist = (last2.base.stacked_pos() - last.base.stacked_pos()).length();

                if dist < 1.0 {
                    wide_bonus *= 1.0 - 0.55 * f64::from(1.0 - dist);
                }
            }

            difficulty += (acute_bonus * Self::ACUTE_ANGLE_MULTIPLIER)
                .max(wide_bonus * Self::WIDE_ANGLE_MULTIPLIER);

            let wiggle_bonus = vel_influence
                * smootherstep(curr_dist, radius, diameter)
                * reverse_lerp(curr_dist, diameter * 3.0, diameter).powf(1.8)
                * smootherstep(curr_angle, 110_f64.to_radians(), 60_f64.to_radians())
                * smootherstep(prev_dist, radius, diameter)
                * reverse_lerp(prev_dist, diameter * 3.0, diameter).powf(1.8)
                * smootherstep(last_angle, 110_f64.to_radians(), 60_f64.to_radians());

            difficulty += wiggle_bonus * Self::WIGGLE_MULTIPLIER;
        }

        if prev_vel.max(curr_vel) != 0.0 {
            if with_slider_travel_dist {
                curr_vel = curr_dist / curr.adjusted_delta_time;
            }

            let dist_ratio = smoothstep(
                (prev_vel - curr_vel).abs() / prev_vel.max(curr_vel),
                0.0,
                1.0,
            );
            let overlap_vel_buff = (diameter * 1.25
                / curr.adjusted_delta_time.min(last.adjusted_delta_time))
            .min((prev_vel - curr_vel).abs());
            let mut vel_change_bonus = overlap_vel_buff * dist_ratio;
            vel_change_bonus *= (curr.adjusted_delta_time.min(last.adjusted_delta_time)
                / curr.adjusted_delta_time.max(last.adjusted_delta_time))
            .powi(2);
            difficulty += vel_change_bonus * Self::VELOCITY_CHANGE_MULTIPLIER;
        }

        if curr.base.is_slider() && with_slider_travel_dist {
            let slider_bonus = curr.travel_dist / curr.travel_time;
            difficulty += if slider_bonus < 1.0 {
                slider_bonus
            } else {
                slider_bonus.powf(0.75)
            } * Self::SLIDER_MULTIPLIER;
        }

        difficulty *= curr.small_circle_bonus;
        difficulty *= Self::high_bpm_bonus(curr.adjusted_delta_time);

        difficulty
    }

    fn high_bpm_bonus(ms: f64) -> f64 {
        (1.0 - 0.03_f64.powf((ms / 1000.0).powf(0.65))).recip()
    }

    fn vector_angle_repetition(
        curr: &OsuDifficultyObject<'_>,
        last: &OsuDifficultyObject<'_>,
        objects: &[OsuDifficultyObject<'_>],
    ) -> f64 {
        let Some((curr_angle, last_angle)) = curr.angle.zip(last.angle) else {
            return 1.0;
        };

        let mut constant_angle_count = 0.0;

        for idx in 0..6 {
            let Some(prev) = curr.previous(idx, objects) else {
                break;
            };

            if curr.adjusted_delta_time.max(prev.adjusted_delta_time)
                > 1.1 * curr.adjusted_delta_time.min(prev.adjusted_delta_time)
            {
                break;
            }

            if let Some((prev_vector, curr_vector)) = prev
                .normalised_vector_angle
                .zip(curr.normalised_vector_angle)
            {
                let angle_diff = (curr_vector - prev_vector).abs();
                constant_angle_count +=
                    (8.0 * 11.25_f64.to_radians().min(angle_diff)).cos();
            }
        }

        let vector_repetition = (0.5 / constant_angle_count).min(1.0).powi(2);
        let stack_factor = smootherstep(
            curr.lazy_jump_dist,
            0.0,
            f64::from(OsuDifficultyObject::NORMALIZED_DIAMETER),
        );
        let angle_diff_adjusted =
            (2.0 * 45_f64.to_radians().min((curr_angle - last_angle).abs() * stack_factor))
                .cos();
        let base_nerf =
            1.0 - 0.15 * Self::calc_angle_acuteness(last_angle) * angle_diff_adjusted;

        (base_nerf + (1.0 - base_nerf) * vector_repetition * 0.5 * stack_factor).powi(2)
    }

    const fn calc_angle_wideness(angle: f64) -> f64 {
        smoothstep(angle, 40_f64.to_radians(), 140_f64.to_radians())
    }

    pub const fn calc_angle_acuteness(angle: f64) -> f64 {
        smoothstep(angle, 140_f64.to_radians(), 40_f64.to_radians())
    }
}

pub struct AgilityEvaluator;

impl AgilityEvaluator {
    pub fn evaluate_diff_of(
        curr: &OsuDifficultyObject<'_>,
        objects: &[OsuDifficultyObject<'_>],
    ) -> f64 {
        if curr.base.is_spinner() {
            return 0.0;
        }

        let travel_dist = curr
            .previous(0, objects)
            .map_or(0.0, |prev| prev.lazy_travel_dist);
        let dist_cap = f64::from(OsuDifficultyObject::NORMALIZED_DIAMETER) * 1.2;
        let dist_scaled = (travel_dist + curr.lazy_jump_dist).min(dist_cap) / dist_cap;
        let mut difficulty = dist_scaled * 1000.0 / curr.adjusted_delta_time;
        difficulty *= curr.small_circle_bonus.powf(1.5);
        difficulty *= (1.0 - 0.2_f64.powf(curr.adjusted_delta_time / 1000.0)).recip();

        difficulty
    }
}

pub struct FlowAimEvaluator;

impl FlowAimEvaluator {
    pub fn evaluate_diff_of<'a>(
        curr: &'a OsuDifficultyObject<'a>,
        objects: &'a [OsuDifficultyObject<'a>],
        with_slider_travel_dist: bool,
    ) -> f64 {
        if curr.base.is_spinner() || curr.idx <= 1 {
            return 0.0;
        }

        let Some(last) = curr.previous(0, objects) else {
            return 0.0;
        };

        if last.base.is_spinner() {
            return 0.0;
        }

        let Some(last_last) = curr.previous(1, objects) else {
            return 0.0;
        };

        let curr_dist = if with_slider_travel_dist {
            curr.lazy_jump_dist
        } else {
            curr.jump_dist
        };
        let prev_dist = if with_slider_travel_dist {
            last.lazy_jump_dist
        } else {
            last.jump_dist
        };
        let mut curr_vel = curr_dist / curr.adjusted_delta_time;

        if last.base.is_slider() && with_slider_travel_dist {
            let slider_dist = last.lazy_travel_dist + curr.lazy_jump_dist;
            curr_vel = curr_vel.max(slider_dist / curr.adjusted_delta_time);
        }

        let prev_vel = prev_dist / last.adjusted_delta_time;
        let mut difficulty = curr_vel * curr.small_circle_bonus.sqrt();
        difficulty *= 1.0
            + 0.25_f64.min(
                ((curr.adjusted_delta_time.max(last.adjusted_delta_time)
                    - curr.adjusted_delta_time.min(last.adjusted_delta_time))
                    / 50.0)
                    .powi(4),
            );

        let overlap_weight = if curr.idx > 2 {
            1.0 - Self::overlap_factor(curr, last)
                * Self::overlap_factor(curr, last_last)
                * Self::overlap_factor(last, last_last)
        } else {
            1.0
        };

        if let Some((curr_angle, last_angle)) = curr.angle.zip(last.angle) {
            let angle_diff = (curr_angle - last_angle).abs();
            let angular_vel = (angle_diff / 2.0).sin() * 180.0
                / (curr.adjusted_delta_time * 0.1);
            difficulty *= 0.8 + (angular_vel / 270.0).sqrt();
        }

        if let Some(curr_angle) = curr.angle {
            difficulty += curr_vel
                * SnapAimEvaluator::calc_angle_acuteness(curr_angle)
                * overlap_weight;
        }

        if prev_vel.max(curr_vel) != 0.0 {
            if with_slider_travel_dist {
                curr_vel = curr_dist / curr.adjusted_delta_time;
            }

            let dist_ratio = smoothstep(
                (prev_vel - curr_vel).abs() / prev_vel.max(curr_vel),
                0.0,
                1.0,
            );
            let overlap_vel_buff =
                (f64::from(OsuDifficultyObject::NORMALIZED_DIAMETER) * 1.25
                    / curr.adjusted_delta_time.min(last.adjusted_delta_time))
                .min((prev_vel - curr_vel).abs());
            difficulty += overlap_vel_buff * dist_ratio * overlap_weight * 0.52;
        }

        if curr.base.is_slider() && with_slider_travel_dist {
            difficulty += curr.travel_dist / curr.travel_time;
        }

        difficulty = difficulty.powf(1.45);

        difficulty
            * smootherstep(
                curr_dist,
                0.0,
                f64::from(OsuDifficultyObject::NORMALIZED_RADIUS),
            )
    }

    fn overlap_factor(first: &OsuDifficultyObject<'_>, second: &OsuDifficultyObject<'_>) -> f64 {
        let radius = first.radius;
        let dist = (first.base.stacked_pos() - second.base.stacked_pos()).length();

        (1.0 - ((f64::from(dist) - radius).max(0.0) / radius).powi(2)).clamp(0.0, 1.0)
    }
}
