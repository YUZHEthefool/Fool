<!-- DOWNLOAD_START -->
## 📦 下载最新版本

**当前版本**: [v1.0.0](https://github.com/YUZHEthefool/Fool/releases/tag/v1.0.0) (发布于 2025-12-09)

### 快速下载

| 平台 | 架构 | 下载 |
|------|------|------|
| 🐧 Linux | x86_64 | [⬇️ 下载](https://github.com/YUZHEthefool/Fool/releases/download/v1.0.0/fool-1.0.0-x86_64-unknown-linux-gnu.tar.gz) ([校验和](https://github.com/YUZHEthefool/Fool/releases/download/v1.0.0/fool-1.0.0-x86_64-unknown-linux-gnu.tar.gz.sha256)) |
| 🍎 macOS | x86_64 (Intel) | [⬇️ 下载](https://github.com/YUZHEthefool/Fool/releases/download/v1.0.0/fool-1.0.0-x86_64-apple-darwin.tar.gz) ([校验和](https://github.com/YUZHEthefool/Fool/releases/download/v1.0.0/fool-1.0.0-x86_64-apple-darwin.tar.gz.sha256)) |
| 🍎 macOS | aarch64 (M1/M2) | [⬇️ 下载](https://github.com/YUZHEthefool/Fool/releases/download/v1.0.0/fool-1.0.0-aarch64-apple-darwin.tar.gz) ([校验和](https://github.com/YUZHEthefool/Fool/releases/download/v1.0.0/fool-1.0.0-aarch64-apple-darwin.tar.gz.sha256)) |

### 安装说明

```bash
# 解压下载的文件
tar -xzf fool-1.0.0-<target>.tar.gz
cd fool-1.0.0

# 移动到系统路径（可选）
sudo mv fool /usr/local/bin/

# 验证安装
fool --version
```

查看 [所有版本](https://github.com/YUZHEthefool/Fool/releases)
<!-- DOWNLOAD_END -->

<!-- CI_BADGE_START -->
[![CI](https://github.com/YUZHEthefool/Fool/actions/workflows/ci.yml/badge.svg)](https://github.com/YUZHEthefool/Fool/actions/workflows/ci.yml)
[![Release](https://github.com/YUZHEthefool/Fool/actions/workflows/release.yml/badge.svg)](https://github.com/YUZHEthefool/Fool/actions/workflows/release.yml)
<!-- CI_BADGE_END -->

# Fool Shell

一款基于 Rust 编写的、状态机驱动的、原生集成 AI 智能辅助的现代化交互式 Shell。

## 特性

- **状态机驱动**：使用确定性有限自动机 (DFA) 进行命令解析，健壮可靠
- **AI 原生集成**：通过 `!` 前缀无缝唤起 AI 助手，支持 OpenAI API 兼容接口
- **流式输出**：AI 响应实时流式显示，类似打字机效果
- **语法高亮**：命令、参数、字符串等不同元素彩色显示
- **智能补全**：文件路径自动补全，历史命令提示
- **管道与重定向**：完整支持 `|`、`>`、`>>`、`<` 操作符
- **历史记录**：持久化保存命令历史，支持上下文感知

## 安装

### 从源码编译

```bash
# 克隆仓库
git clone [<repository-url>](https://github.com/YUZHEthefool/Fool)
cd fool

# 编译 release 版本
cargo build --release

# 可选：安装到系统
cp target/release/fool ~/.local/bin/
```

### 依赖要求

- Rust 1.70+
- OpenSSL 开发库

## 快速开始

```bash
# 启动交互式 Shell
./target/release/fool

# 执行单条命令
./target/release/fool -c "ls -la | head -5"

# 初始化配置文件
./target/release/fool --init-config

# 查看帮助
./target/release/fool --help
```

## 配置

### 配置文件位置

```
~/.config/fool/config.toml
```

运行 `fool --init-config` 可自动生成默认配置文件。

### 完整配置示例

```toml
# Fool Shell 配置文件

[ui]
theme = "dracula"          # 界面主题
editor = "vim"             # 默认编辑器

[history]
file_path = "~/.local/share/fool/history"
max_entries = 10000        # 历史记录最大条数

[ai]
# AI 触发前缀，默认为 "!"
trigger_prefix = "!"

# OpenAI API 配置（兼容所有 OpenAI V1 格式的接口）
api_base = "https://api.openai.com/v1"
api_key = "sk-xxxxxxxxxxxxxxxxxxxxxxxx"
model = "gpt-4o"
temperature = 0.7

# 上下文管理：AI 读取最近多少条交互记录作为上下文
# 值越大，AI 了解的历史越多，但消耗的 token 也越多
context_lines = 10

# 系统提示词
system_prompt = "You are Fool, a helpful assistant running inside a command-line shell. Be concise and provide direct answers. When suggesting commands, provide them in a way that can be easily copied and executed."
```

## AI 配置详解

### 设置 API Key

有三种方式配置 API Key（按优先级排序）：

#### 方式一：配置文件（推荐用于个人设备）

编辑 `~/.config/fool/config.toml`：

```toml
[ai]
api_key = "sk-your-api-key-here"
```

#### 方式二：环境变量 FOOL_AI_KEY

```bash
export FOOL_AI_KEY="sk-your-api-key-here"
```

#### 方式三：环境变量 OPENAI_API_KEY

```bash
export OPENAI_API_KEY="sk-your-api-key-here"
```

### 使用兼容 API（如 Azure、本地模型等）

```toml
[ai]
# 例如使用本地 Ollama
api_base = "http://localhost:11434/v1"
model = "llama2"

# 或使用 Azure OpenAI
api_base = "https://your-resource.openai.azure.com/openai/deployments/your-deployment"
api_key = "your-azure-key"
model = "gpt-4"
```

### 上下文范围配置

`context_lines` 参数控制 AI 能"看到"多少历史命令：

```toml
[ai]
# 最近 10 条命令（默认值，平衡效果与成本）
context_lines = 10

# 更多上下文（AI 理解更完整，但 token 消耗更大）
context_lines = 50

# 最小上下文（节省 token）
context_lines = 3
```

**上下文包含的信息**：
- 用户执行的命令
- 命令的退出码
- 命令输出摘要（如有）

## 使用示例

### 基本命令

```bash
# 普通命令执行
ls -la
cd /var/log
cat syslog | grep error

# 重定向
echo "hello" > output.txt
cat file.txt >> append.txt

# 管道
ps aux | grep nginx | head -5
```

### AI 助手

在命令行输入 `!` 后跟问题即可唤起 AI：

```bash
# 询问如何操作
! 如何查找当前目录下最大的 10 个文件

# 解释命令
! 解释一下 tar -xzvf 的含义

# 排错帮助
! 上一条命令失败了，帮我分析原因

# 生成命令
! 写一个命令统计当前目录下所有 .rs 文件的行数
```

### 内置命令

| 命令 | 说明 |
|------|------|
| `cd [dir]` | 切换目录 |
| `pwd` | 显示当前目录 |
| `export VAR=val` | 设置环境变量 |
| `unset VAR` | 删除环境变量 |
| `alias` | 管理别名 |
| `history` | 显示历史记录 |
| `clear` | 清屏 |
| `help` | 显示帮助 |
| `exit [code]` | 退出 Shell |

## 快捷键

| 快捷键 | 功能 |
|--------|------|
| `Ctrl+C` | 取消当前输入 |
| `Ctrl+D` | 退出 Shell |
| `↑/↓` | 浏览历史命令 |
| `Tab` | 自动补全 |
| `Ctrl+R` | 搜索历史 |

## 项目结构

```
fool/
├── Cargo.toml          # 项目配置
├── src/
│   ├── main.rs         # 入口点
│   ├── config.rs       # 配置管理
│   ├── parser.rs       # 状态机解析器
│   ├── history.rs      # 历史记录
│   ├── ai.rs           # AI 集成
│   ├── executor.rs     # 命令执行
│   └── repl.rs         # 交互界面
└── README.md
```

## 开发

```bash
# 运行测试
cargo test

# 开发模式运行
cargo run

# 检查代码
cargo clippy
```

## 许可证

MIT License

## 贡献

欢迎提交 Issue 和 Pull Request！
