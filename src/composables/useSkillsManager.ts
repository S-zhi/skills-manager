import { computed, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import { dirname, homeDir, join } from "@tauri-apps/api/path";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useToast } from "./useToast";
import type {
  RemoteSkill, InstallResult, LocalSkill,
  IdeSkill, Overview, LinkTarget, DownloadTask, ProjectConfig, DiscoveredSkill,
  ManagerStorageInfo, BatchImportResult, SkillImportItemResult
} from "./types";
import { buildProjectLinkTargets } from "./projectTargets";
import { useIdeConfig } from "./useIdeConfig";
import {
  isSafeRelativePath,
  getErrorMessage,
  isSafeAbsolutePath,
  parseManualSkillSource
} from "./utils";

export function useSkillsManager() {
  type SearchResponse = { skills: RemoteSkill[]; total: number; limit: number; offset: number; hasNext?: boolean; dailyRemaining?: number | null };
  const { t } = useI18n();
  const toast = useToast();
  const cacheTtlMs = 10 * 60 * 1000;
  const searchCache = new Map<
    string,
    { timestamp: number; data: SearchResponse }
  >();
  const activeTab = ref<"local" | "packages" | "market" | "ide" | "projects" | "settings" | "trash">("local");

  const query = ref("");
  const marketSource = ref<"cached" | "skillsmp">("cached");
  const marketError = ref("");
  const dailyRemaining = ref<number | null>(null);
  const onlineHasMore = ref(false);
  let lastSearchKey = "";
  const results = ref<RemoteSkill[]>([]);
  const total = ref(0);
  const limit = ref(20);
  const offset = ref(0);
  const loading = ref(false);
  const installingId = ref<string | null>(null);
  const updatingId = ref<string | null>(null);

  // Local Skills
  const localSkills = ref<LocalSkill[]>([]);
  const ideSkills = ref<IdeSkill[]>([]);
  const localLoading = ref(false);
  const discoveredSkills = ref<DiscoveredSkill[]>([]);
  const discoveryRoot = ref("");
  const discoveryLoading = ref(false);
  const discoveryImporting = ref(false);
  const discoveryImportResults = ref<Record<string, SkillImportItemResult>>({});
  const managerStorage = ref<ManagerStorageInfo | null>(null);

  // Download Queue
  const downloadQueue = ref<DownloadTask[]>([]);
  let isProcessingQueue = false;

  // Timer tracking for cleanup
  const timers: number[] = [];

  // Cleanup on unmount
  onUnmounted(() => {
    timers.forEach((id) => clearTimeout(id));
  });

  const showInstallModal = ref(false);
  const installTargetSkills = ref<LocalSkill[]>([]);
  const installTargetIde = ref<string[]>([]);

  const showUninstallModal = ref(false);
  const uninstallTargetPath = ref("");
  const uninstallTargetName = ref("");
  const uninstallTargetPaths = ref<string[]>([]);
  const uninstallMode = ref<"ide" | "local">("ide");

  const busy = ref(false);
  const busyText = ref("");
  const recentTaskStatus = ref<Record<string, "download" | "update">>({});

  const hasMore = computed(() => marketSource.value === "skillsmp" ? onlineHasMore.value : offset.value + limit.value < total.value);
  const sortedResults = computed(() => results.value);
  const localSkillSourceSet = computed(() => {
    const set = new Set<string>();
    for (const skill of localSkills.value) {
      const sourceKey = skill.sourceUrl?.trim().toLowerCase();
      if (sourceKey) set.add(sourceKey);
    }
    return set;
  });

  const {
    ideOptions,
    selectedIdeFilter,
    customIdeName,
    customIdeDir,
    customIdeOptions,
    refreshIdeOptions,
    addCustomIde: doAddCustomIde,
    removeCustomIde,
    loadLastInstallTargets,
    saveLastInstallTargets
  } = useIdeConfig();

  function addCustomIde() {
    const success = doAddCustomIde(t, (msg: string) => {
      toast.error(msg);
    });
    if (success) {
      void scanLocalSkills();
    }
  }

  const filteredIdeSkills = computed(() =>
    ideSkills.value.filter((skill) => skill.ide === selectedIdeFilter.value)
  );
  async function buildInstallBaseDir(): Promise<string> {
    if (managerStorage.value?.skillsPath) return managerStorage.value.skillsPath;
    const home = await homeDir();
    return join(home, "Skill Manager/Skills");
  }

  async function loadManagerStorage() {
    try {
      managerStorage.value = await invoke<ManagerStorageInfo>("get_manager_storage_info");
    } catch (err) {
      toast.error(getErrorMessage(err, t("errors.storageFailed")));
    }
  }

  function sanitizeExportFileName(name: string): string {
    const sanitized = name.trim().replace(/[<>:"/\\|?*\x00-\x1F]/g, "-").replace(/\s+/g, "-");
    return sanitized || "skill";
  }

  function buildExportDefaultName(skills: LocalSkill[]): string {
    if (skills.length === 1) {
      return `${sanitizeExportFileName(skills[0].name)}.zip`;
    }
    const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
    return `skills-export-${timestamp}.zip`;
  }

  async function buildLinkTargets(targetLabel: string): Promise<LinkTarget[]> {
    const target = ideOptions.value.find((option) => option.label === targetLabel);
    if (!target) return [];

    const dir = target.globalDir;

    // Absolute path: use directly
    if (isSafeAbsolutePath(dir)) {
      return [{ name: target.label, path: dir }];
    }

    // Relative path: join with home directory
    if (!isSafeRelativePath(dir)) return [];

    const home = await homeDir();
    return [
      {
        name: target.label,
        path: await join(home, dir)
      }
    ];
  }

  async function searchMarketplace(reset = true, force = false) {
    if (loading.value) return;
    const source = marketSource.value;
    const keyword = query.value.trim();
    const cacheKey = `${source}|${keyword}|${limit.value}`;
    if (cacheKey !== lastSearchKey) reset = true;
    marketError.value = "";
    if (reset) {
      results.value = [];
      total.value = 0;
      offset.value = 0;
      onlineHasMore.value = false;
    }
    if (source === "skillsmp" && (!keyword || keyword.includes('*') || [...keyword].length > 200)) {
      marketError.value = "SkillsMP：请输入 1–200 个字符的关键词，不支持 * / Enter a keyword (1–200 characters), not a wildcard.";
      return;
    }
    loading.value = true;

    const nextOffset = reset ? 0 : offset.value + limit.value;

    if (reset && !force) {
      const cached = searchCache.get(cacheKey);
      if (cached && Date.now() - cached.timestamp < cacheTtlMs) {
        results.value = cached.data.skills;
        total.value = cached.data.total;
        offset.value = cached.data.offset;
        onlineHasMore.value = cached.data.hasNext ?? false;
        dailyRemaining.value = cached.data.dailyRemaining ?? null;
        lastSearchKey = cacheKey;
        loading.value = false;
        return;
      }
    }

    try {
      const data = await invoke<SearchResponse>(source === "skillsmp" ? "search_skillsmp" : "search_marketplaces", {
        query: keyword,
        limit: limit.value,
        offset: nextOffset
      });

      const deduped = dedupeSkills(reset ? data.skills : [...results.value, ...data.skills]);
      results.value = deduped;

      total.value = data.total;
      offset.value = data.offset;
      onlineHasMore.value = data.hasNext ?? false;
      dailyRemaining.value = data.dailyRemaining ?? null;
      lastSearchKey = cacheKey;

      if (reset) {
        if (searchCache.size >= 30) searchCache.delete(searchCache.keys().next().value!);
        searchCache.set(cacheKey, {
          timestamp: Date.now(),
          data
        });
      }
    } catch (err) {
      marketError.value = getErrorMessage(err, t("errors.searchFailed"));
      toast.error(marketError.value);
    } finally {
      loading.value = false;
    }
  }

  function dedupeSkills(skills: RemoteSkill[]) {
    const map = new Map<string, RemoteSkill>();
    for (const skill of skills) {
      const sourceKey = skill.sourceUrl?.trim().toLowerCase();
      const nameKey = `${skill.marketId}:${skill.name.trim().toLowerCase()}`;
      const key = sourceKey || nameKey;
      if (!map.has(key)) {
        map.set(key, skill);
      }
    }
    return Array.from(map.values());
  }

  function addToDownloadQueue(
    skill: RemoteSkill,
    action: "download" | "update" = "download",
    identity?: { skillUuid: string; targetPath: string }
  ) {
    // Check if already in queue
    if (downloadQueue.value.some(t => t.id === skill.id)) {
      return;
    }
    downloadQueue.value.push({
      id: skill.id,
      name: skill.name,
      sourceUrl: skill.sourceUrl,
      skillUuid: identity?.skillUuid,
      targetPath: identity?.targetPath,
      action,
      status: 'pending'
    });
    processQueue();
  }

  async function processQueue() {
    if (isProcessingQueue) return;
    isProcessingQueue = true;

    while (true) {
      const task = downloadQueue.value.find(t => t.status === 'pending');
      if (!task) break;

      task.status = 'downloading';
      try {
        const installBaseDir = await buildInstallBaseDir();
        const command = task.action === "update"
          ? "update_marketplace_skill"
          : "download_marketplace_skill";

        await invoke(command, {
          request: {
            sourceUrl: task.sourceUrl,
            skillName: task.name,
            installBaseDir,
            skillUuid: task.skillUuid,
            targetPath: task.targetPath
          }
        });
        task.status = 'done';
        recentTaskStatus.value = {
          ...recentTaskStatus.value,
          [task.id]: task.action
        };
        toast.success(
          task.action === "update"
            ? t("messages.updated", { path: task.name })
            : t("messages.downloaded", { path: task.name })
        );
        // Remove completed task after a short delay
        const timerId = window.setTimeout(() => {
          downloadQueue.value = downloadQueue.value.filter(t => t.id !== task.id);
          const nextStatus = { ...recentTaskStatus.value };
          delete nextStatus[task.id];
          recentTaskStatus.value = nextStatus;
          void scanLocalSkills(); // Properly handle async
          // Clean up timer to prevent memory leaks
          const index = timers.indexOf(timerId);
          if (index > -1) timers.splice(index, 1);
        }, 2500);
        timers.push(timerId);
      } catch (err) {
        task.status = 'error';
        task.error = err instanceof Error ? err.message : String(err);
      }
    }

    isProcessingQueue = false;
  }

  function removeFromQueue(taskId: string) {
    downloadQueue.value = downloadQueue.value.filter(t => t.id !== taskId);
  }

  function retryDownload(taskId: string) {
    const task = downloadQueue.value.find(t => t.id === taskId);
    if (task && task.status === 'error') {
      task.status = 'pending';
      task.error = undefined;
      processQueue();
    }
  }

  // Keep original downloadSkill for backward compatibility
  async function downloadSkill(skill: RemoteSkill) {
    addToDownloadQueue(skill, "download");
  }

  async function updateSkill(skill: RemoteSkill) {
    const sourceKey = skill.sourceUrl.trim().toLowerCase();
    const local = localSkills.value.find(
      (candidate) => candidate.sourceUrl?.trim().toLowerCase() === sourceKey
    );
    addToDownloadQueue(
      local ? { ...skill, name: local.name } : skill,
      "update",
      local ? { skillUuid: local.uuid, targetPath: local.path } : undefined
    );
  }

  function setMarketSource(source: "cached" | "skillsmp") {
    if (loading.value || source === marketSource.value) return;
    marketSource.value = source;
    results.value = [];
    total.value = 0;
    offset.value = 0;
    onlineHasMore.value = false;
    dailyRemaining.value = null;
    marketError.value = "";
    lastSearchKey = "";
    if (source === "cached" || query.value.trim()) void searchMarketplace(true);
  }

  async function updateLocalSkill(skill: LocalSkill) {
    const sourceUrl = skill.sourceUrl?.trim();
    if (!sourceUrl) {
      toast.error(t("errors.updateFailed"));
      return;
    }

    addToDownloadQueue(
      {
        id: `local:${skill.path}`,
        name: skill.name,
        namespace: "local",
        sourceUrl,
        description: skill.description,
        descriptionZh: "",
        author: "",
        installs: 0,
        stars: 0,
        marketId: "local",
        marketLabel: "Local"
      },
      "update",
      { skillUuid: skill.uuid, targetPath: skill.path }
    );
  }

  async function updateLocalSkills(skills: LocalSkill[]) {
    for (const skill of skills) {
      if (skill.sourceUrl?.trim()) {
        await updateLocalSkill(skill);
      }
    }
  }

  async function addManualSkill(sourceUrl: string, customName?: string) {
    const parsed = parseManualSkillSource(sourceUrl);
    if (!parsed) {
      toast.error(t("errors.unsupportedManualUrl"));
      return null;
    }

    const resolvedName = (customName?.trim() || parsed.inferredName || "").trim();
    if (!resolvedName) {
      toast.error(t("errors.manualSkillNameRequired"));
      return null;
    }

    const remoteSkill: RemoteSkill = {
      id: `manual:${parsed.normalizedUrl}`,
      name: resolvedName,
      namespace: "manual",
      sourceUrl: parsed.normalizedUrl,
      description: t("market.manualDescription"),
      descriptionZh: "",
      author: parsed.kind === "zip" ? t("market.manualSourceLabel") : "",
      installs: 0,
      stars: 0,
      marketId: "manual",
      marketLabel: t("market.manualSourceLabel")
    };

    if (localSkillSourceSet.value.has(parsed.normalizedUrl.toLowerCase())) {
      await updateSkill(remoteSkill);
      return "update" as const;
    }

    await downloadSkill(remoteSkill);
    return "download" as const;
  }

  async function scanLocalSkills() {
    if (localLoading.value) return;
    localLoading.value = true;

    try {
      const response = (await invoke("scan_overview", {
        request: {
          projectDir: null,
          ideDirs: ideOptions.value.map((item) => ({
            label: item.label,
            relativeDir: item.globalDir
          }))
        }
      })) as Overview;
      localSkills.value = response.managerSkills;
      ideSkills.value = response.ideSkills;
    } catch (err) {
      toast.error(getErrorMessage(err, t("errors.scanFailed")));
    } finally {
      localLoading.value = false;
    }
  }

  async function linkSkillInternal(skill: LocalSkill, ideLabel: string, skipScan = false, suppressToast = false) {
    const linkTargets = await buildLinkTargets(ideLabel);
    if (linkTargets.length === 0) {
      throw new Error(t("errors.selectValidIde"));
    }
    const result = (await invoke("link_local_skill", {
      request: {
        skillPath: skill.path,
        skillName: skill.name,
        linkTargets
      }
    })) as InstallResult;

    const linkedCount = result.linked.length;
    const skippedCount = result.skipped.length;
    if (!suppressToast) {
      toast.success(t("messages.handled", { linked: linkedCount, skipped: skippedCount }));
    }
    if (!skipScan) {
      await scanLocalSkills();
    }
    return result;
  }

  function openInstallModal(skill: LocalSkill | LocalSkill[]) {
    installTargetSkills.value = Array.isArray(skill) ? skill : [skill];
    const lastTargets = loadLastInstallTargets();
    const available = new Set(ideOptions.value.map((item) => item.label));
    const nextTargets = lastTargets.filter((label) => available.has(label));
    installTargetIde.value = nextTargets;
    showInstallModal.value = true;
  }

  function updateInstallTargetIde(next: string[]) {
    installTargetIde.value = next;
    saveLastInstallTargets(next);
  }

  async function confirmInstallToIde(installTarget: "ide" | "project", targetIds: string[], projects?: ProjectConfig[]) {
    if (installTarget === "project") {
      // Project installation
      if (!projects || projects.length === 0) {
        toast.error("No projects available");
        showInstallModal.value = false;
        installTargetSkills.value = [];
        return;
      }
      
      if (installTargetSkills.value.length === 0 || targetIds.length === 0) {
        toast.error(t("errors.selectAtLeastOne"));
        return;
      }
      if (installingId.value) return;
      installingId.value = installTargetSkills.value.length === 1 ? installTargetSkills.value[0].id : "__batch__";
      busy.value = true;
      busyText.value = t("messages.installing");

      try {
        let totalLinked = 0;
        let totalSkipped = 0;
        
        // Get selected projects
        const selectedProjects = projects.filter(p => targetIds.includes(p.id));
        
        // Install to project directories
        for (const skill of installTargetSkills.value) {
          for (const project of selectedProjects) {
            for (const ideLabel of project.ideTargets) {
              const result = await linkSkillToProjectInternal(skill, project, ideLabel, true, true);
              totalLinked += result.linked.length;
              totalSkipped += result.skipped.length;
            }
          }
        }
        
        toast.success(t("messages.handled", { linked: totalLinked, skipped: totalSkipped }));
        await scanLocalSkills();
        showInstallModal.value = false;
        installTargetSkills.value = [];
      } catch (err) {
        toast.error(getErrorMessage(err, t("errors.installFailed")));
      } finally {
        installingId.value = null;
        busy.value = false;
        busyText.value = "";
      }
      return;
    }
    
    // IDE installation (existing logic)
    if (installTargetSkills.value.length === 0 || targetIds.length === 0) {
      toast.error(t("errors.selectAtLeastOne"));
      return;
    }
    if (installingId.value) return;
    installingId.value = installTargetSkills.value.length === 1 ? installTargetSkills.value[0].id : "__batch__";
    busy.value = true;
    busyText.value = t("messages.installing");

    try {
      let totalLinked = 0;
      let totalSkipped = 0;
      
      // Install to global IDE directories
      for (const skill of installTargetSkills.value) {
        for (const label of targetIds) {
          const result = await linkSkillInternal(skill, label, true, true);
          totalLinked += result.linked.length;
          totalSkipped += result.skipped.length;
        }
      }
      
      toast.success(t("messages.handled", { linked: totalLinked, skipped: totalSkipped }));
      await scanLocalSkills();
      showInstallModal.value = false;
      installTargetSkills.value = [];
    } catch (err) {
      toast.error(getErrorMessage(err, t("errors.installFailed")));
    } finally {
      installingId.value = null;
      busy.value = false;
      busyText.value = "";
    }
  }

  async function linkSkillToProjectInternal(
    skill: LocalSkill,
    project: ProjectConfig,
    ideLabel: string,
    skipScan = false,
    suppressToast = false
  ) {
    const linkTargets = buildProjectLinkTargets(project, ideLabel);
    if (linkTargets.length === 0) {
      throw new Error(`${t("errors.selectValidIde")} (${project.name}: ${ideLabel})`);
    }
    const result = (await invoke("link_local_skill", {
      request: {
        skillPath: skill.path,
        skillName: skill.name,
        linkTargets,
        projectDir: project.path
      }
    })) as InstallResult;

    const linkedCount = result.linked.length;
    const skippedCount = result.skipped.length;
    if (!suppressToast) {
      toast.success(t("messages.handled", { linked: linkedCount, skipped: skippedCount }));
    }
    if (!skipScan) {
      await scanLocalSkills();
    }
    return result;
  }

  function closeInstallModal() {
    showInstallModal.value = false;
    installTargetSkills.value = [];
  }

  function openUninstallModal(targetPath: string) {
    uninstallMode.value = "ide";
    uninstallTargetPath.value = targetPath;
    uninstallTargetPaths.value = [targetPath];
    uninstallTargetName.value = targetPath.split(/[\\/]/).pop() || targetPath;
    showUninstallModal.value = true;
  }

  function openUninstallManyModal(paths: string[]) {
    if (paths.length === 0) return;
    uninstallMode.value = "ide";
    uninstallTargetPath.value = "";
    uninstallTargetPaths.value = paths;
    uninstallTargetName.value = t("ide.uninstallSelectedCount", { count: paths.length });
    showUninstallModal.value = true;
  }

  function openDeleteLocalModal(targets: LocalSkill[]) {
    uninstallMode.value = "local";
    uninstallTargetPath.value = "";
    uninstallTargetPaths.value = targets.map((skill) => skill.path);
    uninstallTargetName.value =
      targets.length === 1 ? targets[0].name : t("local.deleteSelectedCount", { count: targets.length });
    showUninstallModal.value = true;
  }

  async function confirmUninstall() {
    busy.value = true;
    busyText.value = uninstallMode.value === "local" ? t("messages.deleting") : t("messages.uninstalling");
    try {
      if (uninstallMode.value === "local") {
        const message = ((await invoke("delete_local_skills", {
          request: {
            targetPaths: uninstallTargetPaths.value
          }
        })) as string);
        toast.success(message);
      } else {
        // IDE mode: uninstall each path
        let successCount = 0;
        let failCount = 0;
        for (const targetPath of uninstallTargetPaths.value) {
          try {
            await invoke("uninstall_skill", {
              request: {
                targetPath,
                projectDir: null,
                ideDirs: ideOptions.value.map((item) => ({
                  label: item.label,
                  relativeDir: item.globalDir
                }))
              }
            });
            successCount++;
          } catch {
            failCount++;
          }
        }
        if (successCount > 0 && failCount === 0) {
          toast.success(t("messages.uninstalledCount", { count: successCount }));
        } else if (successCount > 0 && failCount > 0) {
          toast.success(t("messages.uninstalledPartial", { success: successCount, failed: failCount }));
        } else {
          toast.error(t("errors.uninstallFailed"));
        }
      }
      await scanLocalSkills();
    } catch (err) {
      toast.error(
        getErrorMessage(
          err,
          uninstallMode.value === "local" ? t("errors.deleteFailed") : t("errors.uninstallFailed")
        )
      );
    } finally {
      showUninstallModal.value = false;
      uninstallTargetPath.value = "";
      uninstallTargetName.value = "";
      uninstallTargetPaths.value = [];
      busy.value = false;
      busyText.value = "";
    }
  }

  function cancelUninstall() {
    showUninstallModal.value = false;
    uninstallTargetPath.value = "";
    uninstallTargetName.value = "";
    uninstallTargetPaths.value = [];
  }

  async function discoverSkillsInDirectory() {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        directory: true,
        multiple: false,
        title: t("local.selectDiscoveryDir")
      });
      if (!selected || Array.isArray(selected)) return;

      discoveryLoading.value = true;
      discoveryRoot.value = selected;
      discoveredSkills.value = [];
      discoveryImportResults.value = {};
      discoveredSkills.value = await invoke<DiscoveredSkill[]>("discover_skills_in_directory", {
        request: { rootPath: selected }
      });
    } catch (err) {
      toast.error(getErrorMessage(err, t("errors.discoveryFailed")));
    } finally {
      discoveryLoading.value = false;
    }
  }

  function clearDiscoveredSkills() {
    discoveredSkills.value = [];
    discoveryRoot.value = "";
    discoveryImportResults.value = {};
  }

  async function importDiscoveredSkills(skills: DiscoveredSkill[]) {
    if (skills.length === 0 || discoveryImporting.value) return;
    discoveryImporting.value = true;
    try {
      const result = await invoke<BatchImportResult>("import_discovered_skills", {
        request: { sourcePaths: skills.map((skill) => skill.path) }
      });
      const nextResults = { ...discoveryImportResults.value };
      for (const item of result.items) {
        nextResults[item.sourcePath] = item;
      }
      discoveryImportResults.value = nextResults;
      toast.success(
        t("messages.batchImported", {
          imported: result.imported,
          skipped: result.skipped,
          failed: result.failed
        })
      );
      await scanLocalSkills();
    } catch (err) {
      toast.error(getErrorMessage(err, t("errors.importFailed")));
    } finally {
      discoveryImporting.value = false;
    }
  }

  async function exportLocalSkills(skills: LocalSkill[]) {
    if (skills.length === 0) return;

    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const defaultPath = buildExportDefaultName(skills);
      const exportPath = await save({
        title: t("local.selectExportPath"),
        defaultPath,
        filters: [{ name: "ZIP Archive", extensions: ["zip"] }]
      });

      if (!exportPath) return;

      busy.value = true;
      busyText.value = t("messages.exporting");

      const normalizedExportPath = exportPath.toLowerCase().endsWith(".zip")
        ? exportPath
        : `${exportPath}.zip`;

      const result = (await invoke("export_local_skills", {
        request: {
          targetPaths: skills.map((skill) => skill.path),
          exportPath: normalizedExportPath
        }
      })) as string;

      toast.success(t("messages.exported", { path: result }));
    } catch (err) {
      toast.error(getErrorMessage(err, t("errors.exportFailed")));
    } finally {
      busy.value = false;
      busyText.value = "";
    }
  }

  async function openSkillDirectory(path: string) {
    try {
      await revealItemInDir(path);
    } catch (err) {
      const message = getErrorMessage(err, t("errors.openDirFailed"));
      if (message.includes("os error 2") || message.toLowerCase().includes("cannot find the file")) {
        try {
          await revealItemInDir(await dirname(path));
          toast.error(t("errors.openDirFailed") + ": " + path);
          return;
        } catch {
          // Fall through to the original error below.
        }
      }
      toast.error(message);
    }
  }

  async function adoptIdeSkill(skill: IdeSkill) {
    busy.value = true;
    busyText.value = t("messages.adopting");
    try {
      const message = (await invoke("adopt_ide_skill", {
        request: {
          targetPath: skill.path,
          ideLabel: skill.ide
        }
      })) as string;
      toast.success(message);
      await scanLocalSkills();
    } catch (err) {
      toast.error(getErrorMessage(err, t("errors.adoptFailed")));
    } finally {
      busy.value = false;
      busyText.value = "";
    }
  }

  async function adoptManyIdeSkills(skills: IdeSkill[]) {
    if (skills.length === 0) return;
    busy.value = true;
    busyText.value = t("messages.adopting");
    let successCount = 0;
    let failCount = 0;
    try {
      for (const skill of skills) {
        try {
          await invoke("adopt_ide_skill", {
            request: {
              targetPath: skill.path,
              ideLabel: skill.ide
            }
          });
          successCount++;
        } catch {
          failCount++;
        }
      }
      if (successCount > 0 && failCount === 0) {
        toast.success(t("messages.adoptedCount", { count: successCount }));
      } else if (successCount > 0 && failCount > 0) {
        toast.success(t("messages.adoptedPartial", { success: successCount, failed: failCount }));
      } else {
        toast.error(t("errors.adoptFailed"));
      }
      await scanLocalSkills();
    } finally {
      busy.value = false;
      busyText.value = "";
    }
  }

  onMounted(() => {
    refreshIdeOptions();
    void loadManagerStorage();
    void searchMarketplace(true);
    void scanLocalSkills();
  });

  return {
    // State
    activeTab,
    query,
    results,
    total,
    limit,
    offset,
    loading,
    installingId,
    updatingId,
    localSkills,
    ideSkills,
    localLoading,
    discoveredSkills,
    discoveryRoot,
    discoveryLoading,
    discoveryImporting,
    discoveryImportResults,
    managerStorage,
    ideOptions,
    selectedIdeFilter,
    customIdeName,
    customIdeDir,
    showInstallModal,
    installTargetIde,
    showUninstallModal,
    uninstallTargetName,
    busy,
    busyText,
    hasMore,
    sortedResults,
    localSkillSourceSet,
    filteredIdeSkills,
    customIdeOptions,
    downloadQueue,
    uninstallMode,
    recentTaskStatus,

    // Actions
    refreshIdeOptions,
    addCustomIde,
    removeCustomIde,
    searchMarketplace,
    marketSource,
    marketError,
    dailyRemaining,
    setMarketSource,
    downloadSkill,
    updateSkill,
    updateLocalSkill,
    updateLocalSkills,
    addManualSkill,
    scanLocalSkills,
    openInstallModal,
    updateInstallTargetIde,
    confirmInstallToIde,
    closeInstallModal,
    openUninstallModal,
    openUninstallManyModal,
    openDeleteLocalModal,
    confirmUninstall,
    cancelUninstall,
    discoverSkillsInDirectory,
    clearDiscoveredSkills,
    importDiscoveredSkills,
    loadManagerStorage,
    exportLocalSkills,
    openSkillDirectory,
    adoptIdeSkill,
    adoptManyIdeSkills,
    addToDownloadQueue,
    removeFromQueue,
    retryDownload
  };
}
