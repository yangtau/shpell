# shpell

**用自然语言写命令行命令。** 对 shell 念一句咒语（spell），它变出命令。

```
❯ create an empty file named test
✻ touch test
m3 :: ~/.config ‹main*› » touch test
```

在 zsh 或 bash 里空行按 **Tab** 进入 Shpell 模式（一个独立于 shell prompt 的交互界面）：
在 ❯ 后输入自然语言（支持方向键、粘贴、↑ 回溯本轮历史），生成的命令在 ✻
后流式出现，生成期间 ✻ 脉冲闪动。然后：

- **空行回车** — 接受：回到 zsh，命令放到 prompt 上但**不执行**，由你决定
  运行、修改还是丢弃；整段对话保留在屏幕上
- **继续输入** — 追问，基于上一条命令继续修改
- **Esc / Ctrl-C / Ctrl-D** — 取消

## 演示

> **TODO(@yangtau)：** 录一段演示视频 / GIF 放在这里。

## 特性

- **零打扰**：只在空行按 Tab 时触发，非空行的 Tab 仍是原来的补全
  （兼容 fzf-tab 等插件）；生成的命令永远停在 prompt 上等你确认，不会自动执行
- **流式生成**：命令边生成边显示，带脉冲 spinner 动画
- **多轮追问**：命令不满意可以继续用自然语言修改
- **用 ChatGPT 订阅**：OAuth 登录，无需 API key，不消耗 API 余额
- **干净的实现**：交互在独立进程里完成，不经过 zle —— 自然语言永远不会
  被 shell 解析、高亮或做 history expansion

## 安装

### Nix

```sh
nix profile install github:yangtau/shpell
# 或在 flake 中引用 inputs.shpell.url = "github:yangtau/shpell";
```

### Cargo

```sh
cargo install --path .
```

### 预编译二进制

从 [GitHub Releases](https://github.com/yangtau/shpell/releases) 下载对应平台的
压缩包（Linux x86_64 / arm64 / armv7，均为 musl 静态链接；macOS arm64 /
x86_64），解压后放进 `PATH` 即可。

## 快速开始

1. 登录（使用 ChatGPT 订阅，OAuth，无需 API key）：

   ```sh
   shpell auth login
   ```

   会打印一个登录 URL，在浏览器中完成授权即可（回调监听本机 1455 端口，
   与 Codex CLI 相同的方式）。

2. 在 `~/.zshrc` 末尾加入（bash 用户（4.0+）改在 `~/.bashrc`）：

   ```sh
   eval "$(shpell init zsh)"    # zsh
   eval "$(shpell init bash)"   # bash
   ```

3. 开新终端，空行按 Tab，开始用自然语言写命令。

也可以不装 shell 集成直接用：`shpell gen -- "find large files"` 或
`shpell find large files`。

## 配置

`~/.config/shpell/config.toml`（可选）：

```toml
provider = "openai-chatgpt"   # 目前唯一支持的 provider
model = "gpt-5.4-mini"
reasoning_effort = "low"       # minimal | low | medium | high
```

Shpell 模式的图标（纯 Unicode，任意字体可显示；`export` 后生效）：

| 变量 | 默认 | 说明 |
|---|---|---|
| `SHPELL_USER_ICON` | `❯` | 用户输入行的图标 |
| `SHPELL_AI_ICON` | `✻` | AI 输出行的图标（生成完成后的静止态） |

## 设计

为什么用 Tab 触发而不是前缀词、shell 集成和 `shpell compose` 进程如何分工，
见 [docs/design.md](docs/design.md)。

## License

[MIT](LICENSE)

---

English documentation: [README.md](README.md)
