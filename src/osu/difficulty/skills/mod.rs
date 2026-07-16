use crate::{any::difficulty::skills::StrainSkill, model::mods::GameMods, osu::object::OsuObject};

use self::{aim::Aim, flashlight::Flashlight, reading::Reading, speed::Speed};

use super::{
    HD_FADE_IN_DURATION_MULTIPLIER, object::OsuDifficultyObject, scaling_factor::ScalingFactor,
};

pub mod aim;
pub mod flashlight;
pub mod reading;
pub mod speed;
pub mod strain;

pub struct OsuSkills {
    pub aim: Aim,
    pub aim_no_sliders: Aim,
    pub speed: Speed,
    pub reading: Reading,
    pub flashlight: Flashlight,
}

impl OsuSkills {
    pub fn new(
        mods: &GameMods,
        scaling_factor: &ScalingFactor,
        great_hit_window: f64,
        time_preempt: f64,
        total_objects: usize,
    ) -> Self {
        let hit_window = 2.0 * great_hit_window;

        // * Preempt time can go below 450ms. Normally, this is achieved via the DT mod
        // * which uniformly speeds up all animations game wide regardless of AR.
        // * This uniform speedup is hard to match 1:1, however we can at least make
        // * AR>10 (via mods) feel good by extending the upper linear function above.
        // * Note that this doesn't exactly match the AR>10 visuals as they're
        // * classically known, but it feels good.
        // * This adjustment is necessary for AR>10, otherwise TimePreempt can
        // * become smaller leading to hitcircles not fully fading in.
        let time_fade_in = if mods.hd() {
            time_preempt * HD_FADE_IN_DURATION_MULTIPLIER
        } else {
            400.0 * (time_preempt / OsuObject::PREEMPT_MIN).min(1.0)
        };

        let aim = Aim::new(mods, true);
        let aim_no_sliders = Aim::new(mods, false);
        let speed = Speed::new(mods, hit_window);
        let reading = Reading::new(mods);
        let flashlight = Flashlight::new(
            mods,
            scaling_factor.radius,
            time_preempt,
            time_fade_in,
            total_objects,
        );

        Self {
            aim,
            aim_no_sliders,
            speed,
            reading,
            flashlight,
        }
    }

    pub fn process(&mut self, curr: &OsuDifficultyObject<'_>, objects: &[OsuDifficultyObject<'_>]) {
        self.aim.process(curr, objects);
        self.aim_no_sliders.process(curr, objects);
        self.speed.process(curr, objects);
        self.reading.process(curr, objects);
        self.flashlight.process(curr, objects);
    }
}
