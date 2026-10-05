# 应用适配器与持久化计划

更新：2026-10-05。

## 已有基础

Core 的 RecognitionSession 复用引擎、运行时、模型与推理缓存。
Rust API、Python Session、Generic C ABI 和串行 JSONL worker 已有进程内持久会话。
CLI 仅在单次命令期间持有会话；WASM 识别仍为实验能力。
Android JNI/iOS C 桥接仍使用旧全局引擎与 StubRuntime，尚未接入 RecognitionSession。

## 剩余工作

| 顺序 | 工作项 | 下一步 |
| --- | --- | --- |
| 1 | 宿主接入 | 核对 Tauri/Office 现有实现，按需复用长驻会话，统一模型重载、健康状态与关闭 |
| 2 | 进程监督 | 明确 worker 生命周期、路径授权、超时终止、崩溃恢复与取消责任 |
| 3 | 移动端桥接 | 将 JNI/iOS 从旧引擎升级为有界会话生命周期，明确真实运行时与模型能力 |
| 4 | 缓存治理 | 统一缓存身份、版本/模型/provider 失效条件和资源释放，避免跨模型或宿主串用 |
| 5 | 可选 daemon | 仅在 CLI 多次调用确有需求时设计；不阻塞当前交付 |

## 持久化边界

- 进程内会话可复用；原生推理指针不能跨进程重启保存。
- 重启后只复用已校验的模型文件、合格 provider 记录和明确的磁盘缓存。
- C 接口使用有界 opaque handle，不暴露 Rust 布局；返回缓冲区遵守唯一释放约定。
- Python 每个 Session 独立持有资源，Core 调用释放 GIL；不采用不可隔离的全局 singleton。
- JSONL 是可信本地、path-only、串行协议；认证、沙箱、强制终止与并行由监督层负责。
- 失效句柄、超时、取消和关闭之后必须返回稳定状态，不污染其他会话。

API 细节见 `crates/ffi/include/latexsnipper_session.h`、`crates/python` 与
[worker 文档](../crates/worker/README.md)。[返回总计划](closing-plan.md)
