use crate::body::CelestialBody;
use crate::error::SimError;
use crate::event::SimEvent;
use crate::orbit::{
    BurnDirection, HohmannPlan, apply_impulsive_burn, at_apogee, at_perigee, eci_position_km,
    eci_velocity_km_s, hohmann_plan, propellant_for_delta_v,
};
use crate::spacecraft::{HohmannPlanSnapshot, PendingHohmann, Spacecraft};

pub struct ManeuverResult {
    pub propellant_used_kg: f64,
    pub delta_v_km_s: f64,
    pub new_altitude_km: Option<f64>,
    pub events: Vec<SimEvent>,
}

pub fn execute_prograde_burn(
    body: &CelestialBody,
    craft: &mut Spacecraft,
    sim_seconds: f64,
    delta_v_km_s: f64,
    retrograde: bool,
    isp_mult: f64,
) -> Result<ManeuverResult, SimError> {
    let elements = craft
        .elements
        .ok_or_else(|| SimError::InvalidAction("no orbit".into()))?;
    let direction = if retrograde {
        BurnDirection::Retrograde
    } else {
        BurnDirection::Prograde
    };
    let prop_used = spend_propellant(craft, delta_v_km_s, isp_mult)?;
    let new_elements = apply_impulsive_burn(
        body.mu_km3_s2,
        &elements,
        sim_seconds,
        delta_v_km_s,
        direction,
    )?;
    let alt = new_elements.semi_major_axis_km - body.radius_km;
    craft.elements = Some(new_elements);
    craft.total_delta_v_used_km_s += delta_v_km_s;
    let label = if retrograde {
        "retrograde burn".to_string()
    } else {
        "prograde burn".to_string()
    };
    Ok(ManeuverResult {
        propellant_used_kg: prop_used,
        delta_v_km_s,
        new_altitude_km: Some(alt),
        events: vec![SimEvent::ManeuverExecuted {
            craft_id: craft.id,
            delta_v_km_s,
            propellant_used_kg: prop_used,
            new_altitude_km: Some(alt),
            label,
        }],
    })
}

pub fn spend_propellant(
    craft: &mut Spacecraft,
    delta_v_km_s: f64,
    isp_mult: f64,
) -> Result<f64, SimError> {
    let needed = propellant_for_delta_v(
        delta_v_km_s,
        craft.dry_mass_kg(),
        craft.propellant_kg,
        craft.bus.isp_s * isp_mult,
    );
    if needed > craft.propellant_kg + 1e-6 {
        return Err(SimError::InvalidAction(format!(
            "insufficient propellant: need {needed:.2} kg"
        )));
    }
    craft.propellant_kg -= needed;
    Ok(needed)
}

pub fn start_hohmann(
    body: &CelestialBody,
    craft: &mut Spacecraft,
    sim_seconds: f64,
    target_altitude_km: f64,
    isp_mult: f64,
) -> Result<ManeuverResult, SimError> {
    if craft.pending_hohmann.is_some() {
        return Err(SimError::InvalidAction(
            "Hohmann transfer in progress".into(),
        ));
    }
    let elements = craft
        .elements
        .ok_or_else(|| SimError::InvalidAction("no orbit".into()))?;
    let r1 = elements.semi_major_axis_km;
    let r2 = body.radius_km + target_altitude_km;
    let plan = hohmann_plan(body.mu_km3_s2, body.radius_km, r1, r2, target_altitude_km)?;

    let pos = eci_position_km(body.mu_km3_s2, &elements, sim_seconds);
    let vel = eci_velocity_km_s(body.mu_km3_s2, &elements, sim_seconds);

    let at_right_apsis = if plan.raising {
        at_perigee(&elements, pos, vel)
    } else {
        at_apogee(&elements, pos, vel)
    };
    if !at_right_apsis {
        return Err(SimError::InvalidAction(format!(
            "burn 1 requires {}",
            if plan.raising { "perigee" } else { "apogee" }
        )));
    }

    let prop_used = spend_propellant(craft, plan.burn1_dv_km_s, isp_mult)?;
    let new_elements = apply_impulsive_burn(
        body.mu_km3_s2,
        &elements,
        sim_seconds,
        plan.burn1_dv_km_s,
        plan.burn1_direction,
    )?;
    craft.elements = Some(new_elements);
    craft.total_delta_v_used_km_s += plan.burn1_dv_km_s;
    craft.pending_hohmann = Some(PendingHohmann {
        plan: HohmannPlanSnapshot::from(&plan),
    });

    Ok(ManeuverResult {
        propellant_used_kg: prop_used,
        delta_v_km_s: plan.burn1_dv_km_s,
        new_altitude_km: None,
        events: vec![
            SimEvent::HohmannBurn1Complete {
                craft_id: craft.id,
                delta_v_km_s: plan.burn1_dv_km_s,
                target_altitude_km,
            },
            SimEvent::ManeuverExecuted {
                craft_id: craft.id,
                delta_v_km_s: plan.burn1_dv_km_s,
                propellant_used_kg: prop_used,
                new_altitude_km: None,
                label: "Hohmann burn 1".into(),
            },
        ],
    })
}

pub fn complete_hohmann(
    body: &CelestialBody,
    craft: &mut Spacecraft,
    sim_seconds: f64,
    isp_mult: f64,
) -> Result<ManeuverResult, SimError> {
    let pending = craft
        .pending_hohmann
        .clone()
        .ok_or_else(|| SimError::InvalidAction("no pending Hohmann transfer".into()))?;
    let plan = reconstruct_plan(&pending.plan);
    let elements = craft
        .elements
        .ok_or_else(|| SimError::InvalidAction("no orbit".into()))?;

    let pos = eci_position_km(body.mu_km3_s2, &elements, sim_seconds);
    let vel = eci_velocity_km_s(body.mu_km3_s2, &elements, sim_seconds);

    let at_right_apsis = if plan.raising {
        at_apogee(&elements, pos, vel)
    } else {
        at_perigee(&elements, pos, vel)
    };
    if !at_right_apsis {
        return Err(SimError::InvalidAction(format!(
            "burn 2 requires {}",
            if plan.raising { "apogee" } else { "perigee" }
        )));
    }

    let prop_used = spend_propellant(craft, plan.burn2_dv_km_s, isp_mult)?;
    let new_elements = apply_impulsive_burn(
        body.mu_km3_s2,
        &elements,
        sim_seconds,
        plan.burn2_dv_km_s,
        plan.burn2_direction,
    )?;
    let alt = new_elements.semi_major_axis_km - body.radius_km;
    craft.elements = Some(new_elements);
    craft.total_delta_v_used_km_s += plan.burn2_dv_km_s;
    craft.pending_hohmann = None;

    Ok(ManeuverResult {
        propellant_used_kg: prop_used,
        delta_v_km_s: plan.burn2_dv_km_s,
        new_altitude_km: Some(alt),
        events: vec![
            SimEvent::HohmannTransferComplete {
                craft_id: craft.id,
                target_altitude_km: plan.target_altitude_km,
            },
            SimEvent::ManeuverExecuted {
                craft_id: craft.id,
                delta_v_km_s: plan.burn2_dv_km_s,
                propellant_used_kg: prop_used,
                new_altitude_km: Some(alt),
                label: "Hohmann burn 2".into(),
            },
        ],
    })
}

pub fn preview_hohmann(
    body: &CelestialBody,
    craft: &Spacecraft,
    target_altitude_km: f64,
) -> Result<HohmannPlan, SimError> {
    let elements = craft
        .elements
        .ok_or_else(|| SimError::InvalidAction("no orbit".into()))?;
    let r1 = elements.semi_major_axis_km;
    let r2 = body.radius_km + target_altitude_km;
    hohmann_plan(body.mu_km3_s2, body.radius_km, r1, r2, target_altitude_km)
}

fn reconstruct_plan(snap: &HohmannPlanSnapshot) -> HohmannPlan {
    HohmannPlan {
        burn1_dv_km_s: snap.burn1_dv_km_s,
        burn2_dv_km_s: snap.burn2_dv_km_s,
        total_dv_km_s: snap.total_dv_km_s,
        burn1_direction: if snap.raising {
            BurnDirection::Prograde
        } else {
            BurnDirection::Retrograde
        },
        burn2_direction: if snap.raising {
            BurnDirection::Prograde
        } else {
            BurnDirection::Retrograde
        },
        target_altitude_km: snap.target_altitude_km,
        raising: snap.raising,
    }
}
