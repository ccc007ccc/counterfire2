# Third Party Notices

CounterFire 2 使用以下第三方项目和素材。完整 Rust crate 版本以 `Cargo.lock` 为准，完整 npm 包版本以前端 lockfile 为准。

## Runtime and libraries

- overlay-engine / core-server — overlay IPC、named pipe、共享内存画布协议。项目地址：<https://github.com/ccc007ccc/overlay-engine>
- cs2-gsi — CS2 Game State Integration listener 和 cfg 写入能力。
- Tauri — 桌面应用框架。
- React — 控制面板 UI。
- Vite / TypeScript / Tailwind CSS — 前端构建与样式工具链。
- tokio / serde / anyhow / tracing / parking_lot / windows / rodio / image / rand / bytes — Rust runtime、序列化、错误处理、日志、Windows API、音频和图像处理。

## Assets

`assets/` 中的 PNG/OGG 素材随本项目发布。项目维护者确认这些素材可公开再分发。替换或新增素材时，请在提交前确认授权允许源码分发和 release 二进制分发。

## Licenses

CounterFire 2 本体使用 MIT License。第三方依赖保留各自许可证和版权声明；分发 release 时请同时保留本文件、`LICENSE`、Rust/npm lockfile 中对应依赖信息。
