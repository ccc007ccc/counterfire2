import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { api } from "./api";
import { onUiEvent } from "./events";
import type { AppConfig, RuntimeStatus, UiEventDto } from "./types";

interface EngineState {
  config: AppConfig | null;
  status: RuntimeStatus;
  events: UiEventDto[];
  busy: boolean;
  refresh(): Promise<void>;
  saveConfig(config: AppConfig): Promise<void>;
  start(): Promise<void>;
  stop(): Promise<void>;
  demoOnce(event: string): Promise<void>;
  clearEvents(): void;
}

interface SaveWaiter {
  resolve(): void;
  reject(err: unknown): void;
}

const Ctx = createContext<EngineState | null>(null);
const initialStatus: RuntimeStatus = { running: false, mode: null };
const SAVE_DEBOUNCE_MS = 220;

export function EngineProvider({ children }: { children: ReactNode }) {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [status, setStatus] = useState<RuntimeStatus>(initialStatus);
  const [events, setEvents] = useState<UiEventDto[]>([]);
  const [busy, setBusy] = useState(false);
  const saveTimer = useRef<number | null>(null);
  const pendingConfig = useRef<AppConfig | null>(null);
  const pendingWaiters = useRef<SaveWaiter[]>([]);
  const saveSeq = useRef(0);

  const pushEvent = useCallback((event: UiEventDto) => {
    setEvents((prev) => [event, ...prev].slice(0, 80));
  }, []);

  const pushError = useCallback(
    (message: string) => {
      pushEvent({
        timestampMs: Date.now(),
        level: "error",
        kind: "runtime",
        message,
      });
    },
    [pushEvent],
  );

  const refresh = useCallback(async () => {
    try {
      const [nextConfig, nextStatus] = await Promise.all([
        api.getConfig(),
        api.runtimeStatus(),
      ]);
      setConfig(nextConfig);
      setStatus(nextStatus);
    } catch (err) {
      pushError(`加载状态失败: ${String(err)}`);
      throw err;
    }
  }, [pushError]);

  useEffect(() => {
    refresh().catch(() => undefined);
  }, [refresh]);

  useEffect(() => {
    const unsubscribe = onUiEvent((event) => {
      pushEvent(event);
      if (["runtime", "gsi", "overlay"].includes(event.kind)) {
        api.runtimeStatus()
          .then(setStatus)
          .catch((err) => pushError(`刷新运行状态失败: ${String(err)}`));
      }
    });
    return () => {
      unsubscribe.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, [pushEvent, pushError]);

  const flushConfigSave = useCallback(async () => {
    const next = pendingConfig.current;
    const waiters = pendingWaiters.current;
    pendingConfig.current = null;
    pendingWaiters.current = [];
    saveTimer.current = null;
    if (!next) {
      waiters.forEach((waiter) => waiter.resolve());
      return;
    }

    const seq = ++saveSeq.current;
    try {
      const persisted = await api.updateConfig(next);
      if (seq === saveSeq.current) {
        setConfig(persisted);
      }
      waiters.forEach((waiter) => waiter.resolve());
    } catch (err) {
      pushError(`保存配置失败: ${String(err)}`);
      waiters.forEach((waiter) => waiter.reject(err));
    }
  }, [pushError]);

  useEffect(() => {
    return () => {
      if (saveTimer.current !== null) {
        window.clearTimeout(saveTimer.current);
      }
    };
  }, []);

  const saveConfig = useCallback((next: AppConfig) => {
    setConfig(next);
    pendingConfig.current = next;
    if (saveTimer.current !== null) {
      window.clearTimeout(saveTimer.current);
    }
    const promise = new Promise<void>((resolve, reject) => {
      pendingWaiters.current.push({ resolve, reject });
    });
    saveTimer.current = window.setTimeout(() => {
      flushConfigSave().catch((err) => pushError(`保存配置失败: ${String(err)}`));
    }, SAVE_DEBOUNCE_MS);
    return promise;
  }, [flushConfigSave, pushError]);

  const start = useCallback(async () => {
    setBusy(true);
    try {
      await api.startService();
      setStatus(await api.runtimeStatus());
    } catch (err) {
      pushError(`启动失败: ${String(err)}`);
      throw err;
    } finally {
      setBusy(false);
    }
  }, [pushError]);

  const stop = useCallback(async () => {
    setBusy(true);
    try {
      await api.stopService();
      setStatus(await api.runtimeStatus());
    } catch (err) {
      pushError(`停止失败: ${String(err)}`);
      throw err;
    } finally {
      setBusy(false);
    }
  }, [pushError]);

  const demoOnce = useCallback(async (event: string) => {
    try {
      await api.demoOnce(event);
    } catch (err) {
      pushError(`测试触发失败: ${String(err)}`);
      throw err;
    }
  }, [pushError]);

  const clearEvents = useCallback(() => setEvents([]), []);

  const value = useMemo(
    () => ({
      config,
      status,
      events,
      busy,
      refresh,
      saveConfig,
      start,
      stop,
      demoOnce,
      clearEvents,
    }),
    [config, status, events, busy, refresh, saveConfig, start, stop, demoOnce, clearEvents],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useEngine(): EngineState {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useEngine must be used inside EngineProvider");
  return ctx;
}
