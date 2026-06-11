# 设计说明

## 触发方式：空行 Tab + 独立的 Shpell 模式 UI

zsh 集成（`src/shell/shpell.zsh`）只做一件事：空行按 Tab 时挂起 zle，以
fzf-widget 的方式启动 `shpell compose`（stdin/stderr 接 tty，stdout 被捕获），
结束后按退出码处理 —— `0` 把命令放上 prompt（不执行，由用户决定下一步），
其余取消。非空行的 Tab 委派给原有补全 widget（兼容 fzf-tab 等）。

整个交互界面（图标、流式输出、spinner 动画、追问循环）都在 `shpell compose`
（`src/compose.rs`）里完成，**完全不经过 zle**。自然语言从不接触 shell
解析，因此没有语法高亮误判、history expansion（`!`）、PS2 续行这些问题；
spinner 也不会与 zle 重绘互相干扰。

之前迭代过并放弃的方案：

- **前缀词触发（`x ` / `@`）+ 重载 `accept-line`**：自然语言留在 zle buffer
  里，`?` `>` `$` 会被语法高亮插件按 shell 语义涂色，需要额外 hook 覆盖
  高亮；流式重绘、spinner（`zle -M`）与 zle 显示机制纠缠，边界问题多。
- **`!` 前缀**：与 history expansion 冲突。
- **`#` 前缀**：需 `interactive_comments`，且整行被高亮成注释。
- **无前缀自然语言分类**：误判会拦截正常命令，每次回车都有延迟。

## Provider 抽象

`src/provider/mod.rs` 定义 `Provider` trait（输入自然语言 + shell/os/cwd 上下文，
输出单行命令），由 `config.toml` 的 `provider` 字段选择实现。目前唯一实现
`openai-chatgpt`：

- 认证走 OpenAI 官方 Codex 公共客户端的 OAuth PKCE 流程
  （`auth.openai.com`，本机 1455 回调），即 openclaw / opencode 接 ChatGPT
  订阅的同一套机制，token 存于 `~/.local/share/shpell/auth.json`（0600），
  过期前自动 refresh。
- 请求打到 `chatgpt.com/backend-api/codex/responses`（Responses API、
  SSE-only、`store: false`，需 `ChatGPT-Account-Id` header），按订阅计费，
  不消耗 API 余额。

新增 provider（如 Anthropic、本地模型）只需实现 trait 并在
`provider::from_config` 注册。

## Shell 拓展

`shpell init <shell>` 输出对应 shell 的集成脚本，目前支持 zsh
（`src/shell/shpell.zsh`）和 bash（`src/shell/shpell.bash`）；新增 shell 在
`src/shell/` 加脚本并在 `init_script` 注册即可。

### bash 集成（`src/shell/shpell.bash`，需 bash ≥ 4）

bash 的 readline 无法像 zle widget 那样在按键处理中条件分发，因此 Tab 被
绑定为一个两键宏：

1. 第一个键是 `bind -x` handler。行非空时把第二个键重绑回原来的补全函数
   （加载时从 `bind -p` 捕获，默认 `complete`）；行为空时运行
   `shpell compose`（stdout 被 `$(...)` 捕获，退出码 0 则写入
   `READLINE_LINE`），并把第二个键绑成空宏吞掉。
2. 交互结束后 handler 向终端发 `\e[5n`（设备状态查询），终端回应的
   `\e[0n` 被绑定到 `redraw-current-line`，使 readline 在
   `shpell compose` 留下的光标处重画 prompt —— 与 fzf 的重绘技巧相同。

`READLINE_LINE` 自 bash 4.0 才有，脚本对 bash 3.x（macOS 系统自带）静默
退化为不安装。
