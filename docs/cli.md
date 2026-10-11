# CLI

`snipper convert` 是语义格式、visual export 和 Office package 输出的统一入口。格式帮助、
拼写建议和 capability 输出均来自可执行 importer/exporter registry。

```bash
snipper convert input.docx --to markdown -o output.md
snipper convert input.docx --to pdf -o output.pdf
snipper convert input.json --to png -o output.png
snipper convert input.pdf --to docx -o output.docx
Get-Content input.json -Raw | snipper convert - --to markdown -o -
```

`-` 表示 stdin/stdout。二进制 stdout 默认拒绝，只有显式指定
`--force-binary-stdout` 才会启用。数据只写 stdout，状态与 diagnostics 只写 stderr。

文件输出默认采用同目录临时文件、flush、sync 和 rename；目标存在时默认拒绝覆盖。
二进制 artifact 在 rename 前由 importer 重新打开验证。Diagnostics 支持
`text|json|sarif`、strict、fail-on-warning、quiet 和 verbose。

## Registered single-formula conversion

`formula` 是独立的单公式入口，不改变原 `convert` 文档路线，也不直接修改 Office。

```text
snipper formula capabilities
snipper formula convert --from latex --to omml --input formula.tex --context-sha256 <CALLER_CONTEXT_SHA256>
snipper formula convert --from latex --to omml --input formula.tex --context-sha256 <CALLER_CONTEXT_SHA256> --backend unavailable:route --fallback core:builtin
```

输入文件或 `--input -` 的 UTF-8 stdin 最多 64 KiB。`--mode strict|best-effort`、规范格式 ID、
显式 `--backend` 与最多三个重复 `--fallback` 使用同一注册器；只允许执行前可用性选择，
校验或执行失败不自动重试。stdout 输出有界 JSON 决策报告（最多 512 KiB），拒绝结果返回非零退出码；
参数、读取、配置或激活失败只写 stderr，不产生候选。没有选择后端时只调用 `core:builtin`。

上下文参数必须是 64 位小写十六进制 SHA-256，由调用方绑定定义、字体、样式及真实宿主身份。
CLI 再绑定实际运行的可执行文件摘要、Core 版本及规范化 profile 摘要；包版本不是 build 身份。
每次进程启动会重新计算可执行身份，本入口没有承诺批量转换速度或持久缓存。

默认构建不含 WASI 公式加载器，显式构建后才接受 [v1 profile](formats/conversion-registry.md#声明式-wasi-profile-与-cli)：

```text
cargo build --locked -p latexsnipper-cli --features wasi-formula
snipper formula capabilities --profile power-profile.json --plugin-store ./remote-store
snipper formula convert --from publisher:power --to omml --input power.txt --context-sha256 <CALLER_CONTEXT_SHA256> --profile power-profile.json --plugin-store ./remote-store --backend publisher:import --enable-profile-backend
```

`remote-store` 必须已通过既有可信安装流程安装并显式启用对应包。profile 不安装、联网、授权、
启用或撤销包；新注册后端仍默认禁用，`--enable-profile-backend` 只启用本次 registry 的该条路线。
默认 store 为 `.snipper/plugins/signed-registries/remote`。WASI profile 命令使用固定 8 MiB 宿主工作线程栈，
避免 Windows 主线程栈不足；不放宽 guest 内存、fuel、epoch、并发或执行超时，也不把编译阶段当 guest 执行。

## Batch

`convert` 支持多输入、glob、递归目录、相对路径保留、并发上限、continue-on-error
和 JSON 报告：

```bash
snipper convert "docs/**/*.docx" --to pdf --output-dir converted \
  --jobs 4 --continue-on-error --report batch-report.json
```

## Operations

```bash
snipper doctor
snipper models list
snipper models download
snipper models verify
snipper models purge --yes
snipper plugin list
snipper plugin verify ./my-plugin
snipper plugin install ./my-plugin
snipper plugin enable example.plugin
snipper plugin doctor
snipper capabilities --format json
snipper capabilities --format json --api-version 2
snipper migrate plugin-manifest plugin.json --json
snipper migrate model-manifest model-manifest.json --json
snipper migrate document document.json --json
snipper migrate inspect unknown.json --json
snipper validate input.docx
snipper completions bash
snipper completions zsh
snipper completions fish
snipper completions powershell
snipper manpages --output-dir ./man
```

`models purge` 只删除模型根目录下经过路径校验的 variant，并保留 manifest；必须显式
传入 `--yes`。legacy 外部 process plugin 本地安装会强制检查 manifest、API/core 版本、
entrypoint 边界与 SHA-256。manifest-v3 WASI package 必须通过 signed registry 安装，安装后
保持 disabled；显式 enable 时再次校验，只有已授权的 capability 才会进入运行时矩阵。
Native ABI 不会被重新解释为 v3 trusted plugin。

`recognize` 与 `job run` 的 `--format` 始终决定输出语义，文件扩展名不会静默覆盖它。
完整选项传播/拒绝审计见 [CLI option matrix](cli-option-matrix.md)。迁移命令默认写入新的
同级文件并保护源文件；需要人工处理时返回退出码 11 且不写输出。

稳定退出码可由 `snipper version` 查看：0 success，1 generic，2 arguments，3 input，
4 model，5 recognition，6 conversion，7 output validation，8 strict diagnostics，
9 plugin，10 partial batch failure，11 migration requires manual action。
