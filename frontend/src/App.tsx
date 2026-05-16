import { getCurrentWindow } from "@tauri-apps/api/window";
import { EngineProvider, useEngine } from "./lib/engine";
import type { AppConfig, IconScales, RunMode, UiEventDto } from "./lib/types";

const maxCanvasDimension = 16384;

const demoEvents = [
  ["single", "单杀"],
  ["double", "双杀"],
  ["triple", "三杀"],
  ["quad", "四杀"],
  ["penta", "五杀"],
  ["hexa", "六杀"],
  ["septa", "七杀"],
  ["octo", "八杀"],
  ["headshot", "爆头"],
  ["knife", "刀杀"],
  ["grenade", "雷杀"],
] as const;

const iconScaleControls: Array<[keyof IconScales, string]> = [
  ["single", "单杀"],
  ["double", "双杀"],
  ["triple", "三杀"],
  ["quad", "四杀"],
  ["penta", "五杀"],
  ["hexa", "六杀"],
  ["septa", "七杀"],
  ["octo", "八杀"],
  ["headshot", "爆头"],
  ["knife", "刀杀"],
  ["grenade", "雷杀"],
];

function TitleBar() {
  const win = getCurrentWindow();
  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="brand" data-tauri-drag-region>
        <span className="brand-mark">CF</span>
        <div data-tauri-drag-region>
          <strong>CounterFire 2</strong>
          <span>CS2 GSI Kill Effect Panel</span>
        </div>
      </div>
      <div className="window-actions">
        <button className="window-control" aria-label="Minimize" onClick={() => win.minimize().catch(console.warn)}>
          <span className="win-icon minimize" aria-hidden="true" />
        </button>
        <button className="window-control" aria-label="Maximize" onClick={() => win.toggleMaximize().catch(console.warn)}>
          <span className="win-icon maximize" aria-hidden="true" />
        </button>
        <button className="window-control close-control" aria-label="Close" onClick={() => win.close().catch(console.warn)}>
          <span className="win-icon close" aria-hidden="true" />
        </button>
      </div>
    </header>
  );
}

function ControlPanel() {
  const { config, status, busy, saveConfig, start, stop, demoOnce } = useEngine();

  if (!config) {
    return <main className="main loading">正在加载配置…</main>;
  }

  const configLocked = status.running;
  const patch = <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => {
    saveConfig({ ...config, [key]: value }).catch(console.warn);
  };
  const patchSize = (key: "width" | "height", value: string) => {
    const trimmed = value.trim();
    if (!trimmed) {
      patch(key, null);
      return;
    }
    const parsed = Number(trimmed);
    if (Number.isFinite(parsed) && parsed > 0) {
      patch(key, Math.floor(parsed));
    }
  };
  const patchPort = (value: string) => {
    const parsed = Number(value);
    if (Number.isInteger(parsed) && parsed >= 1 && parsed <= 65535) {
      patch("port", parsed);
    }
  };
  const patchFloat = (
    key: "iconScale" | "iconX" | "iconY" | "volume" | "killStreakResetSeconds",
    value: string,
    min: number,
    max: number,
  ) => {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      const clamped = Math.min(max, Math.max(min, parsed));
      patch(key, Math.round(clamped * 100) / 100);
    }
  };
  const patchIconScale = (key: keyof IconScales, value: string) => {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      const clamped = Math.min(2, Math.max(0.5, parsed));
      patch("iconScales", { ...config.iconScales, [key]: Math.round(clamped * 100) / 100 });
    }
  };

  return (
    <main className="main">
      <section className="hero card">
        <div>
          <p className="eyebrow">Game Bar 小组件控制</p>
          <h1>{status.running ? "击杀特效后台运行中" : "准备启动 CF 风格击杀特效"}</h1>
          <p className="muted">
            {status.running
              ? `当前模式：${status.mode === "demo" ? "Demo 循环" : "CS2 GSI"}`
              : "启动后会连接 overlay-engine，并通过已打开的 Game Bar 小组件播放音效与图标动画。"}
          </p>
        </div>
        <div className="hero-action">
          <button
            className={status.running ? "primary stop" : "primary"}
            disabled={busy}
            onClick={() => (status.running ? stop() : start()).catch(console.warn)}
          >
            {busy ? "处理中…" : status.running ? "停止" : "启动"}
          </button>
          <p>请先手动打开 Game Bar 小组件，否则不会显示效果。</p>
        </div>
      </section>

      <section className="grid two">
        <div className="card stack">
          <div className="section-title">
            <h2>运行配置</h2>
            {status.running && <span className="pill">连接项运行中锁定</span>}
          </div>

          <label>
            <span>运行模式</span>
            <select
              value={config.mode}
              disabled={configLocked}
              onChange={(event) => patch("mode", event.target.value as RunMode)}
            >
              <option value="gsi">CS2 GSI</option>
              <option value="demo">Demo 循环</option>
            </select>
          </label>

          <div className="row">
            <label>
              <span>语言</span>
              <select
                value={config.lang}
                onChange={(event) => patch("lang", event.target.value as AppConfig["lang"])}
              >
                <option value="cn">中文</option>
                <option value="en">English</option>
              </select>
            </label>
            <label>
              <span>阵营选择</span>
              <select
                value={config.side}
                onChange={(event) => patch("side", event.target.value as AppConfig["side"])}
              >
                <option value="auto">自动识别</option>
                <option value="CT">固定 CT</option>
                <option value="T">固定 T</option>
              </select>
            </label>
          </div>

          <div className="row">
            <label>
              <span>GSI 端口（默认 57534）</span>
              <input
                type="number"
                min={1}
                max={65535}
                value={config.port}
                disabled={configLocked}
                onChange={(event) => patchPort(event.target.value)}
              />
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={config.vsync}
                disabled={configLocked}
                onChange={(event) => patch("vsync", event.target.checked)}
              />
              <span>启用 DwmFlush / VSync</span>
            </label>
          </div>

          <div className="section-title compact">
            <h2>实时调整</h2>
            {status.running && <span className="pill">运行中即时生效</span>}
          </div>

          <div className="row">
            <label className="range-label">
              <span>
                全局图标大小
                <strong>{Math.round(config.iconScale * 100)}%</strong>
              </span>
              <input
                type="range"
                min={0.5}
                max={2}
                step={0.05}
                value={config.iconScale}
                onChange={(event) => patchFloat("iconScale", event.target.value, 0.5, 2)}
              />
            </label>
            <label className="range-label">
              <span>
                音效音量
                <strong>{Math.round(config.volume * 100)}%</strong>
              </span>
              <input
                type="range"
                min={0}
                max={2}
                step={0.05}
                value={config.volume}
                onChange={(event) => patchFloat("volume", event.target.value, 0, 2)}
              />
            </label>
          </div>

          <label className="range-label">
            <span>
              连杀重置时间
              <strong>{config.killStreakResetSeconds.toFixed(0)} 秒</strong>
            </span>
            <input
              type="range"
              min={1}
              max={120}
              step={1}
              value={config.killStreakResetSeconds}
              onChange={(event) => patchFloat("killStreakResetSeconds", event.target.value, 1, 120)}
            />
          </label>

          <div className="section-title compact">
            <h2>单独图标大小</h2>
          </div>
          <div className="icon-scale-grid">
            {iconScaleControls.map(([key, label]) => (
              <label className="range-label" key={key}>
                <span>
                  {label}
                  <strong>{Math.round(config.iconScales[key] * 100)}%</strong>
                </span>
                <input
                  type="range"
                  min={0.5}
                  max={2}
                  step={0.05}
                  value={config.iconScales[key]}
                  onChange={(event) => patchIconScale(key, event.target.value)}
                />
              </label>
            ))}
          </div>

          <div className="row">
            <label>
              <span>画布宽度覆盖（空=当前屏幕宽度）</span>
              <input
                type="number"
                min={1}
                max={maxCanvasDimension}
                placeholder="屏幕宽度"
                value={config.width ?? ""}
                disabled={configLocked}
                onChange={(event) => patchSize("width", event.target.value)}
              />
            </label>
            <label>
              <span>画布高度覆盖（空=当前屏幕高度）</span>
              <input
                type="number"
                min={1}
                max={maxCanvasDimension}
                placeholder="屏幕高度"
                value={config.height ?? ""}
                disabled={configLocked}
                onChange={(event) => patchSize("height", event.target.value)}
              />
            </label>
          </div>
        </div>

        <div className="card stack">
          <div className="section-title">
            <h2>图标位置</h2>
            {status.running && <span className="pill">运行中即时生效</span>}
          </div>
          <div className="position-controls">
            <label className="range-label">
              <span>
                水平位置
                <strong>{Math.round(config.iconX * 100)}%</strong>
              </span>
              <input
                type="range"
                min={0}
                max={1}
                step={0.01}
                value={config.iconX}
                onChange={(event) => patchFloat("iconX", event.target.value, 0, 1)}
              />
            </label>
            <label className="range-label">
              <span>
                垂直位置
                <strong>{Math.round(config.iconY * 100)}%</strong>
              </span>
              <input
                type="range"
                min={0}
                max={1}
                step={0.01}
                value={config.iconY}
                onChange={(event) => patchFloat("iconY", event.target.value, 0, 1)}
              />
            </label>
            <div className="inline-actions">
              <button className="ghost" onClick={() => patch("iconX", 0.5)}>
                水平居中
              </button>
              <button className="ghost" onClick={() => patch("iconY", 0.5)}>
                垂直居中
              </button>
            </div>
          </div>

          <div className="section-title compact">
            <h2>测试触发</h2>
            {!status.running && <span className="pill">需先启动</span>}
          </div>
          <p className="muted">启动后台后，可直接向当前 overlay worker 发送击杀事件。</p>
          <div className="demo-grid">
            {demoEvents.map(([id, label]) => (
              <button key={id} disabled={busy || !status.running} onClick={() => demoOnce(id).catch(console.warn)}>
                {label}
              </button>
            ))}
          </div>
        </div>
      </section>
    </main>
  );
}

function EventLog() {
  const { events, clearEvents } = useEngine();
  return (
    <aside className="log card">
      <div className="section-title">
        <h2>状态日志</h2>
        <button className="ghost" onClick={clearEvents}>清空</button>
      </div>
      <div className="log-list" role="log" aria-live="polite" aria-relevant="additions text">
        {events.length === 0 && <p className="muted empty">暂无事件</p>}
        {events.map((event) => (
          <LogLine key={`${event.timestampMs}:${event.message}`} event={event} />
        ))}
      </div>
    </aside>
  );
}

function LogLine({ event }: { event: UiEventDto }) {
  const time = new Date(event.timestampMs).toLocaleTimeString();
  return (
    <div className={`log-line ${event.level}`}>
      <span>{time}</span>
      <strong>{event.kind}</strong>
      <p>{event.message}</p>
    </div>
  );
}

function Shell() {
  return (
    <div className="app-shell">
      <TitleBar />
      <ControlPanel />
      <EventLog />
    </div>
  );
}

export function App() {
  return (
    <EngineProvider>
      <Shell />
    </EngineProvider>
  );
}
