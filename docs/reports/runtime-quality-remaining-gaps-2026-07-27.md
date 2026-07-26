# Runtime quality remaining-gaps report

## A. 本地提交链和基线

基线为 `ba29c27`。首要运行时质量目标由 `592e155` 至 `3edcd4c`
完成；通用应用集成层为 `52ce8dc`；本轮补充实现为 `ac376d6`、
`17142b7`、`f6d8212`。当前分支相对 `origin/main` ahead 14，所有提交
仅在本地，未推送。

## B. 真实数据集构成

保留 50 张、CC0、确定性 synthetic 公式回归集。已实现 real screenshot、
scan、mobile、hard-negative 的版本化 manifest 字段、分组指标和准入规则，
但没有可再分发且完成脱敏/许可审查的真实输入：四组实际准入数均为 0。
表格真实集同样为 0/30。没有使用个人文件、重复图片或 synthetic 冒充真实
分布。

## C. 真实模型公式指标

TrOCR DeIT encoder/decoder 通过 ONNX Runtime CPU 实际跑完 50 张 synthetic
图片，预测未由 ground truth 回填。模型组合 SHA-256 为
`c68629f7efe6b51e05833617f630aee90551dd505064a4a2d8e2529d11bff7f8`。

- exact / normalized exact：0 / 0
- CER：1.3022407503908286
- TER：0.6284916201117319
- parse / structure / balanced：1.0 / 1.0 / 1.0
- latency p50 / p95：1779.3855 / 3707.1481 ms

这是实际模型在 synthetic 分布上的失败基线，不是质量达标声明。JSON、CSV
和逐样本 prediction bundle 均已提交。

## D. hard-negative 与修正回归

指标实现并进入 JSON/CSV：FPR、FNR、修正触发/改善/回归、review required、
confidence calibration，以及按 scale、degradation、序列长度分组。当前
synthetic 公式集上的 trigger 为 0.02，improvement 为 0，regression 为 0，
review required 为 0.02，calibration error 为 0.9666018259525299。
由于 hard-negative 实际样本为 0，当前 FPR 数值不构成可用证据。

## E. decoder artifact 搜索结果

新增搜索器实际检查工作树、Git history/reflog、指定旧目录、GitHub Releases
和 Actions metadata。报告含 170 个去重候选的大小、SHA-256、格式及
`while`/`Add.34` 字节观察，`decoder_step` 候选为 0，状态为 blocked。
外部目录路径和文件名均替换为搜索根编号及内容哈希编号。

## F. 29 状态证据或阻塞

PP-FormulaNet Paddle PIR 静态图中发现一个 `while`：30 个输入（条件加 29
个 block arguments）、29 个输出。因此只能证明 29 个状态候选的静态数量。
本机没有 Paddle runtime，无法捕获 step 0/1/2/3/6/9 的真实变量名、shape、
增长轴和 token prefix；语义映射保持 unknown/low confidence，
`stateSchemaFrozen=false`。未生成伪造 fixture。

## G. Add.34 根因或阻塞

blocked。没有找到可运行的 incremental decoder graph，因而无法观测两个
producer、实际广播 shape 或完成 T=1/2/3/6/9/15/30 的 logits/top-k/KV
对齐。没有撰写虚构根因或伪造回归结果。

## H. provider 验证层级

固定小 ONNX、固定输入和 `1e-6` tolerance 被用于真实 session/inference：

| Provider | 实际层级 | Session | Smoke | 说明 |
|---|---|---:|---:|---|
| CPU | BenchmarkValidated | yes | yes | 输出 SHA-256 `1c14430e…a193` |
| DirectML | SmokeInferencePassed | yes | yes | 与 CPU 在 tolerance 内一致 |
| CUDA | ProbePassed | no | no | provider shared library 缺失，session create failed |
| TensorRT | ProbePassed | no | no | provider shared library 缺失，session create failed |
| CoreML | Declared | no | no | Windows 不支持 |

readiness 不再把 descriptor 或 resolution 当成 runnable；Office-facing 状态
在应用 warmup 前不会误报 ready，之后依据真实模型加载结果。

## I. AST/API 大小和兼容性

Windows x86_64 实测：`Inline` 712 -> 712 bytes（0%），`Formula` 672 ->
688 bytes（+16，2.38%），当前 `RecognitionProvenance` 160 bytes，
`TransformationEvidence` 128 bytes。旧 JSON、新 JSON 缺 evidence、`null`、
未来未知字段和 legacy snapshot 均通过。

序列化实测：

- 100 formulas：inline 138,061 bytes；registry prototype 86,716 bytes
- 1,000 formulas：inline 1,384,561 bytes；registry prototype 866,116 bytes
- 10,000 table cells：838,895 bytes

registry 仅为评估原型，没有破坏当前 AST/JSON。

## J. 表格 TEDS 和单元格指标

已实现 ordered-tree TEDS、structure exact、span/cell/empty accuracy、text
CER、formula normalized exact、reading order 和按 cell count 延迟的 runner。
歧义区间只双跑中置信度单元格，并输出 `selectedCandidate`、
`candidateScores`、`selectionReason`；A1、x、日期、百分数、H2O、No.3、
希腊字母和普通上标文本的 hard-negative 测试通过。crop 引用为显式同意、
hash-addressed、限额保留、默认关闭且不嵌入 AST。

由于没有合格的 30 张真实表格集，真实 TEDS 和 cell 指标保持 blocked；
没有用四张已有图片扩充或造数。

## K. mmap 生命周期实验

`runtime-mmap-experimental` 使用 ORT
`commit_from_memory_directly` 建立真实借用生命周期，production
`memoryMapModel=true` 仍 fail-closed。Windows 实测：

- read 0.8428 ms；mmap 0.4935 ms；page touch 0.0128 ms
- session create + optimize 193.3975 ms
- first / warm inference 2.2390 / 1.4360 ms
- reload session / first inference 3.4440 / 2.5889 ms
- working set / peak 34,349,056 bytes；private 19,038,208 bytes

原子指针切换后旧 session 继续推理，新 session 打开不同 SHA 的新版本；
映射期间 delete/replace 和实验目录清理均在本机成功。该隔离结果不等于
production RSS/cold-start 收益。

## L. 依赖重复风险

`cargo tree -d --edges normal,build` 已分类。`base64` 重复主要来自 tokenizer
旧依赖；`digest/sha2` 分属项目与 PDF crypto；`ureq/webpki-roots` 分属
runtime 和 ORT build；`getrandom/rand` 为三代图像/tokenizer/PDF 栈；
`ndarray` 0.16/0.17 是项目与 ORT 的类型边界。它们涉及 TLS、随机数、
crypto 或类型不兼容，本轮不做高风险强制收敛。

## M. 测试证据 JSON 和产物 SHA

机器证据基于干净提交 `f6d8212`：5 条命令全部 exit 0，278 passed、0
failed、0 ignored、0 filtered、0 Clippy warnings。完整
`cargo test --locked --workspace` 在 Windows 降并行度至 2 后为 942 passed、
0 failed、2 ignored；全 workspace/all-targets/all-features Clippy 以
`-D warnings` 通过。第一次默认高并行完整测试触发 Windows `os error 1450`
资源不足；低并行重跑完整通过。

证据 JSON 列出 dataset manifest、prediction、benchmark、provider、
type-size、mmap 和 decoder 报告的相对路径、大小及 SHA-256。CI 新增
Windows 机器摘要 artifact、三平台 CPU provider smoke artifact；无物理
accelerator runner 的结果不会伪造。

## N. 未完成项

1. 合法可再分发的 real screenshot/scan/mobile/hard-negative 数据及真实指标。
2. 30 张真实表格数据及真实 TEDS/cell 指标。
3. 可运行 decoder_step、Paddle runtime state capture、29 状态语义冻结和
   Add.34 对齐闭环。
4. Linux CUDA、Windows accelerator CI、macOS CoreML 等对应物理 runner
   上的 smoke/benchmark。
5. mmap production cache/session owner 集成；实验 feature 不自动升级能力。

这些项目需要新的数据许可、模型 artifact、运行时或硬件，不以模拟结果替代。

## O. 提交 SHA

- `592e155`–`3edcd4c`：首要运行时质量目标
- `52ce8dc`：通用应用集成层
- `ac376d6`：剩余质量证据主实现
- `17142b7`：全特性 Clippy 指针位数修复
- `f6d8212`：规范化机器测试摘要

最终报告和证据清单将在独立文档提交中固化。未执行 push。

