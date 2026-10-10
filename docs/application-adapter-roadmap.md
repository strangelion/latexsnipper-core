# 应用适配器与持久化计划

更新：2026-10-10。

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

## Obsidian 公式转换接入

以下为明确安排的集成修复，不把现有接口名称当成验收证据。

| 顺序 | 工作项 | 状态与验收边界 |
| --- | --- | --- |
| OBS-01 | 裸公式与行内输出 | Core/Rust/WASM/TS 独立裸公式入口已有，保留 `latex` 文档兼容行为；LaTeX/Typst/MathML/OMML 单公式行内输出实测通过，宿主接入待 OBS-05 |
| OBS-02 | 专用转换 Worker | 官方客户端公式 RPC、队列/预算、硬取消及超时重建已有，真实 Chrome Worker 通过；继续接精简构建和宿主资源加载 |
| OBS-03 | 精简转换 WASM | `conversion-only` 构建、独立依赖门禁及本地 Node/Chrome 实测已有；继续 CI 与实际宿主打包，不将体积和单次启动测量泛化为性能保证 |
| OBS-04 | 原生转换 RPC | 待实现：JSONL 新增版本化公式动作，保留既有识别协议；独立进程监督由桌面宿主负责，移动端不用该路线 |
| OBS-05 | 宿主集成验收 | 待实际宿主：将现有 Obsidian 插件接入该链路，桌面/移动分别验证资源路径、错误/取消与 Markdown 插入；不以 Node 或普通 Chrome 冒充 Obsidian 验收 |

`typst` 的有限重建仍是 best-effort；裸公式投影不保证任意 MathLive/TeX
语法支持、视觉保真或自定义宏可执行。WASI 转换插件不是浏览器转换 Worker。

## 持久化边界

- 进程内会话可复用；原生推理指针不能跨进程重启保存。
- 重启后只复用已校验的模型文件、合格 provider 记录和明确的磁盘缓存。
- C 接口使用有界 opaque handle，不暴露 Rust 布局；返回缓冲区遵守唯一释放约定。
- Python 每个 Session 独立持有资源，Core 调用释放 GIL；不采用不可隔离的全局 singleton。
- JSONL 是可信本地、path-only、串行协议；认证、沙箱、强制终止与并行由监督层负责。
- 失效句柄、超时、取消和关闭之后必须返回稳定状态，不污染其他会话。

API 细节见 `crates/ffi/include/latexsnipper_session.h`、`crates/python` 与
[worker 文档](../crates/worker/README.md)。[返回总计划](closing-plan.md)
