use satelliteinc_core::{GameContent, PlayerAction, SaveGame, SimError, SimEvent, SimState};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

thread_local! {
    static SIM: RefCell<Option<SimState>> = const { RefCell::new(None) };
}

fn with_sim<F, T>(f: F) -> Result<T, JsValue>
where
    F: FnOnce(&mut SimState) -> Result<T, JsValue>,
{
    SIM.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let sim = borrow
            .as_mut()
            .ok_or_else(|| JsValue::from_str("sim not initialized"))?;
        f(sim)
    })
}

fn sim_error(err: &SimError) -> JsValue {
    JsValue::from_str(&err.to_string())
}

#[wasm_bindgen(start)]
pub fn wasm_start() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn init(content_json: &str) -> Result<(), JsValue> {
    let content = GameContent::from_json(content_json)
        .map_err(|e| JsValue::from_str(&format!("invalid content: {e}")))?;
    let sim = SimState::new(content).map_err(|e| sim_error(&e))?;
    SIM.with(|cell| *cell.borrow_mut() = Some(sim));
    Ok(())
}

#[wasm_bindgen]
pub fn step(real_dt_seconds: f64) -> Result<String, JsValue> {
    with_sim(|sim| {
        sim.step(real_dt_seconds);
        serde_json::to_string(&sim.snapshot()).map_err(|e| JsValue::from_str(&e.to_string()))
    })
}

#[wasm_bindgen]
pub fn apply_action(action_json: &str) -> Result<String, JsValue> {
    with_sim(|sim| {
        let action: PlayerAction = serde_json::from_str(action_json)
            .map_err(|e| JsValue::from_str(&format!("invalid action: {e}")))?;
        sim.apply_action(action).map_err(|e| sim_error(&e))?;
        serde_json::to_string(&sim.snapshot()).map_err(|e| JsValue::from_str(&e.to_string()))
    })
}

#[wasm_bindgen]
pub fn snapshot() -> Result<String, JsValue> {
    with_sim(|sim| {
        serde_json::to_string(&sim.snapshot()).map_err(|e| JsValue::from_str(&e.to_string()))
    })
}

#[wasm_bindgen]
pub fn drain_events() -> Result<String, JsValue> {
    with_sim(|sim| {
        let events: Vec<SimEvent> = sim.drain_events();
        serde_json::to_string(&events).map_err(|e| JsValue::from_str(&e.to_string()))
    })
}

#[wasm_bindgen]
pub fn reset(content_json: &str) -> Result<(), JsValue> {
    init(content_json)
}

#[wasm_bindgen]
pub fn export_save() -> Result<String, JsValue> {
    with_sim(|sim| {
        sim.export_save()
            .to_json()
            .map_err(|e| JsValue::from_str(&e.to_string()))
    })
}

#[wasm_bindgen]
pub fn import_save(save_json: &str) -> Result<String, JsValue> {
    SIM.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let sim = borrow
            .as_ref()
            .ok_or_else(|| JsValue::from_str("sim not initialized"))?;
        let save = SaveGame::from_json(save_json)
            .map_err(|e| JsValue::from_str(&format!("invalid save: {e}")))?;
        let content = sim.content.clone();
        let new_sim = SimState::import_save(content, save).map_err(|e| sim_error(&e))?;
        let snapshot = new_sim.snapshot();
        *borrow = Some(new_sim);
        serde_json::to_string(&snapshot).map_err(|e| JsValue::from_str(&e.to_string()))
    })
}

// Re-export for TypeScript type generation if needed
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
