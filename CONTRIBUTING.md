# Contributing

感谢你改进 CounterFire 2。提交前请保持改动聚焦，避免把不相关重构混入功能或修复。

## Development setup

```bash
npm --prefix frontend install
cargo tauri dev
```

请先启动 overlay-engine，并手动打开 Xbox Game Bar 小组件再测试图标显示。

## Checks

提交前至少运行：

```bash
npm --prefix frontend run typecheck
npm --prefix frontend run build
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

涉及 UI 的改动还需要手动验证主流程：启动、停止、Demo 模式、测试触发、运行中调整配置。

## Pull requests

- 说明改动目的和验证方式。
- 不提交 `target/`、`frontend/node_modules/`、`frontend/dist/` 或临时文件。
- 不提交 `.env`、token、凭据或私有路径日志。
- 新增或替换素材时说明素材来源和授权。
