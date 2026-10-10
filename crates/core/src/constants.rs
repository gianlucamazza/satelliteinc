/// Earth gravitational parameter (km³/s²).
pub const MU_EARTH: f64 = 398_600.441_8;

/// Mean Earth equatorial radius (km).
pub const EARTH_RADIUS_KM: f64 = 6_371.0;

/// Earth sidereal rotation rate (rad/s).
pub const EARTH_ROTATION_RAD_S: f64 = 7.292_115_9e-5;

/// Seconds per sim-day.
pub const SECONDS_PER_DAY: f64 = 86_400.0;

/// Astronomical unit (km) — solar flux normalizes to 1.0 here.
pub const AU_KM: f64 = 1.495_978_707e8;

/// Career pressure: scales rival / weather timing from completed programs.
/// Returns a multiplier on *intervals* (>1 = calmer early, <1 = harsher late).
#[must_use]
pub fn career_pressure_interval_scale(programs_completed: usize) -> f64 {
    match programs_completed {
        0 => 1.3,     // Earth-only: gentler onboarding
        1 | 2 => 1.0, // Moon–Mars baseline
        _ => 0.75,    // Venus+ deep-space pressure
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn career_pressure_eases_early_and_tightens_late() {
        assert!(career_pressure_interval_scale(0) > career_pressure_interval_scale(1));
        assert_eq!(
            career_pressure_interval_scale(1),
            career_pressure_interval_scale(2)
        );
        assert!(career_pressure_interval_scale(3) < career_pressure_interval_scale(2));
        assert!(career_pressure_interval_scale(5) < 1.0);
    }
}
