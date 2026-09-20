# Skills Manager

[English](README.md) | [中文](README_zh-CN.md)

**将 skill 快速安装到 全局/项目** 
一款专业的跨平台 AI Skills 管理器。支持从各大主流技能市场（如 Claude Plugins、SkillsLLM、SkillsMP 等）进行聚合搜索，一键下载至统一的本地仓库，并可通过符号链接（Symlink）极速分配到任意受支持的 AI 开发环境中。全面兼容 Windows、macOS 与 Linux 操作系统，让您的 AI 编程助手能力无限扩展。

![Local](docs/screenshots/zh-CN/local.png)
![Market](docs/screenshots/zh-CN/market.png)
![IDE](docs/screenshots/zh-CN/ide.png)

## ✨ 核心特性

- 🔍 **聚合市场检索**：基于公开 Registry，一站式搜索全网优质 Skills
- 📦 **统一本地仓库**：集中化管理下载内容 (`~/.skills-manager/skills`)
- 🚀 **一键极速分发**：以系统软链接形式，将统一的本地 Skills 秒级安装至各个目标 IDE
- 🛠️ **多维管理界面**：支持基于 IDE 的细粒度浏览、无痕安全卸载机制
- ⚙️ **项目管理**：支持项目管理，将 skills 挂载到项目下，可以配置项目使用的 ide

## 🎯 原生支持的 IDE（按字母顺序）

- **Antigravity**: `.gemini/antigravity/skills`
- **Claude Code**: `.claude/skills`
- **CodeBuddy**: `.codebuddy/skills`
- **Codex**: `.codex/skills`
- **Cursor**: `.cursor/skills`
- **Kiro**: `.kiro/skills`
- **OpenClaw**: `.openclaw/skills`
- **OpenCode**: `.config/opencode/skills`
- **Qoder**: `.qoder/skills`
- **Trae**: `.trae/skills`
- **VSCode**: `.github/skills`
- **Windsurf**: `.windsurf/skills`

## 📖 使用指南

### 📥 获取与使用

- **普通用户（推荐）**：直接前往 [Releases 页面](https://github.com/Rito-w/skills-manager/releases) 下载最新版本安装包即可。
- **开发者**：拉取源码在本地运行，或进行深度定制。

### 🍎 macOS 安全使用要求

由于目前暂时未配置 Apple 开发者商业证书，初次打开应用可能会遇到“已损坏，无法打开”或提示“未知的开发者”等系统拦截。作为开发者或极客用户，您可以在终端执行以下命令进行安全放行：

```bash
xattr -dr com.apple.quarantine "/Applications/skills-manager-gui.app"
```

### 🔍 1) 市场浏览 (Market)

- 基于配置好的服务源，聚合展示全网可用的优质 Skills。
- 点击下载将自动入库至本地，若本地仓库已存在较旧版本，将高亮显示“更新”按钮。

### 🗂️ 2) 本地仓库 (Local Skills)

- 集中俯瞰已下载到设备底层仓库的所有 Skills。
- 点击“安装”，即可在弹出的面板中勾选一个或多个原生 / 自定义的 IDE 实施批量挂载发布。

#### Windows：递归发现目录中的 Skills

在“已有 Skills”页面点击“发现目录中的 Skills”，选择任意 Windows 文件夹后，应用会递归查找该目录及所有子目录中的 `SKILL.md`。扫描是只读操作：不会复制、移动或修改发现到的文件，也不会自动将其加入 Skills Manager 本地仓库。

发现结果会显示 Skill 来源。应用会根据路径识别通用 `.agents/skills`，以及 Codex、Claude Code、Cursor、Gemini/Antigravity、VS Code/GitHub Copilot、Windsurf、Qoder、Trae、Kiro、CodeBuddy、OpenClaw 和 OpenCode 等 Agent CLI 的 Skill 目录；无法判断来源时显示为通用 `SKILL.md`。

每个结果都会标记是否为“规范 Agent Skill”。当前检查规则为：

- 文件包含以 `---` 开始和结束的 YAML frontmatter；
- frontmatter 包含非空的 `name` 和 `description`；
- `name` 长度为 1–64，只包含小写字母、数字和单连字符，连字符不位于首尾；
- `name` 与 Skill 目录名一致；
- `description` 不超过 1024 个字符。

只要存在 `SKILL.md` 就会展示，因此 Agent CLI 自定义的兼容 Skill 即使不满足上述规范也不会被隐藏；界面会列出具体的不规范原因。

#### 本地仓库与复制行为

Skills Manager 自己维护统一仓库：Windows 上为 `%USERPROFILE%\.skills-manager\skills`（其他系统为 `~/.skills-manager/skills`）。不同操作的文件行为如下：

| 操作 | 是否复制到统一仓库 | 说明 |
| --- | --- | --- |
| 发现目录中的 Skills | 否 | 只读递归扫描并显示规范状态 |
| 导入本地 Skill | 是 | 将用户选中的单个 Skill 目录复制到统一仓库 |
| 纳入统一管理 | 是 | 先复制 IDE 中的 Skill 到统一仓库，再用链接替换原目录 |
| 从市场下载 | 是 | 下载并解压到统一仓库 |
| 安装到 IDE | 通常否 | 优先创建符号链接；Windows 下失败时使用目录联接 |
| Windows 安装到 Qoder | 是 | Qoder 目标使用副本，并写入 `.skills-manager-source` 来源标记 |

目录发现与导入是两个独立动作。本阶段不会在发现后自动复制；需要纳入管理时仍使用“导入本地 Skill”或 IDE 页面的“纳入统一管理”。

### ⌨️ 3) IDE 纬度管理 (IDE Browse)

- 灵活切换工作环境视角（如 VSCode 或 Cursor），独立查看各自已挂载使用的技能列表。
- 安全卸载模块：仅安全移除软链接；若非软链接则执行物理剔除，相互防干扰。
- 找不到您的生产力工具？只需在右上角轻松创建你的“自定义 IDE”。

## 👨‍💻 安装与开发

### 环境依赖

- Node.js (建议 LTS)
- Rust (通过 rustup 安装)
- macOS: Xcode Command Line Tools

### 本地开发

```bash
pnpm install
pnpm tauri dev
```

### 打包发布

```bash
pnpm tauri build
```

## 📡 远程数据来源

- **Claude Plugins**: `https://claude-plugins.dev/api/skills`
- **SkillsLLM**: `https://skillsllm.com/api/skills`
- **SkillsMP**: `https://skillsmp.com/api/v1/skills/search`（由于跨域限制可能需要提供 API Key 配置）
- 下载 ZIP 请求代理: `https://github-zip-api.val.run/zip?source=<repo>`

## 🛠 技术栈

- 桌面端核心框架：**Tauri 2**
- 前端视图层：**Vue 3** + **TypeScript** + **Vite**
- 底层文件与系统逻辑：**Rust** (命令侧)

## 📄 License

TBD
