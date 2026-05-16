# Security Policy

## Supported versions

首个公开版本发布后，仅当前最新 release 接收安全修复。

## Reporting a vulnerability

请通过 GitHub Security Advisory 或 issue 私下/公开报告安全问题。报告时请包含：

- 影响版本。
- 复现步骤。
- 预期影响。
- 相关日志或截图。

## Security boundaries

- CounterFire 2 只监听 localhost GSI，不暴露公网服务。
- localhost GSI 请求可被本机其他进程伪造，因此不要把 GSI 数据当作安全边界。
- CounterFire 2 依赖 overlay-engine 的本机 named pipe 和共享内存能力；请只运行来自可信来源的 overlay-engine。
- 不要把包含个人路径、账号信息或本地环境细节的日志直接公开上传。
- 不要在配置、issue 或 pull request 中提交密钥、token、`.env` 或凭据文件。
