# Changelog

## 0.1.2 - 2026-05-21

### Fixed

- 修复观战视角切换时历史击杀被误识别为本地击杀的问题。
- 去重 `PlayerGotKill` 与 `KillFeed` 双来源，避免同一次击杀重复触发。

## 0.1.1 - 2026-05-16

### Fixed

- 修复打包版状态日志浮层缺少高斯模糊的问题。
- 将默认击杀图标垂直位置改为 75%。

## 0.1.0 - 2026-05-16

### Added

- 初始公开版本。
- CS2 GSI 本地击杀事件监听。
- CrossFire 风格击杀图标动画和音效播放。
- Demo 循环和手动测试触发。
- 中文/英文语音、CT/T 阵营选择、自动阵营识别。
- 图标位置、全局图标大小、单事件图标大小、音量和连杀重置时间配置。
- overlay-engine Xbox Game Bar 小组件渲染路径。

### Notes

- Requires overlay-engine: <https://github.com/ccc007ccc/overlay-engine>
- 需要用户手动打开 overlay-engine 的 Xbox Game Bar 小组件。
- Windows 二进制暂未代码签名。
