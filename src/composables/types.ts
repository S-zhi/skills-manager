/**
 * Remote skill from marketplace
 */
export type RemoteSkill = {
  id: string;
  name: string;
  namespace: string;
  sourceUrl: string;
  description: string;
  descriptionZh: string;
  author: string;
  installs: number;
  stars: number;
  marketId: string;
  marketLabel: string;
};

/**
 * Result of skill installation
 */
export type InstallResult = {
  installedPath: string;
  linked: string[];
  skipped: string[];
};

/**
 * Local skill managed by skills-manager
 */
export type LocalSkill = {
  id: string;
  uuid: string;
  name: string;
  description: string;
  path: string;
  source: string;
  sourceUrl?: string;
  ide?: string;
  usedBy: string[];
};

export type LocalSkillPreview = {
  skillMdPath: string;
  skillMdContent: string;
};

export type SkillLibraryEntry = {
  uuid: string;
  favorite: boolean;
  tags: string[];
};

export type SkillLibraryStore = {
  schemaVersion: number;
  revision: number;
  entries: SkillLibraryEntry[];
};

export type SkillPackage = {
  id: string;
  name: string;
  description: string;
  skillUuids: string[];
  createdAt: number;
  updatedAt: number;
};

export type SkillPackageStore = {
  schemaVersion: number;
  revision: number;
  packages: SkillPackage[];
};

/**
 * Skill found by recursively scanning a user-selected directory.
 * Discovery is read-only; the skill is not copied into manager storage.
 */
export type DiscoveredSkill = {
  id: string;
  uuid?: string;
  name: string;
  description: string;
  path: string;
  skillMdPath: string;
  provider: string;
  isStandard: boolean;
  issues: string[];
};

export type ManagerStorageInfo = {
  rootPath: string;
  skillsPath: string;
  pluginsPath: string;
  legacySkillsPath: string;
  legacyExists: boolean;
};

export type SkillImportStatus = "imported" | "skipped" | "failed";

export type SkillImportItemResult = {
  sourcePath: string;
  name: string;
  targetPath?: string;
  status: SkillImportStatus;
  message: string;
};

export type BatchImportResult = {
  items: SkillImportItemResult[];
  imported: number;
  skipped: number;
  failed: number;
};

/**
 * Skill in IDE directory
 */
export type IdeSkill = {
  id: string;
  name: string;
  path: string;
  ide: string;
  source: string;
  managed: boolean;
};

/**
 * Overview of all skills
 */
export type Overview = {
  managerSkills: LocalSkill[];
  ideSkills: IdeSkill[];
};

/**
 * IDE configuration option
 */
export type IdeOption = {
  id: string;
  label: string;
  globalDir: string;
};

/**
 * Link target for skill installation
 */
export type LinkTarget = {
  name: string;
  path: string;
};

/**
 * Download task in queue
 */
export type DownloadTask = {
  id: string;
  name: string;
  sourceUrl: string;
  skillUuid?: string;
  targetPath?: string;
  action: "download" | "update";
  status: "pending" | "downloading" | "done" | "error";
  error?: string;
};

/**
 * IDE directory in a project
 */
export type ProjectIdeDir = {
  label: string;
  relativeDir: string;
  absolutePath: string;
};

/**
 * Project configuration
 */
export type ProjectConfig = {
  id: string;
  name: string;
  path: string;
  ideTargets: string[];
  detectedIdeDirs: ProjectIdeDir[];
};

export type TranslationSettingsView = {
  schemaVersion: number;
  revision: number;
  basic: {
    provider: "azure" | "deepl" | "google" | "mymemory" | "libretranslate";
    endpoint: string;
    region: string;
    sourceLanguage: string;
    targetLanguage: string;
  };
  advanced: {
    provider: "gemini" | "openai-compatible";
    baseUrl: string;
    model: string;
    temperature: number;
    preserveStructure: boolean;
  };
  basicApiKeyConfigured: boolean;
  advancedApiKeyConfigured: boolean;
  configPath: string;
};
