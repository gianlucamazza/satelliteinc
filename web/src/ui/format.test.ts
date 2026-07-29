import { describe, expect, it } from "vitest";
import {
  contractDaysLeft,
  contractProgress,
  dataFlowStatus,
  downlinkRateLabel,
  formatDuration,
  isDownlinkLinkLive,
  nextPassStation,
  orbitRequirementText,
  payloadModeLabel,
} from "./format.ts";
import { esc, gauge } from "./html.ts";
import type { Snapshot, SpacecraftSnapshot } from "../types.ts";

function craft(partial: Partial<SpacecraftSnapshot>): SpacecraftSnapshot {
  return {
    id: 0,
    name: "Alpha",
    bus_id: "cubesat",
    bus_name: "CubeSat",
    phase: "OnOrbit",
    body_id: "earth",
    position_eci_km: [0, 0, 0],
    velocity_eci_km_s: [0, 0, 0],
    elements: null,
    orbit_trail_km: [],
    power_wh: 50,
    battery_wh_capacity: 100,
    buffer_mb: 20,
    buffer_capacity_mb: 100,
    downlink_active: false,
    in_sunlight: true,
    in_pass: false,
    downlink_rate_mb_s: 0.05,
    imaging_rate_mb_s: 0.1,
    target_altitude_km: 400,
    target_inclination_deg: 97,
    altitude_km: 400,
    propellant_kg: 1,
    propellant_capacity_kg: 2,
    delta_v_remaining_km_s: 0.1,
    total_delta_v_used_km_s: 0,
    pending_hohmann: null,
    at_apsis: false,
    at_node: false,
    seconds_to_perigee: null,
    seconds_to_apogee: null,
    hohmann_preview: null,
    maneuver_target_altitude_km: 500,
    payload_mode: "imaging",
    downlink_station_id: "svalbard",
    command_queue: [],
    active_anomaly: null,
    anomaly_label: null,
    light_delay_seconds: 0,
    solar_flux: 1,
    ops_cost_usd_per_day: 1000,
    contracts_completed: 0,
    lifetime_revenue_usd: 0,
    lifetime_ops_usd: 0,
    launched_at_sim_seconds: 0,
    station_status: [],
    transfer: null,
    transfer_planner: null,
    ...partial,
  };
}

function snap(partial: Partial<Snapshot> & { spacecraft: SpacecraftSnapshot[] }): Snapshot {
  return {
    bodies: [],
    epoch_utc: "",
    sim_seconds: 0,
    sim_days: 0,
    time_accel: 1,
    tick: 0,
    company_name: "Test",
    cash_usd: 100_000,
    reputation: 20,
    game_mode: "campaign",
    unlocked_tech: [],
    unlocked_buses: [],
    marketplace: [],
    active_contracts: [],
    ground_stations: [
      {
        id: "svalbard",
        name: "SvalSat",
        latitude_deg: 78,
        longitude_deg: 15,
        bandwidth_mbps: 24,
        body_id: "earth",
        dsn: false,
        requires_tech: null,
        unlocked: true,
      },
      {
        id: "kiruna",
        name: "Kiruna",
        latitude_deg: 67,
        longitude_deg: 20,
        bandwidth_mbps: 16,
        body_id: "earth",
        dsn: false,
        requires_tech: null,
        unlocked: true,
      },
    ],
    reference_satellites: [],
    phase: "on_orbit",
    outcome: null,
    ops_cost_usd_per_day: 1000,
    transfers_unlocked: false,
    next_event_sim_seconds: null,
    programs: [],
    max_flights: 2,
    active_flights: 1,
    ...partial,
  };
}

describe("formatDuration", () => {
  it("formats seconds", () => {
    expect(formatDuration(45)).toBe("45s");
  });
  it("formats minutes", () => {
    expect(formatDuration(90)).toBe("1.5 min");
  });
  it("formats hours", () => {
    expect(formatDuration(7200)).toBe("2.0 h");
  });
});

describe("contractProgress", () => {
  it("is 100 when requirement is zero", () => {
    expect(
      contractProgress({
        downlink_mb_required: 0,
        downlink_mb_done: 0,
      } as never),
    ).toBe(100);
  });
  it("clamps to 100", () => {
    expect(
      contractProgress({
        downlink_mb_required: 100,
        downlink_mb_done: 150,
      } as never),
    ).toBe(100);
  });
  it("computes percentage", () => {
    expect(
      contractProgress({
        downlink_mb_required: 200,
        downlink_mb_done: 50,
      } as never),
    ).toBe(25);
  });
});

describe("contractDaysLeft", () => {
  it("never goes negative", () => {
    expect(
      contractDaysLeft(
        { deadline_sim_seconds: 1000 } as never,
        5000,
      ),
    ).toBe(0);
  });
  it("returns remaining days", () => {
    const days = contractDaysLeft(
      { deadline_sim_seconds: 86_400 * 3 } as never,
      86_400,
    );
    expect(days).toBeCloseTo(2, 5);
  });
});

describe("orbitRequirementText", () => {
  it("returns empty when unconstrained", () => {
    expect(
      orbitRequirementText({
        min_altitude_km: null,
        max_altitude_km: null,
        min_inclination_deg: null,
      }),
    ).toBe("");
  });
  it("joins altitude and inclination", () => {
    expect(
      orbitRequirementText({
        min_altitude_km: 700,
        max_altitude_km: null,
        min_inclination_deg: 97,
      }),
    ).toBe("alt ≥ 700 km · inc ≥ 97°");
  });
});

describe("payloadModeLabel", () => {
  it("labels known modes", () => {
    expect(payloadModeLabel("imaging")).toBe("Imaging");
    expect(payloadModeLabel("downlink_priority")).toBe("Downlink priority");
  });
});

describe("isDownlinkLinkLive", () => {
  it("uses the selected dish, not fleet-wide in_pass", () => {
    const c = craft({
      in_pass: true,
      downlink_station_id: "svalbard",
      station_status: [
        {
          station_id: "svalbard",
          elevation_deg: -5,
          in_pass: false,
          next_pass_in_seconds: 600,
          bandwidth_mbps: 24,
        },
        {
          station_id: "kiruna",
          elevation_deg: 40,
          in_pass: true,
          next_pass_in_seconds: null,
          bandwidth_mbps: 16,
        },
      ],
    });
    expect(isDownlinkLinkLive(c)).toBe(false);
    c.downlink_station_id = "kiruna";
    expect(isDownlinkLinkLive(c)).toBe(true);
  });
});

describe("nextPassStation", () => {
  it("picks soonest ETA for the given craft, not elevation", () => {
    const alpha = craft({
      id: 1,
      station_status: [
        {
          station_id: "kiruna",
          elevation_deg: 20,
          in_pass: false,
          next_pass_in_seconds: 3600,
          bandwidth_mbps: 16,
        },
        {
          station_id: "svalbard",
          elevation_deg: 2,
          in_pass: false,
          next_pass_in_seconds: 120,
          bandwidth_mbps: 24,
        },
      ],
    });
    const beta = craft({
      id: 2,
      station_status: [
        {
          station_id: "kiruna",
          elevation_deg: 5,
          in_pass: false,
          next_pass_in_seconds: 30,
          bandwidth_mbps: 16,
        },
      ],
    });
    const s = snap({ spacecraft: [beta, alpha] });
    // Without craft arg, legacy falls back to spacecraft[0] (beta).
    expect(nextPassStation(s)?.name).toBe("Kiruna");
    // Selected craft alpha: soonest is Svalbard (120s), not higher elev Kiruna.
    expect(nextPassStation(s, alpha)?.name).toBe("SvalSat");
    expect(nextPassStation(s, alpha)?.etaSeconds).toBe(120);
  });
});

describe("dataFlowStatus", () => {
  it("diagnoses selected dish link, not any-station in_pass", () => {
    const c = craft({
      downlink_active: true,
      in_pass: true,
      buffer_mb: 50,
      downlink_station_id: "svalbard",
      station_status: [
        {
          station_id: "svalbard",
          elevation_deg: -10,
          in_pass: false,
          next_pass_in_seconds: 300,
          bandwidth_mbps: 24,
        },
      ],
    });
    const s = snap({ spacecraft: [c] });
    const flow = dataFlowStatus(s, c);
    expect(flow?.kind).toBe("warn");
    expect(flow?.text).toMatch(/Waiting for.*pass/i);
    expect(flow?.text).toMatch(/5\.0 min|300/);
  });

  it("reports live downlink with rate when link is good", () => {
    const c = craft({
      downlink_active: true,
      in_pass: true,
      buffer_mb: 50,
      downlink_rate_mb_s: 0.1,
      downlink_station_id: "svalbard",
      station_status: [
        {
          station_id: "svalbard",
          elevation_deg: 45,
          in_pass: true,
          next_pass_in_seconds: null,
          bandwidth_mbps: 24,
        },
      ],
    });
    const s = snap({ spacecraft: [c] });
    const flow = dataFlowStatus(s, c);
    expect(flow?.kind).toBe("ok");
    expect(flow?.text).toContain("Downlinking");
    expect(flow?.text).toContain("100 KB/s");
  });
});

describe("downlinkRateLabel", () => {
  it("shows Armed when radio is on but dish is not in view", () => {
    const c = craft({
      downlink_active: true,
      downlink_rate_mb_s: 0.05,
      downlink_station_id: "svalbard",
      station_status: [
        {
          station_id: "svalbard",
          elevation_deg: -5,
          in_pass: false,
          next_pass_in_seconds: 100,
          bandwidth_mbps: 24,
        },
      ],
    });
    expect(downlinkRateLabel(c)).toBe("Armed");
  });
});

describe("html helpers", () => {
  it("escapes markup", () => {
    expect(esc(`<script>"&`)).toBe("&lt;script&gt;&quot;&amp;");
  });
  it("renders gauge with danger when low", () => {
    const html = gauge("Battery", 5, 100, "Wh", { lowIsBad: true });
    expect(html).toContain("danger");
    expect(html).toContain("Battery");
  });
});
