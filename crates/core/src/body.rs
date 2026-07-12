//! Celestial bodies as data: registry compiled from `GameContent::bodies`.
//!
//! Every body-centered-inertial (BCI) frame shares its axis orientation with
//! the heliocentric frame; a body's `orbit` is parent-relative Kepler
//! elements with epoch at `sim_seconds = 0`. Positions are pure functions of
//! sim time, so the ephemeris is deterministic by construction.

use std::collections::HashMap;

use crate::content::{AnomalyFlavorDef, CelestialBodyDef};
use crate::error::SimError;
use crate::orbit::{KeplerElements, eci_position_km, mean_to_true_anomaly_deg};

#[derive(Debug, Clone)]
pub struct CelestialBody {
    pub id: String,
    pub name: String,
    pub mu_km3_s2: f64,
    pub radius_km: f64,
    /// Sidereal rotation rate (rad/s); negative = retrograde (Venus).
    pub rotation_rad_s: f64,
    /// Index of the parent body in the registry; `None` for the root (sun).
    pub parent: Option<usize>,
    /// Parent-relative orbit, epoch at sim t = 0; `None` for the root.
    pub orbit: Option<KeplerElements>,
    /// Sphere-of-influence radius; `f64::INFINITY` for the root.
    pub soi_radius_km: f64,
    pub min_parking_altitude_km: f64,
    pub max_parking_altitude_km: f64,
    /// Renderer pass-through (texture id); physics never reads it.
    pub texture: Option<String>,
    /// Effective DSN bandwidth fraction from this body.
    pub dsn_bandwidth_factor: f64,
    /// Divides the anomaly MTBF at this body.
    pub anomaly_rate_multiplier: f64,
    pub anomaly_flavor: Option<AnomalyFlavorDef>,
}

#[derive(Debug, Clone)]
pub struct BodyRegistry {
    bodies: Vec<CelestialBody>,
    by_id: HashMap<String, usize>,
}

impl BodyRegistry {
    pub fn from_content(defs: &[CelestialBodyDef]) -> Result<Self, SimError> {
        let mut by_id = HashMap::with_capacity(defs.len());
        for (i, def) in defs.iter().enumerate() {
            if by_id.insert(def.id.clone(), i).is_some() {
                return Err(SimError::Content(format!("duplicate body '{}'", def.id)));
            }
        }

        let mut bodies = Vec::with_capacity(defs.len());
        for def in defs {
            let parent = match &def.parent {
                Some(pid) => Some(*by_id.get(pid).ok_or_else(|| {
                    SimError::Content(format!("body '{}' has unknown parent '{pid}'", def.id))
                })?),
                None => None,
            };

            let orbit = def.orbit.as_ref().map(|o| KeplerElements {
                semi_major_axis_km: o.semi_major_axis_km,
                eccentricity: o.eccentricity,
                inclination_deg: o.inclination_deg,
                raan_deg: o.raan_deg,
                arg_perigee_deg: o.arg_periapsis_deg,
                true_anomaly_deg: mean_to_true_anomaly_deg(
                    o.mean_anomaly_epoch_deg,
                    o.eccentricity,
                ),
                epoch_sim_seconds: 0.0,
            });

            let soi_radius_km = match (def.soi_radius_km, parent, &orbit) {
                (Some(explicit), _, _) => explicit,
                (None, None, _) => f64::INFINITY,
                (None, Some(pidx), Some(orbit)) => {
                    let parent_mu = defs[pidx].mu_km3_s2;
                    orbit.semi_major_axis_km * (def.mu_km3_s2 / parent_mu).powf(0.4)
                }
                (None, Some(_), None) => {
                    return Err(SimError::Content(format!(
                        "body '{}' has a parent but no orbit",
                        def.id
                    )));
                }
            };

            bodies.push(CelestialBody {
                id: def.id.clone(),
                name: def.name.clone(),
                mu_km3_s2: def.mu_km3_s2,
                radius_km: def.radius_km,
                rotation_rad_s: def.rotation_rad_s,
                parent,
                orbit,
                soi_radius_km,
                min_parking_altitude_km: def.min_parking_altitude_km,
                max_parking_altitude_km: def.max_parking_altitude_km,
                texture: def.texture.clone(),
                dsn_bandwidth_factor: def.dsn_bandwidth_factor,
                anomaly_rate_multiplier: def.anomaly_rate_multiplier,
                anomaly_flavor: def.anomaly_flavor.clone(),
            });
        }

        let registry = Self { bodies, by_id };
        registry.earth_checked()?;
        // `by_id` is built before parents resolve, so self/forward references
        // parse; a cycle would hang the parent-chain walks. Bounded walk.
        for start in 0..registry.bodies.len() {
            let mut current = registry.bodies[start].parent;
            let mut hops = 0;
            while let Some(pidx) = current {
                hops += 1;
                if hops > registry.bodies.len() {
                    return Err(SimError::Content(format!(
                        "body '{}' has a cyclic parent chain",
                        registry.bodies[start].id
                    )));
                }
                current = registry.bodies[pidx].parent;
            }
        }
        Ok(registry)
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&CelestialBody> {
        self.by_id.get(id).map(|&i| &self.bodies[i])
    }

    #[must_use]
    pub fn idx(&self, id: &str) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    #[must_use]
    pub fn body(&self, idx: usize) -> &CelestialBody {
        &self.bodies[idx]
    }

    pub fn iter(&self) -> impl Iterator<Item = &CelestialBody> {
        self.bodies.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.bodies.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }

    fn earth_checked(&self) -> Result<&CelestialBody, SimError> {
        self.get("earth")
            .ok_or_else(|| SimError::Content("bodies must include 'earth'".into()))
    }

    /// Hot-path convenience: the registry is validated to contain earth.
    #[must_use]
    pub fn earth(&self) -> &CelestialBody {
        self.get("earth").expect("registry validated to hold earth")
    }

    /// Parent-relative position (km) at `sim_seconds`; `[0,0,0]` for the root.
    #[must_use]
    pub fn parent_relative_position_km(&self, idx: usize, sim_seconds: f64) -> [f64; 3] {
        let body = &self.bodies[idx];
        match (body.parent, &body.orbit) {
            (Some(pidx), Some(orbit)) => {
                let parent_mu = self.bodies[pidx].mu_km3_s2;
                eci_position_km(parent_mu, orbit, sim_seconds)
            }
            _ => [0.0, 0.0, 0.0],
        }
    }

    /// Heliocentric position (km) at `sim_seconds`: parent-chain sum.
    #[must_use]
    pub fn heliocentric_position_km(&self, idx: usize, sim_seconds: f64) -> [f64; 3] {
        let mut pos = self.parent_relative_position_km(idx, sim_seconds);
        let mut current = self.bodies[idx].parent;
        while let Some(pidx) = current {
            let parent_pos = self.parent_relative_position_km(pidx, sim_seconds);
            pos[0] += parent_pos[0];
            pos[1] += parent_pos[1];
            pos[2] += parent_pos[2];
            current = self.bodies[pidx].parent;
        }
        pos
    }

    /// Unit vector from the body toward the sun at `sim_seconds`.
    /// For the root body itself this is meaningless; returns +X.
    #[must_use]
    pub fn sun_direction_from(&self, idx: usize, sim_seconds: f64) -> [f64; 3] {
        let pos = self.heliocentric_position_km(idx, sim_seconds);
        let r = (pos[0] * pos[0] + pos[1] * pos[1] + pos[2] * pos[2]).sqrt();
        if r < 1.0 {
            return [1.0, 0.0, 0.0];
        }
        [-pos[0] / r, -pos[1] / r, -pos[2] / r]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{EARTH_RADIUS_KM, EARTH_ROTATION_RAD_S, MU_EARTH};
    use crate::content::GameContent;

    fn registry() -> BodyRegistry {
        let content = GameContent::from_json(include_str!("../../../content/game.json")).unwrap();
        BodyRegistry::from_content(&content.bodies).unwrap()
    }

    /// The refactor's zero-change guarantee: content earth must carry the
    /// exact same f64 bits as the constants the physics used to import.
    #[test]
    fn content_earth_matches_constants_exactly() {
        let reg = registry();
        let earth = reg.earth();
        assert_eq!(earth.mu_km3_s2, MU_EARTH);
        assert_eq!(earth.radius_km, EARTH_RADIUS_KM);
        assert_eq!(earth.rotation_rad_s, EARTH_ROTATION_RAD_S);
    }

    #[test]
    fn soi_radii_are_physical() {
        let reg = registry();
        let earth = reg.earth();
        // Earth's SOI ~ 0.93e6 km.
        assert!((earth.soi_radius_km - 9.24e5).abs() < 5e4);
        let sun = reg.get("sun").unwrap();
        assert!(sun.soi_radius_km.is_infinite());
        let moon = reg.get("moon").unwrap();
        // Moon's SOI ~ 66_000 km.
        assert!((moon.soi_radius_km - 6.6e4).abs() < 5e3);
    }

    #[test]
    fn earth_sits_at_minus_x_at_epoch() {
        // Sim t = 0 is the 2026 vernal equinox: Earth at heliocentric
        // longitude 180°, so the sun direction from Earth is +X — the same
        // vector the fixed-sun shadow model has always used.
        let reg = registry();
        let idx = reg.idx("earth").unwrap();
        let pos = reg.heliocentric_position_km(idx, 0.0);
        let r = (pos[0] * pos[0] + pos[1] * pos[1] + pos[2] * pos[2]).sqrt();
        assert!(pos[0] < 0.0);
        assert!((pos[1] / r).abs() < 1e-6, "earth off the -X axis: {pos:?}");
        assert!((pos[2] / r).abs() < 1e-9);
        let sun_dir = reg.sun_direction_from(idx, 0.0);
        assert!((sun_dir[0] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn moon_heliocentric_is_earth_plus_geocentric() {
        let reg = registry();
        let earth = reg.idx("earth").unwrap();
        let moon = reg.idx("moon").unwrap();
        let t = 1_234_567.0;
        let earth_helio = reg.heliocentric_position_km(earth, t);
        let moon_geo = reg.parent_relative_position_km(moon, t);
        let moon_helio = reg.heliocentric_position_km(moon, t);
        for k in 0..3 {
            assert!((moon_helio[k] - (earth_helio[k] + moon_geo[k])).abs() < 1e-6);
        }
    }
}
