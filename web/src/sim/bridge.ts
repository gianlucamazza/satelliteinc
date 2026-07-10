import init, {
  init as simInit,
  step,
  apply_action,
  drain_events,
  export_save,
  import_save,
} from "@wasm/satelliteinc_wasm.js";
import type { GameContent, SimEvent, Snapshot } from "../types.ts";

let ready = false;

export async function loadSim(content: GameContent): Promise<void> {
  await init();
  simInit(JSON.stringify(content));
  ready = true;
}

export function isReady(): boolean {
  return ready;
}

export function simStep(realDt: number): Snapshot {
  const json = step(realDt);
  return JSON.parse(json) as Snapshot;
}

export function simAction(action: Record<string, unknown>): Snapshot {
  const json = apply_action(JSON.stringify(action));
  return JSON.parse(json) as Snapshot;
}

export function pullEvents(): SimEvent[] {
  const json = drain_events();
  return JSON.parse(json) as SimEvent[];
}

export function exportSave(): string {
  return export_save();
}

export function importSave(saveJson: string): Snapshot {
  const json = import_save(saveJson);
  return JSON.parse(json) as Snapshot;
}