use std::collections::VecDeque;

use crate::{
    any::difficulty::object::IDifficultyObject,
    model::mods::GameMods,
    osu::difficulty::{
        evaluators::{AgilityEvaluator, FlowAimEvaluator, SnapAimEvaluator},
        object::OsuDifficultyObject,
    },
    util::{difficulty::norm, float_ext::FloatExt},
};

#[derive(Copy, Clone)]
struct StrainPeak {
    value: f64,
    section_len: f64,
}

impl StrainPeak {
    fn new(value: f64, section_len: f64) -> Self {
        Self {
            value,
            section_len: section_len.round(),
        }
    }
}

pub struct Aim {
    include_sliders: bool,
    current_strain: f64,
    current_section_peak: f64,
    current_section_begin: f64,
    current_section_end: f64,
    strain_peaks: Vec<StrainPeak>,
    object_difficulties: Vec<f64>,
    slider_strains: Vec<f64>,
    queued_strains: VecDeque<(f64, f64)>,
    total_len: f64,
    has_touch_device: bool,
    has_relax: bool,
    has_autopilot: bool,
    magnetised_strength: Option<f64>,
}

impl Aim {
    const DECAY_WEIGHT: f64 = 0.9;
    const MAX_SECTION_LEN: f64 = 400.0;
    const MAX_STORED_LEN: f64 = 11.0 / (1.0 - Self::DECAY_WEIGHT);
    const REDUCED_SECTION_TIME: f64 = 4000.0;
    const REDUCED_STRAIN_BASELINE: f64 = 0.727;

    pub fn new(mods: &GameMods, include_sliders: bool) -> Self {
        Self {
            include_sliders,
            current_strain: 0.0,
            current_section_peak: 0.0,
            current_section_begin: 0.0,
            current_section_end: 0.0,
            strain_peaks: Vec::with_capacity(256),
            object_difficulties: Vec::with_capacity(256),
            slider_strains: Vec::with_capacity(64),
            queued_strains: VecDeque::new(),
            total_len: 0.0,
            has_touch_device: mods.td(),
            has_relax: mods.rx(),
            has_autopilot: mods.ap(),
            magnetised_strength: mods.attraction_strength(),
        }
    }

    pub fn process(&mut self, curr: &OsuDifficultyObject<'_>, objects: &[OsuDifficultyObject<'_>]) {
        if curr.idx == 0 {
            self.current_section_begin = curr.start_time;
            self.current_section_end = self.current_section_begin + Self::MAX_SECTION_LEN;
            self.current_section_peak = self.strain_value_at(curr, objects);
            self.object_difficulties.push(self.current_section_peak);

            return;
        }

        self.backfill_peaks(curr, objects);
        let curr_strain = self.strain_value_at(curr, objects);

        if curr_strain > self.current_section_peak {
            self.queued_strains.clear();
            self.save_current_peak(curr.start_time - self.current_section_begin);
            self.current_section_begin = curr.start_time;
            self.current_section_end = self.current_section_begin + Self::MAX_SECTION_LEN;
            self.current_section_peak = curr_strain;
        } else {
            while self
                .queued_strains
                .back()
                .is_some_and(|&(strain, _)| strain < curr_strain)
            {
                self.queued_strains.pop_back();
            }

            self.queued_strains
                .push_back((curr_strain, curr.start_time));
        }

        self.object_difficulties.push(curr_strain);
    }

    fn backfill_peaks(
        &mut self,
        curr: &OsuDifficultyObject<'_>,
        objects: &[OsuDifficultyObject<'_>],
    ) {
        while curr.start_time > self.current_section_end {
            self.save_current_peak(self.current_section_end - self.current_section_begin);
            self.current_section_begin = self.current_section_end;

            if let Some((strain, start_time)) = self.queued_strains.pop_front() {
                self.current_section_end = start_time + Self::MAX_SECTION_LEN;
                self.start_new_section_from(self.current_section_begin, curr, objects);
                self.current_section_peak = self.current_section_peak.max(strain);
            } else {
                self.current_section_end = self.current_section_begin + Self::MAX_SECTION_LEN;
                self.start_new_section_from(self.current_section_begin, curr, objects);
            }
        }
    }

    fn save_current_peak(&mut self, section_len: f64) {
        let peak = StrainPeak::new(self.current_section_peak, section_len);
        let idx = self
            .strain_peaks
            .partition_point(|other| other.value >= peak.value);
        self.strain_peaks.insert(idx, peak);
        self.total_len += peak.section_len;

        while self.total_len > Self::MAX_STORED_LEN * Self::MAX_SECTION_LEN {
            if let Some(removed) = self.strain_peaks.pop() {
                self.total_len -= removed.section_len;
            } else {
                break;
            }
        }
    }

    fn start_new_section_from(
        &mut self,
        time: f64,
        curr: &OsuDifficultyObject<'_>,
        objects: &[OsuDifficultyObject<'_>],
    ) {
        let prev_start_time = curr
            .previous(0, objects)
            .map_or(0.0, |prev| prev.start_time);
        self.current_section_peak =
            self.current_strain * 0.2_f64.powf((time - prev_start_time) / 1000.0);
    }

    fn strain_value_at(
        &mut self,
        curr: &OsuDifficultyObject<'_>,
        objects: &[OsuDifficultyObject<'_>],
    ) -> f64 {
        if self.has_autopilot {
            return 0.0;
        }

        let decay = 0.2_f64.powf(curr.adjusted_delta_time / 1000.0);
        self.current_strain *= decay;
        self.current_strain += self.calculate_adjusted_difficulty(curr, objects) * (1.0 - decay);

        if curr.base.is_slider() {
            self.slider_strains.push(self.current_strain);
        }

        self.current_strain
    }

    fn calculate_adjusted_difficulty(
        &self,
        curr: &OsuDifficultyObject<'_>,
        objects: &[OsuDifficultyObject<'_>],
    ) -> f64 {
        let snap = SnapAimEvaluator::evaluate_diff_of(curr, objects, self.include_sliders) * 70.9;
        let agility = AgilityEvaluator::evaluate_diff_of(curr, objects) * 2.35;
        let mut flow =
            FlowAimEvaluator::evaluate_diff_of(curr, objects, self.include_sliders) * 242.0;
        let mut combined_snap = norm(1.2, [snap, agility]);
        let ratio = flow / combined_snap;
        let p_snap = if FloatExt::eq(ratio, 0.0) {
            0.0
        } else if ratio.is_nan() {
            1.0
        } else {
            1.0 / (1.0 + (-7.27 * ratio.ln()).exp())
        };

        if self.has_touch_device {
            combined_snap = norm(1.2, [snap.powf(0.89), agility]);
        }

        if self.has_relax {
            combined_snap *= 0.75;
            flow *= 0.6;
        }

        let mut difficulty = 1.12 * (combined_snap * p_snap + flow * (1.0 - p_snap));

        if let Some(strength) = self.magnetised_strength {
            difficulty *= 1.0 - strength;
        }

        difficulty * (0.985 + curr.overall_difficulty.max(0.0).powi(2) / 4000.0)
    }

    fn current_strain_peaks(&self) -> Vec<StrainPeak> {
        let mut peaks = self.strain_peaks.clone();
        let current = StrainPeak::new(
            self.current_section_peak,
            self.current_section_end - self.current_section_begin,
        );
        let idx = peaks.partition_point(|other| other.value >= current.value);
        peaks.insert(idx, current);

        peaks
    }

    fn reduced_strain_peaks(&self) -> Vec<StrainPeak> {
        let mut strains: Vec<_> = self
            .current_strain_peaks()
            .into_iter()
            .filter(|peak| peak.value > 0.0)
            .collect();
        let mut time = 0.0;
        let mut skip = 0;

        while skip < strains.len() && time < Self::REDUCED_SECTION_TIME {
            let strain = strains[skip];
            let mut added_time = 0.0;

            while added_time < strain.section_len {
                let scale = (1.0
                    + 9.0 * ((time + added_time) / Self::REDUCED_SECTION_TIME).clamp(0.0, 1.0))
                .log10();
                strains.push(StrainPeak::new(
                    strain.value
                        * (Self::REDUCED_STRAIN_BASELINE
                            + (1.0 - Self::REDUCED_STRAIN_BASELINE) * scale),
                    20.0_f64.min(strain.section_len - added_time),
                ));
                added_time += 20.0;
            }

            time += strain.section_len;
            skip += 1;
        }

        let mut reduced = strains.split_off(skip);
        reduced.sort_by(|a, b| b.value.total_cmp(&a.value));

        reduced
    }

    pub fn difficulty_value(&self) -> f64 {
        let mut difficulty = 0.0;
        let mut time = 0.0;

        for strain in self.reduced_strain_peaks() {
            let start_time = time;
            let end_time = time + strain.section_len / Self::MAX_SECTION_LEN;
            let weight = Self::DECAY_WEIGHT.powf(start_time) - Self::DECAY_WEIGHT.powf(end_time);
            difficulty += strain.value * weight;
            time = end_time;
        }

        difficulty / (1.0 - Self::DECAY_WEIGHT)
    }

    pub fn count_top_weighted_strains(&self, difficulty_value: f64) -> f64 {
        if self.object_difficulties.is_empty() {
            return 0.0;
        }

        let consistent_top = difficulty_value * (1.0 - Self::DECAY_WEIGHT);

        if FloatExt::eq(consistent_top, 0.0) {
            return self.object_difficulties.len() as f64;
        }

        self.object_difficulties
            .iter()
            .map(|strain| 1.1 / (1.0 + (-10.0 * (strain / consistent_top - 0.88)).exp()))
            .sum()
    }

    pub fn count_top_weighted_sliders(&self, difficulty_value: f64) -> f64 {
        if self.slider_strains.is_empty() {
            return 0.0;
        }

        let consistent_top = difficulty_value * (1.0 - Self::DECAY_WEIGHT);

        if FloatExt::eq(consistent_top, 0.0) {
            return 0.0;
        }

        self.slider_strains
            .iter()
            .map(|strain| 1.1 / (1.0 + (-10.0 * (strain / consistent_top - 0.88)).exp()))
            .sum()
    }

    pub fn get_difficult_sliders(&self) -> f64 {
        let Some(max_strain) = self.slider_strains.iter().copied().max_by(f64::total_cmp) else {
            return 0.0;
        };

        if FloatExt::eq(max_strain, 0.0) {
            return 0.0;
        }

        self.slider_strains
            .iter()
            .map(|strain| (1.0 + (-(strain / max_strain * 12.0 - 6.0)).exp()).recip())
            .sum()
    }

    pub fn into_current_strain_peaks(self) -> Vec<f64> {
        self.current_strain_peaks()
            .into_iter()
            .map(|peak| peak.value)
            .collect()
    }
}
