import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { UiEventDto } from "./types";

export function onUiEvent(cb: (event: UiEventDto) => void): Promise<UnlistenFn> {
  return listen<UiEventDto>("ui-event", (raw) => cb(raw.payload));
}
