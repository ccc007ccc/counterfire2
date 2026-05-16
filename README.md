# CounterFire 2

CounterFire 2 是一个 Windows 桌面应用，用 CS2 GSI 事件触发 CrossFire 风格的击杀音效和击杀图标动画。应用本身只负责控制面板、GSI 监听、素材载入和向 overlay-engine 发送绘制命令；画面显示依赖 overlay-engine 的 Xbox Game Bar 小组件。

## 功能

- 监听 CS2 Game State Integration 本地击杀事件。
- 按击杀数、爆头、刀杀、雷杀选择不同音效和图标。
- 支持中文/英文语音素材。
- 支持自动识别 CT/T 阵营，也可固定阵营。
- 支持 Demo 模式和手动测试触发。
- 支持运行中调整图标位置、全局图标大小、单个事件图标大小、音量和连杀重置时间。
- 分辨率变化后会重建 Game Bar 画布。

## 运行要求

- Windows 10/11。
- WebView2 Runtime。
- Xbox Game Bar。
- overlay-engine：<https://github.com/ccc007ccc/overlay-engine>
- CS2（GSI 模式需要）。

CounterFire 2 不会自动启动或唤起 overlay-engine 小组件。使用前请先安装并运行 overlay-engine，然后手动打开 Xbox Game Bar 中的 overlay-engine 小组件；否则音频可能播放，但图标不会显示。

## 快速开始

### 使用 release 产物

1. 从 GitHub Releases 下载 portable exe 或 NSIS 安装包。
2. 启动 overlay-engine，并在 Xbox Game Bar 中打开对应小组件。
3. 启动 CounterFire 2。
4. 选择 `CS2 GSI` 或 `Demo 循环`。
5. 点击启动。

首次 release 的 Windows 二进制暂未代码签名，Windows SmartScreen 可能提示未知发布者。

### 从源码运行

```bash
npm --prefix frontend install
cargo tauri dev
```

### 构建

```bash
npm --prefix frontend install
cargo tauri build
```

CLI fallback：

```bash
counterfire2 --cli --demo
counterfire2 --cli --port 57534 --side auto
```

## CS2 GSI

GSI 模式启动后会尝试写入 CS2 GSI cfg。如果自动写入失败，界面状态日志会显示原因。默认端口是 `57534`，如果被占用会尝试 fallback 端口。

本地 GSI HTTP 端点只绑定 localhost，但本机其他进程仍可伪造 GSI 请求。请只在可信本机环境中运行。

## 素材

`assets/` 下包含随项目发布的 PNG/OGG 素材。项目维护者确认这些素材可随源码和 release 再分发；如果你替换素材，请确认替换素材的授权允许分发。

## 与 overlay-engine 的关系

CounterFire 2 依赖 overlay-engine 的 IPC 协议、named pipe 和共享内存画布能力。请优先使用与当前 release notes 声明兼容的 overlay-engine 版本。

依赖项目：<https://github.com/ccc007ccc/overlay-engine>

## 开发验证

```bash
npm --prefix frontend run typecheck
npm --prefix frontend run build
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo build --release
```

## License

MIT。详见 [LICENSE](LICENSE)。
