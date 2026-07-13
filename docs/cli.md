# CLI

`snipper convert` 是语义、visual 和 Office package 输出的统一入口。格式列表、
help、拼写建议与 capability 输出由可执行 registry 生成，不维护第二份常量。

```bash
snipper convert input.docx --to markdown -o output.md
snipper convert input.docx --to pdf -o output.pdf
snipper convert input.json --to png -o output.png
snipper convert input.pdf --to docx -o output.docx
Get-Content input.json -Raw | snipper convert - --to markdown -o -
```

`-` 表示 stdin/stdout。二进制 stdout 默认拒绝，必须显式传入
`--force-binary-stdout`。数据只写 stdout；状态和 diagnostics 只写 stderr。

文件输出默认通过同目录临时文件、flush、sync 和 rename 完成；目标已存在时默认
拒绝，使用 `--force` 明确替换，或用 `--no-clobber` 表达严格策略。二进制 artifact
写入前会通过统一 importer 重开验证。

Diagnostics 支持 `--diagnostics text|json|sarif`、`--strict`、
`--fail-on-warning`、`--quiet` 和 `--verbose`。稳定退出码可由 `snipper version`
查看：0 成功，1 通用失败，2 参数错误，3 输入错误，4 model 错误，5 recognition
错误，6 conversion 不支持，7 输出验证失败，8 strict diagnostics，9 plugin，
10 batch 部分失败。

`snipper doctor` 输出 OS、架构、core/runtime、model 目录、输出目录可写性、
capability 数和 exit-code contract。`models list|download|verify` 继续可用。

尚未实现：glob/recursive batch、completion/man page、plugin 安装管理、models purge。
这些命令不得在 capability 或文档中标记为 production-ready。
