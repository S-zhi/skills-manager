import type { IdeOption } from "./types";

/**
 * Default IDE options available for skill installation
 */
export const defaultIdeOptions: IdeOption[] = [
  { id: "antigravity", label: "Antigravity", globalDir: ".gemini/antigravity/skills" },
  { id: "claude", label: "Claude Code", globalDir: ".claude/skills" },
  { id: "codebuddy", label: "CodeBuddy", globalDir: ".codebuddy/skills" },
  { id: "codex", label: "Codex", globalDir: ".codex/skills" },
  { id: "cursor", label: "Cursor", globalDir: ".cursor/skills" },
  { id: "kiro", label: "Kiro", globalDir: ".kiro/skills" },
  { id: "openclaw", label: "OpenClaw", globalDir: ".openclaw/skills" },
  { id: "opencode", label: "OpenCode", globalDir: ".config/opencode/skills" },
  { id: "qoder", label: "Qoder", globalDir: ".qoder/skills" },
  { id: "trae", label: "Trae", globalDir: ".trae/skills" },
  { id: "vscode", label: "VSCode", globalDir: ".github/skills" },
  { id: "windsurf", label: "Windsurf", globalDir: ".windsurf/skills" }
];

/**
 * LocalStorage keys
 */
export const STORAGE_KEYS = {
  IDE_OPTIONS: "skillsManager.ideOptions",
  INSTALL_TARGETS: "skillsManager.lastInstallTargets"
} as const;
