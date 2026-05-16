export type RunMode = "gsi" | "demo";
export type Lang = "cn" | "en";
export type Side = "auto" | "CT" | "T";
export type UiLevel = "info" | "warn" | "error";
export type UiKind = "runtime" | "overlay" | "gsi" | "demo" | "effect" | "config";

export interface IconScales {
  single: number;
  double: number;
  triple: number;
  quad: number;
  penta: number;
  hexa: number;
  septa: number;
  octo: number;
  headshot: number;
  knife: number;
  grenade: number;
}

export interface AppConfig {
  mode: RunMode;
  port: number;
  lang: Lang;
  side: Side;
  width: number | null;
  height: number | null;
  vsync: boolean;
  iconScale: number;
  iconScales: IconScales;
  iconX: number;
  iconY: number;
  volume: number;
  killStreakResetSeconds: number;
}

export interface RuntimeStatus {
  running: boolean;
  mode: RunMode | null;
}

export interface UiEventDto {
  timestampMs: number;
  level: UiLevel;
  kind: UiKind;
  message: string;
}
