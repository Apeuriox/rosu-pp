use crate::{
    GameMods,
    any::difficulty::{
        object::{HasStartTime, IDifficultyObject},
        skills::strain_decay,
    },
    osu::difficulty::{evaluators::FlashlightEvaluator, object::OsuDifficultyObject},
    util::{difficulty::reverse_lerp, traits::IEnumerable},
};

define_skill! {
    pub struct Flashlight: StrainSkill => [OsuDifficultyObject<'a>][OsuDifficultyObject<'a>] {
        current_strain: f64,
        has_flashlight_mod: bool,
        has_hidden_mod: bool,
        hidden_objects: bool,
        has_touch_device: bool,
        has_relax: bool,
        has_autopilot: bool,
        magnetised_strength: Option<f64>,
        deflate_start_scale: Option<f64>,
        total_objects: usize,
        evaluator: FlashlightEvaluator,
    }

    pub fn new(
        mods: &GameMods,
        radius: f64,
        time_preempt: f64,
        time_fade_in: f64,
        total_objects: usize
    ) -> Self {
        let scaling_factor = 52.0 / radius;

        Self {
            current_strain: 0.0,
            has_flashlight_mod: mods.fl(),
            has_hidden_mod: mods.hd(),
            hidden_objects: mods.hd() && !mods.hd_only_fade_approach_circles().unwrap_or(false),
            has_touch_device: mods.td(),
            has_relax: mods.rx(),
            has_autopilot: mods.ap(),
            magnetised_strength: mods.attraction_strength(),
            deflate_start_scale: mods.deflate_start_scale(),
            total_objects: total_objects,
            evaluator: FlashlightEvaluator::new(scaling_factor, time_preempt, time_fade_in),
        }
    }
}

impl Flashlight {
    const SKILL_MULTIPLIER: f64 = 0.058;
    const STRAIN_DECAY_BASE: f64 = 0.15;

    fn calculate_initial_strain(
        &mut self,
        time: f64,
        curr: &OsuDifficultyObject<'_>,
        objects: &[OsuDifficultyObject<'_>],
    ) -> f64 {
        let prev_start_time = curr
            .previous(0, objects)
            .map_or(0.0, HasStartTime::start_time);

        self.current_strain * strain_decay(time - prev_start_time, Self::STRAIN_DECAY_BASE)
    }

    fn strain_value_at(
        &mut self,
        curr: &OsuDifficultyObject<'_>,
        objects: &[OsuDifficultyObject<'_>],
    ) -> f64 {
        if !self.has_flashlight_mod {
            return 0.0;
        }

        self.current_strain *= strain_decay(curr.delta_time, Self::STRAIN_DECAY_BASE);
        let mut difficulty = self
            .evaluator
            .evaluate_diff_of(curr, objects, self.hidden_objects, self.has_hidden_mod);

        if self.has_touch_device {
            difficulty = difficulty.powf(0.9);
        }

        if let Some(strength) = self.magnetised_strength {
            difficulty *= 1.0 - strength;
        }

        if let Some(scale) = self.deflate_start_scale {
            difficulty *= reverse_lerp(scale, 11.0, 1.0).clamp(0.1, 1.0);
        }

        if self.has_relax {
            difficulty *= 0.7;
        }

        if self.has_autopilot {
            difficulty *= 0.4;
        }

        difficulty *= 0.985 + curr.overall_difficulty.max(0.0).powi(2) / 4000.0;
        self.current_strain += difficulty * Self::SKILL_MULTIPLIER;

        self.current_strain
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "function definition needs to stay in-sync with `StrainSkill::difficulty_value`"
    )]
    fn difficulty_value(current_strain_peaks: Vec<f64>) -> f64 {
        current_strain_peaks.cs_sum()
    }

    pub fn difficulty_to_performance(difficulty: f64) -> f64 {
        25.0 * f64::powf(difficulty, 2.0)
    }

    pub fn current_difficulty_value(&self) -> f64 {
        let peaks = <Self as crate::any::difficulty::skills::StrainSkill>::get_current_strain_peaks(
            self.strain_skill_strain_peaks.clone(),
            self.strain_skill_current_section_peak,
        );
        let mut sum: f64 = peaks.into_iter().sum();
        let total = self.total_objects as f64;
        sum *= 0.7
            + 0.1 * (total / 200.0).min(1.0)
            + if self.total_objects > 200 {
                0.2 * ((total - 200.0) / 200.0).min(1.0)
            } else {
                0.0
            };

        sum
    }
}
