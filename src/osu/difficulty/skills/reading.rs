use crate::{
    model::mods::GameMods,
    osu::difficulty::{evaluators::ReadingEvaluator, object::OsuDifficultyObject},
    util::float_ext::FloatExt,
};

pub struct Reading {
    current_strain: f64,
    object_difficulties: Vec<f64>,
    object_start_times: Vec<f64>,
    has_hidden: bool,
    has_touch_device: bool,
    has_relax: bool,
    has_autopilot: bool,
    magnetised_strength: Option<f64>,
}

impl Reading {
    pub fn new(mods: &GameMods) -> Self {
        let has_hidden = mods.hd() && !mods.hd_only_fade_approach_circles().unwrap_or(false);

        Self {
            current_strain: 0.0,
            object_difficulties: Vec::with_capacity(256),
            object_start_times: Vec::with_capacity(256),
            has_hidden,
            has_touch_device: mods.td(),
            has_relax: mods.rx(),
            has_autopilot: mods.ap(),
            magnetised_strength: mods.attraction_strength(),
        }
    }

    pub fn process(
        &mut self,
        curr: &OsuDifficultyObject<'_>,
        objects: &[OsuDifficultyObject<'_>],
    ) {
        self.object_start_times.push(curr.start_time);
        let decay = 0.8_f64.powf(curr.delta_time / 1000.0);
        self.current_strain *= decay;

        let mut difficulty = ReadingEvaluator::evaluate_diff_of(curr, objects, self.has_hidden);

        if self.has_touch_device {
            difficulty = difficulty.powf(0.89);
        }

        if let Some(strength) = self.magnetised_strength {
            difficulty *= 1.0 - strength;
        }

        if self.has_relax {
            difficulty *= 0.4;
        }

        if self.has_autopilot {
            difficulty *= 0.1;
        }

        difficulty *= 0.825 + curr.overall_difficulty.max(0.0).powf(2.2) / 1125.0;
        self.current_strain += difficulty * (1.0 - decay) * 2.5;
        self.object_difficulties.push(self.current_strain);
    }

    fn transformed_difficulties(&self) -> Vec<f64> {
        let mut difficulties: Vec<_> = self
            .object_difficulties
            .iter()
            .copied()
            .filter(|value| *value > 0.0)
            .collect();

        let Some(&first) = self.object_start_times.first() else {
            return difficulties;
        };

        let reduced_note_count = self
            .object_start_times
            .iter()
            .take_while(|&&time| time <= first + 60_000.0)
            .count();

        for (idx, difficulty) in difficulties
            .iter_mut()
            .take(reduced_note_count)
            .enumerate()
        {
            let scale = (1.0 + 9.0 * idx as f64 / reduced_note_count as f64)
                .clamp(1.0, 10.0)
                .log10();
            *difficulty *= scale;
        }

        difficulties
    }

    pub fn difficulty_value(&self) -> (f64, f64) {
        let mut difficulties = self.transformed_difficulties();
        difficulties.sort_by(|a, b| b.total_cmp(a));
        let mut difficulty = 0.0;
        let mut weight_sum = 0.0;

        for (idx, value) in difficulties.into_iter().enumerate() {
            let idx = idx as f64;
            let harmonic = 1.0 / (1.0 + idx);
            let weight = (1.0 + harmonic) / (idx.powf(0.9) + 1.0 + harmonic);
            difficulty += value * weight;
            weight_sum += weight;
        }

        (difficulty, weight_sum)
    }

    pub fn count_top_weighted_object_difficulties(
        &self,
        difficulty_value: f64,
        weight_sum: f64,
    ) -> f64 {
        if self.object_difficulties.is_empty() || FloatExt::eq(weight_sum, 0.0) {
            return 0.0;
        }

        let consistent_top = difficulty_value / weight_sum;

        if FloatExt::eq(consistent_top, 0.0) {
            return 0.0;
        }

        self.object_difficulties
            .iter()
            .map(|value| 1.1 / (1.0 + (-5.0 * (value / consistent_top - 1.15)).exp()))
            .sum()
    }

    pub fn into_object_difficulties(self) -> Vec<f64> {
        self.object_difficulties
    }
}
