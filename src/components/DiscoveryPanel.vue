<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import type {
  DiscoveredSkill,
  SkillImportItemResult
} from "../composables/types";

const { t } = useI18n();

const props = defineProps<{
  skills: DiscoveredSkill[];
  rootPath: string;
  loading: boolean;
  importing: boolean;
  importResults: Record<string, SkillImportItemResult>;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "discover"): void;
  (e: "clear"): void;
  (e: "import", skills: DiscoveredSkill[]): void;
  (e: "openDir", path: string): void;
}>();

const selectedPaths = ref<string[]>([]);
const dialog = ref<HTMLElement | null>(null);
const searchQuery = ref("");
const standardFilter = ref<"all" | "standard" | "compatible">("all");
const previouslyFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null;

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
  props.skills.filter((skill) => !skill.isDuplicate && selectedPaths.value.includes(skill.path))
);

const selectableFilteredSkills = computed(() =>
  filteredSkills.value.filter((skill) => !skill.isDuplicate)
);

const allFilteredSelected = computed(
  () => selectableFilteredSkills.value.length > 0
    && selectableFilteredSkills.value.every((skill) => selectedPaths.value.includes(skill.path))
);

const duplicateCount = computed(() => props.skills.filter((skill) => skill.isDuplicate).length);

watch(
  () => props.skills,
  (skills) => {
    selectedPaths.value = skills.filter((skill) => !skill.isDuplicate).map((skill) => skill.path);
  },
  { deep: true, immediate: true }
);

const importModeLabel = computed(() => {
  if (props.skills.length === 1) return t("discovery.singleMode");
  if (props.skills.length > 1) return t("discovery.batchMode", { count: props.skills.length });
  return "";
});

function toggleSelected(path: string, checked: boolean) {
  if (props.skills.find((skill) => skill.path === path)?.isDuplicate) return;
  selectedPaths.value = checked
    ? Array.from(new Set([...selectedPaths.value, path]))
    : selectedPaths.value.filter((value) => value !== path);
}

function toggleAllFiltered(checked: boolean) {
  const visiblePaths = selectableFilteredSkills.value.map((skill) => skill.path);
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

function close() {
  emit("close");
}

function handleKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    close();
    return;
  }
  if (event.key !== "Tab" || !dialog.value) return;
  const focusable = Array.from(dialog.value.querySelectorAll<HTMLElement>(
    'button:not([disabled]), input:not([disabled]), select:not([disabled]), summary, [tabindex]:not([tabindex="-1"])'
  ));
  if (!focusable.length) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}

onMounted(async () => {
  document.addEventListener("keydown", handleKeydown);
  await nextTick();
  dialog.value?.querySelector<HTMLElement>(".modal-close")?.focus();
});

onBeforeUnmount(() => {
  document.removeEventListener("keydown", handleKeydown);
  previouslyFocused?.focus();
});
</script>

<template>
  <Teleport to="body">
    <div class="import-skill-backdrop">
      <section
        ref="dialog"
        class="import-skill-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-skill-title"
      >
        <header class="modal-header">
          <h2 id="import-skill-title" class="panel-title">{{ t("local.import") }}</h2>
          <button class="modal-close" type="button" :aria-label="t('discovery.close')" @click="close">×</button>
        </header>

        <div class="modal-body">
          <div class="import-toolbar">
            <button class="primary" type="button" :disabled="loading || importing" @click="$emit('discover')">
              {{ loading ? t("local.discovering") : t("discovery.chooseFolder") }}
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
            <div class="summary-content">
              <div class="summary-title">{{ t("local.discoveryTitle", { count: skills.length }) }}</div>
              <div class="card-link">{{ rootPath }}</div>
              <div class="summary-badges">
                <span v-if="importModeLabel" class="mode-label">{{ importModeLabel }}</span>
                <span v-if="duplicateCount" class="duplicate-count">
                  {{ t("discovery.duplicateCount", { count: duplicateCount }) }}
                </span>
              </div>
            </div>
            <button
              class="primary"
              type="button"
              :disabled="selectedSkills.length === 0 || importing"
              @click="$emit('import', selectedSkills)"
            >
              {{ importing
                ? t("discovery.importing")
                : t("discovery.importSelected", { count: selectedSkills.length }) }}
            </button>
          </div>

          <div v-if="rootPath" class="filters">
            <input v-model="searchQuery" class="input" :placeholder="t('discovery.searchPlaceholder')" />
            <select v-model="standardFilter" class="input filter-select">
              <option value="all">{{ t("discovery.filterAll") }}</option>
              <option value="standard">{{ t("local.standard") }}</option>
              <option value="compatible">{{ t("local.nonStandard") }}</option>
            </select>
          </div>

          <div v-if="rootPath" class="selection-actions">
            <span class="selection-count">{{ t("discovery.selectedCount", { count: selectedSkills.length }) }}</span>
            <label class="checkbox select-results">
              <input
                type="checkbox"
                :checked="allFilteredSelected"
                :disabled="selectableFilteredSkills.length === 0 || importing"
                @change="toggleAllFiltered(($event.target as HTMLInputElement).checked)"
              />
              {{ t("discovery.selectVisible") }}
            </label>
            <button class="ghost" type="button" :disabled="selectedSkills.length === 0 || importing" @click="clearSelection">
              {{ t("discovery.clearSelection") }}
            </button>
          </div>

          <div v-if="loading" class="hint result-message">{{ t("local.discovering") }}</div>
          <div v-else-if="rootPath && skills.length === 0" class="hint result-message">{{ t("local.discoveryEmpty") }}</div>
          <div v-else-if="rootPath && filteredSkills.length === 0" class="hint result-message">
            {{ t("discovery.filteredEmpty") }}
          </div>

          <div v-if="filteredSkills.length > 0" class="cards discovery-cards">
            <article
              v-for="skill in filteredSkills"
              :key="skill.id"
              class="card discovery-card"
              :class="{ duplicate: skill.isDuplicate }"
              :aria-disabled="skill.isDuplicate"
            >
              <div class="card-header">
                <div class="skill-heading">
                  <label class="checkbox card-select">
                    <input
                      type="checkbox"
                      :checked="selectedPaths.includes(skill.path)"
                      :disabled="importing || skill.isDuplicate"
                      @change="toggleSelected(skill.path, ($event.target as HTMLInputElement).checked)"
                    />
                  </label>
                  <div>
                    <div class="card-title">{{ skill.name }}</div>
                    <div class="badges">
                      <span v-if="skill.isDuplicate" class="duplicate-badge">{{ t("discovery.duplicate") }}</span>
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
                      {{ skill.uuid ? `${t("local.uuidLabel")}: ${skill.uuid}` : t("discovery.uuidPending") }}
                    </div>
                  </div>
                </div>
                <button class="ghost" type="button" @click="$emit('openDir', skill.path)">{{ t("local.openDir") }}</button>
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
        </div>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.discovery-summary,
.filters,
.import-toolbar,
.skill-heading,
.badges,
.summary-badges {
  display: flex;
  align-items: center;
  gap: 12px;
}

.import-skill-backdrop { position: fixed; inset: 0; z-index: 1400; display: grid; place-items: center; padding: 28px; background: var(--color-overlay); }
.import-skill-dialog { width: min(860px, 100%); max-height: min(84vh, 850px); overflow: hidden; border: 1px solid var(--color-modal-border); border-radius: 18px; background: var(--color-modal-bg); box-shadow: 0 28px 80px #0005; }
.modal-header { display: flex; align-items: center; justify-content: space-between; gap: 20px; padding: 22px 24px 18px; border-bottom: 1px solid var(--color-panel-border); }
.modal-header .panel-title { margin: 0; }
.modal-close { display: grid; place-items: center; flex: 0 0 36px; width: 36px; height: 36px; padding: 0; border: 0; border-radius: 9px; background: transparent; color: var(--color-muted); font-size: 24px; line-height: 1; cursor: pointer; }
.modal-close:hover { background: var(--color-tabs-bg); color: var(--color-text); }
.modal-body { max-height: calc(min(84vh, 850px) - 78px); overflow-y: auto; padding: 22px 24px 24px; }
.import-toolbar { flex-wrap: wrap; }

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

.mode-label {
  padding: 4px 8px;
  border: 1px solid var(--color-accent-border);
  border-radius: 999px;
  background: var(--color-accent-soft);
  color: var(--color-accent);
  font-size: 11px;
}

.duplicate-count,
.duplicate-badge {
  padding: 4px 8px;
  border: 1px solid var(--color-chip-border);
  border-radius: 999px;
  background: var(--color-chip-bg);
  color: var(--color-muted);
  font-size: 11px;
}

.select-results {
  white-space: nowrap;
}

.selection-count {
  color: var(--color-muted);
  font-size: 13px;
}

.discovery-summary {
  justify-content: space-between;
  margin-top: 16px;
  padding: 12px 14px;
  border: 1px solid var(--color-panel-border);
  border-radius: 12px;
  background: var(--color-card-bg);
}

.summary-title {
  font-size: 13px;
  font-weight: 700;
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
  margin-top: 12px;
}

.discovery-card {
  background: var(--color-card-bg);
}

.discovery-card.duplicate {
  opacity: .5;
  filter: grayscale(.35);
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
.import-badge,
.duplicate-badge {
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

.result-message { margin-top: 18px; }

@media (max-width: 720px) {
  .import-skill-backdrop { align-items: stretch; padding: 12px; }
  .import-skill-dialog { max-height: calc(100vh - 24px); border-radius: 14px; }
  .modal-header { padding: 18px; }
  .modal-body { max-height: calc(100vh - 94px); padding: 18px; }
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
