use crate::{
    model::mods::GameMods,
    osu::difficulty::{
        evaluators::{RhythmEvaluator, SpeedEvaluator},
        object::OsuDifficultyObject,
    },
    util::float_ext::FloatExt,
};

pub struct Speed {
    current_strain: f64,
    hit_window: f64,
    has_autopilot: bool,
    has_relax: bool,
    object_difficulties: Vec<f64>,
    slider_strains: Vec<f64>,
}

impl Speed {
    pub fn new(mods: &GameMods, hit_window: f64) -> Self {
        Self {
            current_strain: 0.0,
            hit_window,
            has_autopilot: mods.ap(),
            has_relax: mods.rx(),
            object_difficulties: Vec::with_capacity(256),
            slider_strains: Vec::with_capacity(64),
        }
    }

    pub fn process(&mut self, curr: &OsuDifficultyObject<'_>, objects: &[OsuDifficultyObject<'_>]) {
        if self.has_relax {
            self.object_difficulties.push(0.0);

            return;
        }

        let decay = 0.3_f64.powf(curr.adjusted_delta_time / 1000.0);
        self.current_strain *= decay;

        let mut adjusted = SpeedEvaluator::evaluate_diff_of(curr, objects, self.hit_window);

        if self.has_autopilot {
            adjusted *= 0.5;
        }

        self.current_strain += adjusted * (1.0 - decay) * 1.16;
        let total_strain =
            self.current_strain * RhythmEvaluator::evaluate_diff_of(curr, objects, self.hit_window);

        if curr.base.is_slider() {
            self.slider_strains.push(total_strain);
        }

        self.object_difficulties.push(total_strain);
    }

    pub fn difficulty_value(&self) -> (f64, f64) {
        let mut difficulties: Vec<_> = self
            .object_difficulties
            .iter()
            .copied()
            .filter(|value| *value > 0.0)
            .collect();
        difficulties.sort_by(|a, b| b.total_cmp(a));

        let mut difficulty = 0.0;
        let mut weight_sum = 0.0;

        for (idx, value) in difficulties.into_iter().enumerate() {
            let idx = idx as f64;
            let harmonic = 20.0 / (1.0 + idx);
            let weight = (1.0 + harmonic) / (idx.powf(0.9) + 1.0 + harmonic);
            weight_sum += weight;
            difficulty += value * weight;
        }

        (difficulty, weight_sum)
    }

    pub fn count_top_weighted_object_difficulties(
        &self,
        difficulty_value: f64,
        weight_sum: f64,
    ) -> f64 {
        self.count_weighted(&self.object_difficulties, difficulty_value, weight_sum)
    }

    pub fn count_top_weighted_sliders(&self, difficulty_value: f64, weight_sum: f64) -> f64 {
        self.count_weighted(&self.slider_strains, difficulty_value, weight_sum)
    }

    fn count_weighted(&self, values: &[f64], difficulty_value: f64, weight_sum: f64) -> f64 {
        if values.is_empty() || FloatExt::eq(weight_sum, 0.0) {
            return 0.0;
        }

        let consistent_top = difficulty_value / weight_sum;

        if FloatExt::eq(consistent_top, 0.0) {
            return 0.0;
        }

        values
            .iter()
            .map(|value| 1.1 / (1.0 + (-10.0 * (value / consistent_top - 0.88)).exp()))
            .sum()
    }

    pub fn relevant_note_count(&self) -> f64 {
        let Some(max_strain) = self
            .object_difficulties
            .iter()
            .copied()
            .max_by(f64::total_cmp)
        else {
            return 0.0;
        };

        if FloatExt::eq(max_strain, 0.0) {
            return 0.0;
        }

        self.object_difficulties
            .iter()
            .map(|strain| (1.0 + (-(strain / max_strain * 12.0 - 6.0)).exp()).recip())
            .sum()
    }

    pub fn into_object_difficulties(self) -> Vec<f64> {
        self.object_difficulties
    }
}
