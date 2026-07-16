use crate::{
    any::difficulty::object::IDifficultyObject,
    osu::difficulty::object::OsuDifficultyObject,
    util::difficulty::{bpm_to_milliseconds, milliseconds_to_bpm},
};

pub struct SpeedEvaluator;

impl SpeedEvaluator {
    pub fn evaluate_diff_of<'a>(
        curr: &'a OsuDifficultyObject<'a>,
        objects: &'a [OsuDifficultyObject<'a>],
        hit_window: f64,
    ) -> f64 {
        if curr.base.is_spinner() {
            return 0.0;
        }

        let mut strain_time = curr.adjusted_delta_time;
        let doubletap_feasibility =
            1.0 - curr.doubletap_feasibility(curr.next(0, objects), hit_window);
        strain_time /= ((strain_time / hit_window) / 0.93).clamp(0.92, 1.0);

        let speed_bonus = if milliseconds_to_bpm(strain_time, None) > 200.0 {
            0.75 * ((bpm_to_milliseconds(200.0, None) - strain_time) / 40.0).powi(2)
        } else {
            0.0
        };

        let difficulty = (1.0 + speed_bonus) * 1000.0 / strain_time;
        let high_bpm_bonus = (1.0 - 0.3_f64.powf(curr.adjusted_delta_time / 1000.0)).recip();

        difficulty * high_bpm_bonus * doubletap_feasibility
    }
}
