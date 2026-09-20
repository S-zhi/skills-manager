<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import type {
  DiscoveredSkill,
  ManagerStorageInfo,
  SkillImportItemResult
} from "../composables/types";

const { t } = useI18n();

const props = defineProps<{
  skills: DiscoveredSkill[];
  rootPath: string;
  loading: boolean;
  importing: boolean;
  importResults: Record<string, SkillImportItemResult>;
  storage: ManagerStorageInfo | null;
}>();

defineEmits<{
  (e: "discover"): void;
  (e: "clear"): void;
  (e: "import", skills: DiscoveredSkill[]): void;
  (e: "openDir", path: string): void;
}>();

const selectedPaths = ref<string[]>([]);
const searchQuery = ref("");
const standardFilter = ref<"all" | "standard" | "compatible">("all");

const filteredSkills = computed(() => {
  const keyword = searchQuery.value.trim().toLowerCase();
  return props.skills.filter((skill) => {
    if (standardFilter.value === "standard" && !skill.isStandard) return false;
    if (standardFilter.value === "compatible" && skill.isStandard) return false;
    if (!keyword) return true;
    return [skill.name, skill.description, skill.path, skill.provider]
      .some((value) => value.toLowerCase().includes(keyword));
  });
});

const selectedSkills = computed(() =>
  props.skills.filter((skill) => selectedPaths.value.includes(skill.path))
);

const allFilteredSelected = computed(
  () => filteredSkills.value.length > 0
    && filteredSkills.value.every((skill) => selectedPaths.value.includes(skill.path))
);

watch(
  () => props.skills,
  (skills) => {
    const available = new Set(skills.map((skill) => skill.path));
    selectedPaths.value = selectedPaths.value.filter((path) => available.has(path));
  },
  { deep: true }
);

function toggleSelected(path: string, checked: boolean) {
  selectedPaths.value = checked
    ? Array.from(new Set([...selectedPaths.value, path]))
    : selectedPaths.value.filter((value) => value !== path);
}

function toggleAllFiltered(checked: boolean) {
  const visiblePaths = filteredSkills.value.map((skill) => skill.path);
  if (checked) {
    selectedPaths.value = Array.from(new Set([...selectedPaths.value, ...visiblePaths]));
    return;
  }
  selectedPaths.value = selectedPaths.value.filter((path) => !visiblePaths.includes(path));
}

function clearSelection() {
  selectedPaths.value = [];
}

function issueLabel(issue: string) {
  const labels: Record<string, string> = {
    missing_frontmatter: t("local.discoveryIssues.missingFrontmatter"),
    missing_name: t("local.discoveryIssues.missingName"),
    invalid_name: t("local.discoveryIssues.invalidName"),
    missing_description: t("local.discoveryIssues.missingDescription"),
    description_too_long: t("local.discoveryIssues.descriptionTooLong"),
    directory_name_mismatch: t("local.discoveryIssues.directoryNameMismatch")
  };
  return labels[issue] ?? issue;
}

function importStatusLabel(result: SkillImportItemResult) {
  return t(`discovery.importStatus.${result.status}`);
}
</script>

<template>
  <section class="panel">
    <div class="panel-title">{{ t("discovery.title") }}</div>
    <div class="hint">{{ t("discovery.hint") }}</div>

    <div class="storage-card">
      <div>
        <div class="storage-label">{{ t("discovery.storageTitle") }}</div>
        <div class="card-link">{{ storage?.skillsPath ?? t("discovery.storageLoading") }}</div>
      </div>
      <button
        v-if="storage?.rootPath"
        class="ghost"
        type="button"
        @click="$emit('openDir', storage.rootPath)"
      >
        {{ t("discovery.openStorage") }}
      </button>
    </div>

    <div class="actions discovery-actions">
      <button class="primary" type="button" :disabled="loading || importing" @click="$emit('discover')">
        {{ loading ? t("local.discovering") : t("local.discover") }}
      </button>
      <button
        class="ghost"
        type="button"
        :disabled="selectedSkills.length === 0 || importing"
        @click="$emit('import', selectedSkills)"
      >
        {{ importing
          ? t("discovery.importing")
          : t("discovery.importSelected", { count: selectedSkills.length }) }}
      </button>
      <button
        v-if="rootPath"
        class="ghost"
        type="button"
        :disabled="loading || importing"
        @click="$emit('clear')"
      >
        {{ t("local.clearDiscovery") }}
      </button>
    </div>

    <div v-if="rootPath" class="discovery-summary">
      <div>
        <div class="summary-title">{{ t("local.discoveryTitle", { count: skills.length }) }}</div>
        <div class="card-link">{{ rootPath }}</div>
      </div>
      <div class="selection-actions">
        <span class="selection-count">
          {{ t("discovery.selectedCount", { count: selectedSkills.length }) }}
        </span>
        <button
          class="ghost"
          type="button"
          :disabled="filteredSkills.length === 0 || importing || allFilteredSelected"
          @click="toggleAllFiltered(true)"
        >
          {{ t("discovery.selectVisible") }}
        </button>
        <button
          class="ghost"
          type="button"
          :disabled="selectedSkills.length === 0 || importing"
          @click="clearSelection"
        >
          {{ t("discovery.clearSelection") }}
        </button>
      </div>
    </div>

    <div v-if="rootPath" class="filters">
      <input v-model="searchQuery" class="input" :placeholder="t('discovery.searchPlaceholder')" />
      <select v-model="standardFilter" class="input filter-select">
        <option value="all">{{ t("discovery.filterAll") }}</option>
        <option value="standard">{{ t("local.standard") }}</option>
        <option value="compatible">{{ t("local.nonStandard") }}</option>
      </select>
    </div>

    <div v-if="loading" class="hint">{{ t("local.discovering") }}</div>
    <div v-else-if="rootPath && skills.length === 0" class="hint">{{ t("local.discoveryEmpty") }}</div>
    <div v-else-if="rootPath && filteredSkills.length === 0" class="hint">
      {{ t("discovery.filteredEmpty") }}
    </div>

    <div v-if="filteredSkills.length > 0" class="cards discovery-cards">
      <article v-for="skill in filteredSkills" :key="skill.id" class="card discovery-card">
        <div class="card-header">
          <div class="skill-heading">
            <label class="checkbox card-select">
              <input
                type="checkbox"
                :checked="selectedPaths.includes(skill.path)"
                :disabled="importing"
                @change="toggleSelected(skill.path, ($event.target as HTMLInputElement).checked)"
              />
            </label>
            <div>
              <div class="card-title">{{ skill.name }}</div>
              <div class="badges">
                <span class="ide-badge active">{{ skill.provider }}</span>
                <span class="status-badge" :class="{ valid: skill.isStandard }">
                  {{ skill.isStandard ? t("local.standard") : t("local.nonStandard") }}
                </span>
                <span
                  v-if="importResults[skill.path]"
                  class="import-badge"
                  :class="importResults[skill.path].status"
                >
                  {{ importStatusLabel(importResults[skill.path]) }}
                </span>
              </div>
              <div class="skill-uuid">
                {{ skill.uuid
                  ? `${t("local.uuidLabel")}: ${skill.uuid}`
                  : t("discovery.uuidPending") }}
              </div>
            </div>
          </div>
          <button class="ghost" type="button" @click="$emit('openDir', skill.path)">
            {{ t("local.openDir") }}
          </button>
        </div>
        <p class="card-desc">{{ skill.description || t("local.previewEmptyDescription") }}</p>
        <div class="card-link">{{ skill.skillMdPath }}</div>
        <div v-if="importResults[skill.path]" class="import-result">
          {{ importResults[skill.path].message }}
          <button
            v-if="importResults[skill.path].targetPath"
            class="link-button"
            type="button"
            @click="$emit('openDir', importResults[skill.path].targetPath!)"
          >
            {{ t("discovery.openImported") }}
          </button>
        </div>
        <ul v-if="skill.issues.length > 0" class="issue-list">
          <li v-for="issue in skill.issues" :key="issue">{{ issueLabel(issue) }}</li>
        </ul>
      </article>
    </div>

    <div class="hint">{{ t("local.discoveryReadOnlyHint") }}</div>
  </section>
</template>

<style scoped>
.storage-card,
.discovery-summary,
.filters,
.discovery-actions,
.skill-heading,
.badges {
  display: flex;
  align-items: center;
  gap: 12px;
}

.skill-uuid {
  margin-top: 6px;
  color: var(--color-muted);
  font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  font-size: 11px;
  overflow-wrap: anywhere;
}

.selection-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  flex-wrap: wrap;
  gap: 8px;
}

.selection-count {
  color: var(--color-muted);
  font-size: 13px;
}

.storage-card,
.discovery-summary {
  justify-content: space-between;
  margin-top: 14px;
  padding: 12px 14px;
  border: 1px solid var(--color-panel-border);
  border-radius: 12px;
  background: var(--color-card-bg);
}

.storage-label,
.summary-title {
  font-size: 13px;
  font-weight: 700;
}

.discovery-actions {
  flex-wrap: wrap;
  margin-top: 14px;
}

.filters {
  margin-top: 12px;
}

.filters .input:first-child {
  flex: 1;
}

.filter-select {
  width: 220px;
}

.discovery-cards {
  max-height: 560px;
  overflow: auto;
  padding-right: 4px;
}

.discovery-card {
  background: var(--color-card-bg);
}

.skill-heading {
  align-items: flex-start;
}

.badges {
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 7px;
}

.ide-badge,
.status-badge,
.import-badge {
  padding: 4px 8px;
  border-radius: 999px;
  border: 1px solid var(--color-chip-border);
  font-size: 11px;
  line-height: 1.2;
}

.ide-badge.active,
.status-badge.valid,
.import-badge.imported {
  border-color: var(--color-success-border);
  background: var(--color-success-bg);
  color: var(--color-success-text);
}

.status-badge,
.import-badge.failed {
  border-color: var(--color-error-border);
  background: var(--color-error-bg);
  color: var(--color-error-text);
}

.import-badge.skipped {
  background: var(--color-chip-bg);
  color: var(--color-meta);
}

.issue-list {
  margin: 10px 0 0;
  padding-left: 22px;
  color: var(--color-muted);
  font-size: 12px;
}

.import-result {
  margin-top: 10px;
  color: var(--color-meta);
  font-size: 12px;
}

.link-button {
  margin-left: 8px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--color-text);
  text-decoration: underline;
  cursor: pointer;
}

@media (max-width: 720px) {
  .storage-card,
  .discovery-summary,
  .filters {
    align-items: stretch;
    flex-direction: column;
  }

  .selection-actions {
    justify-content: flex-start;
  }

  .filter-select {
    width: 100%;
  }
}
</style>
