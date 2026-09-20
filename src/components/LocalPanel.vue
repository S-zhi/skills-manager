<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type {
  LocalSkill,
  LocalSkillPreview,
  DownloadTask,
  IdeOption,
  ManagerStorageInfo,
  SkillLibraryStore,
  SkillPackage,
  SkillPackageStore
} from "../composables/types";
import DownloadQueue from "./DownloadQueue.vue";
import SkillPreviewModal from "./SkillPreviewModal.vue";
import SkillHistory from "./SkillHistory.vue";
import SkillEditor from "./SkillEditor.vue";
import { useI18n } from "vue-i18n";
import { normalizeSkillName } from "../composables/utils";
import { useToast } from "../composables/useToast";

const { t, locale } = useI18n();
const historySkill = ref<LocalSkill | null>(null);
const editorSkill = ref<LocalSkill | null>(null);
const metadata = ref<SkillLibraryStore>({ schemaVersion: 1, revision: 0, entries: [] });
const metadataBusy = ref(false);
const favoriteOnly = ref(false);
const selectedTag = ref("");
const tagEditorSkill = ref<LocalSkill | null>(null);
const tagDraft = ref("");
const toast = useToast();

const props = defineProps<{
  localSkills: LocalSkill[];
  localLoading: boolean;
  installingId: string | null;
  downloadQueue: DownloadTask[];
  ideOptions: IdeOption[];
  managerStorage: ManagerStorageInfo | null;
}>();

const emit = defineEmits<{
  (e: "install", skill: LocalSkill): void;
  (e: "installMany", skills: LocalSkill[]): void;
  (e: "updateLocal", skill: LocalSkill): void;
  (e: "updateLocalMany", skills: LocalSkill[]): void;
  (e: "exportLocal", skills: LocalSkill[]): void;
  (e: "deleteLocal", skills: LocalSkill[]): void;
  (e: "openDir", path: string): void;
  (e: "create"): void;
  (e: "refresh"): void;
  (e: "import"): void;
  (e: "retryDownload", taskId: string): void;
  (e: "removeFromQueue", taskId: string): void;
}>();

const selectedIds = ref<string[]>([]);
const searchQuery = ref("");
const previewVisible = ref(false);
const previewLoading = ref(false);
const previewSkill = ref<LocalSkill | null>(null);
const previewData = ref<LocalSkillPreview | null>(null);
const translatedDescriptions = ref<Record<string, string>>({});
const UNCATEGORIZED_GROUP_ID = "__uncategorized__";
const packageStore = ref<SkillPackageStore>({ schemaVersion: 3, revision: 0, uncategorizedPosition: 0, packages: [] });
const packageBusy = ref(false);
const packageDialogOpen = ref(false);
const packageEditId = ref<string | null>(null);
const packageName = ref("");
const packageDescription = ref("");
const assignmentOpen = ref(false);
const assignmentSkillIds = ref<string[]>([]);
const assignmentPackageId = ref("");
const expandedPackageIds = ref<string[]>([UNCATEGORIZED_GROUP_ID]);
const draggedPackageId = ref<string | null>(null);
const dropTargetGroupId = ref<string | null>(null);
const dragPreviewOrder = ref<string[] | null>(null);

const filteredLocalSkills = computed(() => {
  const keyword = searchQuery.value.trim().toLowerCase();
  const candidates = props.localSkills.filter((skill) => {
    const entry = metadata.value.entries.find((item) => item.uuid === skill.uuid);
    if (favoriteOnly.value && !entry?.favorite) return false;
    if (selectedTag.value && !entry?.tags.includes(selectedTag.value)) return false;
    return true;
  });
  if (!keyword) return candidates;
  const normalizedKeyword = normalizeSkillName(keyword);
  return candidates.filter((skill) => {
    const haystacks = [skill.name, translatedDescriptions.value[skill.uuid] ?? skill.description, skill.path];
    return haystacks.some((value) => {
      const lowered = value.toLowerCase();
      return lowered.includes(keyword) || normalizeSkillName(value).includes(normalizedKeyword);
    });
  });
});

const availableTags = computed(() => Array.from(new Set(metadata.value.entries.flatMap((entry) => entry.tags))).sort((a, b) => a.localeCompare(b)));
const sortedPackages = computed(() => [...packageStore.value.packages].sort((a, b) => a.position - b.position));
const packagedSkillIds = computed(() => new Set(sortedPackages.value.flatMap((pkg) => pkg.skillUuids)));
const orderedSkillGroups = computed(() => {
  const packageNumbers = new Map(sortedPackages.value.map((pkg, index) => [pkg.id, index + 1]));
  return [
    {
      id: UNCATEGORIZED_GROUP_ID,
      name: t("packages.uncategorized"),
      description: "",
      position: packageStore.value.uncategorizedPosition,
      number: null as number | null,
      skills: filteredLocalSkills.value.filter((skill) => !packagedSkillIds.value.has(skill.uuid)),
      package: null as SkillPackage | null
    },
    ...sortedPackages.value.map((pkg) => ({
      id: pkg.id,
      name: pkg.name,
      description: pkg.description,
      position: pkg.position,
      number: packageNumbers.get(pkg.id) ?? null,
      skills: filteredLocalSkills.value.filter((skill) => pkg.skillUuids.includes(skill.uuid)),
      package: pkg
    }))
  ].sort((a, b) => a.position - b.position);
});
const skillGroups = computed(() => orderedSkillGroups.value);

async function loadPackages() {
  try {
    packageStore.value = await invoke<SkillPackageStore>("list_skill_packages");
    const available = new Set([UNCATEGORIZED_GROUP_ID, ...packageStore.value.packages.map((pkg) => pkg.id)]);
    expandedPackageIds.value = expandedPackageIds.value.filter((id) => available.has(id));
  } catch (reason) {
    toast.error(String(reason));
  }
}

function openPackageDialog(pkg?: SkillPackage) {
  packageEditId.value = pkg?.id ?? null;
  packageName.value = pkg?.name ?? "";
  packageDescription.value = pkg?.description ?? "";
  packageDialogOpen.value = true;
}

async function savePackage() {
  packageBusy.value = true;
  try {
    const existing = packageEditId.value
      ? packageStore.value.packages.find((pkg) => pkg.id === packageEditId.value)
      : undefined;
    packageStore.value = await invoke<SkillPackageStore>("save_skill_package", {
      request: {
        id: packageEditId.value,
        name: packageName.value,
        description: packageDescription.value,
        skillUuids: existing?.skillUuids ?? [],
        revision: packageStore.value.revision
      }
    });
    const saved = packageEditId.value
      ? packageStore.value.packages.find((pkg) => pkg.id === packageEditId.value)
      : packageStore.value.packages[packageStore.value.packages.length - 1];
    if (saved && !expandedPackageIds.value.includes(saved.id)) expandedPackageIds.value.push(saved.id);
    packageDialogOpen.value = false;
    toast.success(t("packages.saved"));
  } catch (reason) {
    toast.error(String(reason));
    await loadPackages();
  } finally {
    packageBusy.value = false;
  }
}

async function deletePackage(pkg: SkillPackage) {
  if (!window.confirm(t("packages.deleteConfirm"))) return;
  packageBusy.value = true;
  try {
    packageStore.value = await invoke<SkillPackageStore>("delete_skill_package", {
      id: pkg.id,
      revision: packageStore.value.revision
    });
    toast.success(t("packages.deleted"));
  } catch (reason) {
    toast.error(String(reason));
    await loadPackages();
  } finally {
    packageBusy.value = false;
  }
}

function openAssignment(skills: LocalSkill[]) {
  if (!skills.length) return;
  assignmentSkillIds.value = skills.map((skill) => skill.uuid);
  const current = sortedPackages.value.find((pkg) => pkg.skillUuids.includes(skills[0].uuid));
  assignmentPackageId.value = skills.every((skill) => current?.skillUuids.includes(skill.uuid))
    ? current?.id ?? ""
    : "";
  assignmentOpen.value = true;
}

async function assignPackage() {
  packageBusy.value = true;
  try {
    packageStore.value = await invoke<SkillPackageStore>("assign_skill_package_members", {
      request: {
        packageId: assignmentPackageId.value || null,
        skillUuids: assignmentSkillIds.value,
        revision: packageStore.value.revision
      }
    });
    assignmentOpen.value = false;
    toast.success(t("packages.assigned"));
  } catch (reason) {
    toast.error(String(reason));
    await loadPackages();
  } finally {
    packageBusy.value = false;
  }
}

function toggleGroup(id: string) {
  expandedPackageIds.value = expandedPackageIds.value.includes(id)
    ? expandedPackageIds.value.filter((item) => item !== id)
    : [...expandedPackageIds.value, id];
}

function startGroupDrag(event: DragEvent, groupId: string) {
  draggedPackageId.value = groupId;
  dragPreviewOrder.value = skillGroups.value.map((group) => group.id);
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", groupId);
  }
}

function groupVisualOrder(groupId: string) {
  const order = dragPreviewOrder.value?.indexOf(groupId) ?? -1;
  return order >= 0 ? order : undefined;
}

function previewGroupMove(targetId: string) {
  const sourceId = draggedPackageId.value;
  const ids = dragPreviewOrder.value;
  if (!sourceId || !ids || sourceId === targetId || dropTargetGroupId.value === targetId) return;
  const sourceIndex = ids.indexOf(sourceId);
  const targetIndex = ids.indexOf(targetId);
  if (sourceIndex < 0 || targetIndex < 0) return;
  const next = [...ids];
  next.splice(targetIndex, 0, next.splice(sourceIndex, 1)[0]);
  dragPreviewOrder.value = next;
  dropTargetGroupId.value = targetId;
}

function cancelGroupDrag() {
  draggedPackageId.value = null;
  dropTargetGroupId.value = null;
  dragPreviewOrder.value = null;
}

async function dropPackage() {
  const ids = dragPreviewOrder.value ? [...dragPreviewOrder.value] : [];
  cancelGroupDrag();
  if (!ids.length || ids.every((id, index) => orderedSkillGroups.value[index]?.id === id)) return;
  packageBusy.value = true;
  try {
    packageStore.value = await invoke<SkillPackageStore>("reorder_skill_packages", {
      request: { groupIds: ids, revision: packageStore.value.revision }
    });
  } catch (reason) {
    toast.error(String(reason));
    await loadPackages();
  } finally {
    packageBusy.value = false;
  }
}
function metadataEntry(skill: LocalSkill) {
  return metadata.value.entries.find((entry) => entry.uuid === skill.uuid) ?? { uuid: skill.uuid, favorite: false, tags: [] };
}
async function loadMetadata() {
  try { metadata.value = await invoke<SkillLibraryStore>("get_skill_library_metadata"); }
  catch (reason) { toast.error(String(reason)); }
}
async function saveMetadata(skill: LocalSkill, favorite: boolean, tags: string[]) {
  metadataBusy.value = true;
  try {
    metadata.value = await invoke<SkillLibraryStore>("save_skill_library_entry", { request: { uuid: skill.uuid, favorite, tags, revision: metadata.value.revision } });
  } catch (reason) {
    toast.error(String(reason));
    await loadMetadata();
  } finally { metadataBusy.value = false; }
}
function toggleFavorite(skill: LocalSkill) {
  const entry = metadataEntry(skill);
  void saveMetadata(skill, !entry.favorite, entry.tags);
}
function openTagEditor(skill: LocalSkill) {
  tagEditorSkill.value = skill;
  tagDraft.value = metadataEntry(skill).tags.join(", ");
}
async function saveTags() {
  if (!tagEditorSkill.value) return;
  const skill = tagEditorSkill.value;
  const entry = metadataEntry(skill);
  const tags = tagDraft.value.split(/[,，]/).map((tag) => tag.trim()).filter(Boolean);
  await saveMetadata(skill, entry.favorite, tags);
  tagEditorSkill.value = null;
}

onMounted(() => {
  void loadMetadata();
  void loadPackages();
});

watch(
  () => props.localSkills,
  (skills) => {
    const available = new Set(skills.map((skill) => skill.id));
    selectedIds.value = selectedIds.value.filter((id) => available.has(id));
  },
  { deep: true }
);

watch(searchQuery, (next, previous) => {
  if (!next.trim() && previous.trim()) emit("refresh");
});

function refreshOnEmptySearch() {
  if (!searchQuery.value.trim()) emit("refresh");
}

const selectedSkills = computed(() =>
  filteredLocalSkills.value.filter((skill) => selectedIds.value.includes(skill.id))
);
const selectedUpdatableSkills = computed(() =>
  selectedSkills.value.filter((skill) => !!skill.sourceUrl?.trim())
);

const allSelected = computed(
  () =>
    filteredLocalSkills.value.length > 0 &&
    filteredLocalSkills.value.every((skill) => selectedIds.value.includes(skill.id))
);

function toggleSelectAll(checked: boolean) {
  const filteredIds = filteredLocalSkills.value.map((skill) => skill.id);
  if (checked) {
    selectedIds.value = Array.from(new Set([...selectedIds.value, ...filteredIds]));
    return;
  }
  selectedIds.value = selectedIds.value.filter((id) => !filteredIds.includes(id));
}

function toggleSelected(skillId: string, checked: boolean) {
  selectedIds.value = checked
    ? [...selectedIds.value, skillId]
    : selectedIds.value.filter((id) => id !== skillId);
}

function buildIdeBadgeList(skill: LocalSkill) {
  return props.ideOptions.filter(option => skill.usedBy.includes(option.label)).map((option) => ({
    label: option.label,
    active: skill.usedBy.includes(option.label)
  }));
}

function installSelected() {
  if (selectedSkills.value.length === 0) return;
  emit("installMany", selectedSkills.value);
}

function exportSelected() {
  if (selectedSkills.value.length === 0) return;
  emit("exportLocal", selectedSkills.value);
}

function updateSelected() {
  if (selectedUpdatableSkills.value.length === 0) return;
  emit("updateLocalMany", selectedUpdatableSkills.value);
}

function deleteSelected() {
  if (selectedSkills.value.length === 0) return;
  emit("deleteLocal", selectedSkills.value);
}

async function openPreview(skill: LocalSkill) {
  const currentSkillPath = skill.path;
  previewVisible.value = true;
  previewLoading.value = true;
  previewSkill.value = skill;
  previewData.value = null;

  try {
    const result = await invoke<LocalSkillPreview>("read_local_skill_preview", {
      skillPath: currentSkillPath,
      targetLanguage: "zh-CN"
    });
    if (previewSkill.value?.path !== currentSkillPath) return;
    previewData.value = result;
    if ((result.translationStatus === "translated" || result.translationStatus === "cached") && result.displayDescription?.trim()) {
      translatedDescriptions.value = {
        ...translatedDescriptions.value,
        [skill.uuid]: result.displayDescription.trim()
      };
    }
  } catch {
    if (previewSkill.value?.path !== currentSkillPath) return;
    closePreview();
    toast.error(t("errors.previewFailed"));
  } finally {
    if (previewSkill.value?.path === currentSkillPath || previewSkill.value === null) {
      previewLoading.value = false;
    }
  }
}

function closePreview() {
  previewVisible.value = false;
  previewLoading.value = false;
  previewSkill.value = null;
  previewData.value = null;
}
</script>

<template>
  <section class="panel">
    <div class="panel-heading">
      <div>
        <div class="panel-title">{{ t("local.title") }}</div>
        <div class="hint">{{ t("local.hint") }}</div>
      </div>
      <div class="panel-heading-actions">
        <button class="ghost package-create" type="button" :disabled="packageBusy" @click="openPackageDialog()">
          {{ t("packages.create") }}
        </button>
        <button class="primary" type="button" @click="$emit('create')">
          {{ t("createSkill.title") }}
        </button>
        <button class="ghost" type="button" :disabled="localLoading" @click="$emit('import')">
          {{ t("local.import") }}
        </button>
      </div>
    </div>
    <details class="repository-details"><summary>{{ t('local.storageTitle') }}</summary><div class="storage-card">
      <div>
        <div class="storage-label">{{ t("local.storageTitle") }}</div>
        <div class="card-link">{{ managerStorage?.skillsPath ?? t("discovery.storageLoading") }}</div>
      </div>
      <button
        v-if="managerStorage?.rootPath"
        class="ghost"
        type="button"
        @click="$emit('openDir', managerStorage.rootPath)"
      >
        {{ t("discovery.openStorage") }}
      </button>
    </div></details>
    <div class="panel-summary">
      <span>{{ t("local.total", { count: localSkills.length }) }}</span>
      <label class="checkbox select-all">
        <input
          type="checkbox"
          :checked="allSelected"
          :disabled="filteredLocalSkills.length === 0"
          @change="toggleSelectAll(($event.target as HTMLInputElement).checked)"
        />
        {{ t("local.selectAll") }}
      </label>
    </div>
    <div class="search-row">
      <input
        v-model="searchQuery"
        type="search"
        class="input"
        :placeholder="t('local.searchPlaceholder')"
        @keyup.enter="refreshOnEmptySearch"
      />
      <div class="hint search-summary">
        {{ t("local.filteredTotal", { shown: filteredLocalSkills.length, total: localSkills.length }) }}
      </div>
      <label class="favorite-filter"><input v-model="favoriteOnly" type="checkbox" /> {{ locale === 'zh-CN' ? '仅看收藏' : 'Favorites' }}</label>
      <select v-model="selectedTag" class="input tag-filter" :aria-label="locale === 'zh-CN' ? '按标签筛选' : 'Filter by tag'">
        <option value="">{{ locale === 'zh-CN' ? '全部标签' : 'All tags' }}</option>
        <option v-for="tag in availableTags" :key="tag" :value="tag">{{ tag }}</option>
      </select>
    </div>
    <div v-if="selectedSkills.length" class="actions">
      <div class="buttons">
        <button v-if="selectedSkills.length" class="ghost" :disabled="localLoading" @click="installSelected">
          {{ t("local.installSelected", { count: selectedSkills.length }) }}
        </button>
        <button v-if="selectedSkills.length" class="ghost" :disabled="selectedUpdatableSkills.length === 0 || localLoading" @click="updateSelected">
          {{ t("local.updateSelected", { count: selectedUpdatableSkills.length }) }}
        </button>
        <button v-if="selectedSkills.length" class="ghost" :disabled="localLoading" @click="exportSelected">
          {{ t("local.exportSelected", { count: selectedSkills.length }) }}
        </button>
        <button v-if="selectedSkills.length" class="ghost package-assign" :disabled="packageBusy" @click="openAssignment(selectedSkills)">
          {{ t("packages.assignSelected", { count: selectedSkills.length }) }}
        </button>
        <button v-if="selectedSkills.length" class="ghost danger" :disabled="localLoading" @click="deleteSelected">
          {{ t("local.deleteSelected", { count: selectedSkills.length }) }}
        </button>
        <button
          v-if="selectedSkills.length === localSkills.length && localSkills.length > 0"
          class="ghost danger"
          :disabled="localSkills.length === 0 || localLoading"
          @click="$emit('deleteLocal', localSkills)"
        >
          {{ t("local.deleteAll") }}
        </button>
      </div>
    </div>

    <DownloadQueue
      :tasks="downloadQueue"
      @retry="$emit('retryDownload', $event)"
      @remove="$emit('removeFromQueue', $event)"
    />
    <SkillHistory v-if="historySkill" :key="historySkill.path" :skill="historySkill" @close="historySkill = null" @restored="$emit('refresh')" />

    <div v-if="localLoading" class="hint">{{ t("local.scanning") }}</div>
    <div v-if="!localLoading && localSkills.length === 0" class="hint">{{ t("local.emptyHint") }}</div>
    <div v-else-if="!localLoading && filteredLocalSkills.length === 0" class="hint">
      {{ t("local.searchEmptyHint") }}
    </div>
    <div v-if="filteredLocalSkills.length > 0" class="skill-groups">
      <section
        v-for="group in skillGroups"
        v-show="group.id === UNCATEGORIZED_GROUP_ID || group.skills.length > 0 || group.package"
        :key="group.id"
        class="skill-group"
        :style="{ order: groupVisualOrder(group.id) }"
        :class="{ dragging: draggedPackageId === group.id, 'drop-target': dropTargetGroupId === group.id && draggedPackageId !== group.id }"
        @dragover.prevent
        @dragenter.prevent="previewGroupMove(group.id)"
        @dragleave.self="dropTargetGroupId === group.id && (dropTargetGroupId = null)"
        @drop.prevent="dropPackage"
      >
        <div class="skill-group-header">
          <button
            class="package-drag-handle"
            type="button"
            draggable="true"
            :disabled="packageBusy"
            :title="t('packages.dragHint')"
            @dragstart="startGroupDrag($event, group.id)"
            @dragend="cancelGroupDrag"
          ><span></span><span></span><span></span><span></span></button>
          <span class="group-kind-icon" :class="{ system: !group.package }" aria-hidden="true">
            {{ group.package ? '◫' : '◇' }}
          </span>
          <button class="skill-group-toggle" type="button" @click="toggleGroup(group.id)">
            <span class="group-chevron" :class="{ expanded: expandedPackageIds.includes(group.id) }">▶</span>
            <strong>{{ group.package ? `${group.number}. ${group.name}` : group.name }}</strong>
            <span class="group-count">{{ group.skills.length }}</span>
          </button>
          <span v-if="group.description" class="group-description">{{ group.description }}</span>
          <details class="group-menu">
            <summary :aria-label="t('packages.moreActions')" :title="t('packages.moreActions')">•••</summary>
            <div class="group-menu-content">
              <button type="button" :disabled="!group.package" @click="group.package && openPackageDialog(group.package)">
                {{ t('packages.rename') }}
              </button>
              <button class="danger" type="button" :disabled="!group.package" @click="group.package && deletePackage(group.package)">
                {{ t('packages.delete') }}
              </button>
              <p v-if="!group.package" class="system-group-hint">{{ t('packages.systemGroupHint') }}</p>
            </div>
          </details>
        </div>
        <div v-show="expandedPackageIds.includes(group.id)" class="cards">
      <article
        v-for="skill in group.skills"
        :key="skill.id"
        class="card local-card"
        :class="{ linked: skill.usedBy.length > 0 }"
      >
        <div class="card-header">
          <div class="card-title-row">
            <label class="checkbox card-select">
              <input
                type="checkbox"
                :checked="selectedIds.includes(skill.id)"
                @change="toggleSelected(skill.id, ($event.target as HTMLInputElement).checked)"
              />
            </label>
            <div>
              <button class="skill-title-button" @click="openPreview(skill)">{{ skill.name }}</button>
              <div class="card-meta">
                {{ skill.usedBy.length > 0 ? t("local.linked") : t("local.unused") }}
                <span v-if="skill.source === 'legacy'" class="legacy-badge">
                  {{ t("local.legacyRepository") }}
                </span>
              </div>
            </div>
          </div>
          <div class="card-actions">
            <button class="favorite-button" :class="{ active: metadataEntry(skill).favorite }" :disabled="metadataBusy" :aria-label="metadataEntry(skill).favorite ? (locale === 'zh-CN' ? '取消收藏' : 'Unfavorite') : (locale === 'zh-CN' ? '收藏' : 'Favorite')" @click="toggleFavorite(skill)">★</button>
            <button class="primary" :disabled="installingId === skill.id" @click="$emit('install', skill)">
            {{ installingId === skill.id ? t("local.processing") : t("local.install") }}
            </button>
            <details class="skill-menu"><summary :aria-label="t('local.more')">•••</summary><div class="skill-menu-content">
            <button class="ghost" @click="historySkill = skill">{{ locale === 'zh-CN' ? '版本历史' : 'Version history' }}</button>
            <button class="ghost" @click="editorSkill = skill">{{ locale === 'zh-CN' ? '编辑 SKILL.md' : 'Edit SKILL.md' }}</button>
            <button class="ghost" @click="openTagEditor(skill)">{{ locale === 'zh-CN' ? '管理标签' : 'Manage tags' }}</button>
            <button class="ghost" @click="openAssignment([skill])">{{ t('packages.assignOne') }}</button>
            <button
              v-if="skill.sourceUrl && skill.sourceUrl.trim()"
              class="ghost"
              @click="$emit('updateLocal', skill)"
            >
              {{ t("local.updateOne") }}
            </button>
            <button class="ghost" @click="openPreview(skill)">
              {{ t("local.preview") }}
            </button>
            <button class="ghost" @click="$emit('openDir', skill.path)">
              {{ t("local.openDir") }}
            </button>
            <button class="ghost" @click="$emit('exportLocal', [skill])">
              {{ t("local.exportOne") }}
            </button>
            <button class="ghost danger" @click="$emit('deleteLocal', [skill])">
              {{ t("local.deleteOne") }}
            </button>
            </div></details>
          </div>
        </div>
        <p class="card-desc">{{ translatedDescriptions[skill.uuid] ?? skill.description }}</p>
        <div v-if="metadataEntry(skill).tags.length" class="skill-tags">
          <button v-for="tag in metadataEntry(skill).tags" :key="tag" type="button" @click="selectedTag = tag">#{{ tag }}</button>
        </div>
        <div class="ide-badges">
          <span
            v-for="badge in buildIdeBadgeList(skill)"
            :key="badge.label"
            class="ide-badge"
            :class="{ active: badge.active }"
          >
            {{ badge.label }}
          </span>
        </div>
      </article>
        </div>
      </section>
    </div>

    <SkillPreviewModal
      :visible="previewVisible"
      :skill="previewSkill"
      :preview="previewData"
      :loading="previewLoading"
      @close="closePreview"
    />
    <SkillEditor
      v-if="editorSkill"
      :key="editorSkill.path"
      :skill="editorSkill"
      @close="editorSkill = null"
      @saved="$emit('refresh')"
    />
    <div v-if="tagEditorSkill" class="tag-backdrop" @click.self="tagEditorSkill = null">
      <form class="tag-dialog" @submit.prevent="saveTags">
        <h2>{{ locale === 'zh-CN' ? '管理标签' : 'Manage tags' }} · {{ tagEditorSkill.name }}</h2>
        <p class="hint">{{ locale === 'zh-CN' ? '使用逗号分隔；每个标签最多 32 个字符，最多 20 个。' : 'Separate tags with commas; up to 20 tags and 32 characters each.' }}</p>
        <input v-model="tagDraft" class="input" autofocus :placeholder="locale === 'zh-CN' ? '例如：工作流, Windows, 写作' : 'Example: workflow, Windows, writing'" />
        <div class="tag-actions"><button class="ghost" type="button" @click="tagEditorSkill = null">{{ locale === 'zh-CN' ? '取消' : 'Cancel' }}</button><button class="primary" :disabled="metadataBusy" type="submit">{{ locale === 'zh-CN' ? '保存' : 'Save' }}</button></div>
      </form>
    </div>
    <div v-if="packageDialogOpen" class="tag-backdrop" @click.self="packageDialogOpen = false">
      <form class="tag-dialog" @submit.prevent="savePackage">
        <h2>{{ packageEditId ? t('packages.rename') : t('packages.create') }}</h2>
        <label class="package-field">{{ t('packages.name') }}
          <input v-model="packageName" class="input" required maxlength="100" autofocus />
        </label>
        <label class="package-field">{{ t('packages.description') }}
          <textarea v-model="packageDescription" class="input" rows="3" maxlength="4000"></textarea>
        </label>
        <div class="tag-actions"><button class="ghost" type="button" @click="packageDialogOpen = false">{{ t('packages.cancel') }}</button><button class="primary" :disabled="packageBusy || !packageName.trim()" type="submit">{{ t('packages.save') }}</button></div>
      </form>
    </div>
    <div v-if="assignmentOpen" class="tag-backdrop" @click.self="assignmentOpen = false">
      <form class="tag-dialog" @submit.prevent="assignPackage">
        <h2>{{ t('packages.assignTitle', { count: assignmentSkillIds.length }) }}</h2>
        <p class="hint">{{ t('packages.singleOwnerHint') }}</p>
        <select v-model="assignmentPackageId" class="input package-select" autofocus>
          <option value="">{{ t('packages.uncategorized') }}</option>
          <option v-for="(pkg, index) in sortedPackages" :key="pkg.id" :value="pkg.id">{{ index + 1 }}. {{ pkg.name }}</option>
        </select>
        <div class="tag-actions"><button class="ghost" type="button" @click="assignmentOpen = false">{{ t('packages.cancel') }}</button><button class="primary" :disabled="packageBusy" type="submit">{{ t('packages.assign') }}</button></div>
      </form>
    </div>
  </section>
</template>

<style scoped>
.panel-heading {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 24px;
}
.panel-heading-actions {
  display: flex;
  flex: 0 0 auto;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 10px;
}
.package-create { border-color: var(--color-accent-border); color: var(--color-accent); }
.skill-groups { display: flex; flex-direction: column; gap: 14px; margin-top: 22px; }
.skill-group { position: relative; overflow: visible; border: 1px solid var(--color-panel-border); border-radius: 14px; background: color-mix(in srgb, var(--color-card-bg) 62%, transparent); box-shadow: 0 5px 18px #0000000a; transition: border-color .15s, box-shadow .15s, opacity .15s, transform .15s; }
.skill-group::before { position: absolute; top: 11px; bottom: 11px; left: -1px; width: 3px; border-radius: 0 4px 4px 0; background: var(--color-accent); content: ""; opacity: .72; }
.skill-group:hover { border-color: var(--color-accent-border); box-shadow: 0 8px 24px #00000010; }
.skill-group.dragging { opacity: .48; transform: scale(.995); }
.skill-group.drop-target { border-color: var(--color-accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-accent) 16%, transparent); }
.skill-group-header { display: flex; align-items: center; gap: 10px; min-height: 56px; padding: 8px 12px; border-bottom: 1px solid var(--color-panel-border); border-radius: 14px 14px 0 0; background: color-mix(in srgb, var(--color-tabs-bg) 64%, transparent); }
.skill-group-toggle { display: inline-flex; align-items: center; gap: 8px; min-width: 0; padding: 7px 4px; border: 0; background: transparent; color: var(--color-text); cursor: pointer; }
.skill-group-toggle:hover strong { color: var(--color-accent); }
.skill-group-toggle strong { font-size: 15px; transition: color .15s; }
.group-chevron { color: var(--color-muted); font-size: 10px; transition: transform .15s; }
.group-chevron.expanded { transform: rotate(90deg); }
.group-count { min-width: 22px; padding: 2px 7px; border: 1px solid var(--color-panel-border); border-radius: 999px; background: var(--color-chip-bg); color: var(--color-muted); font-size: 11px; text-align: center; }
.group-description { flex: 1; min-width: 80px; overflow: hidden; color: var(--color-muted); font-size: 12px; text-overflow: ellipsis; white-space: nowrap; }
.group-kind-icon { display: grid; flex: 0 0 28px; width: 28px; height: 28px; place-items: center; border: 1px solid var(--color-accent-border); border-radius: 8px; background: color-mix(in srgb, var(--color-accent) 10%, transparent); color: var(--color-accent); font-size: 16px; }
.group-kind-icon.system { border-style: dashed; color: var(--color-muted); }
.package-drag-handle { display: grid; flex: 0 0 26px; width: 26px; height: 26px; grid-template-columns: repeat(2, 5px); place-content: center; gap: 3px; padding: 0; border: 1px solid #438cf3; border-radius: 7px; background: linear-gradient(145deg, #2679eb, #145cc7); box-shadow: 0 3px 8px #1769db3d; cursor: grab; }
.package-drag-handle:active { cursor: grabbing; }
.package-drag-handle:focus-visible, .group-menu summary:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 2px; }
.package-drag-handle span { width: 4px; height: 4px; border-radius: 50%; background: #fff; }
.group-menu { position: relative; margin-left: auto; }
.group-menu summary { display: grid; width: 34px; height: 34px; place-items: center; border-radius: 8px; color: var(--color-muted); font-size: 16px; font-weight: 700; letter-spacing: 1px; list-style: none; cursor: pointer; user-select: none; }
.group-menu summary::-webkit-details-marker { display: none; }
.group-menu summary:hover, .group-menu[open] summary { background: var(--color-chip-bg); color: var(--color-text); }
.group-menu-content { position: absolute; top: 39px; right: 0; z-index: 12; display: flex; width: 190px; flex-direction: column; gap: 3px; padding: 7px; border: 1px solid var(--color-panel-border); border-radius: 11px; background: var(--color-modal-bg); box-shadow: 0 12px 34px #0003; }
.group-menu-content button { width: 100%; padding: 8px 10px; border: 0; border-radius: 7px; background: transparent; color: var(--color-text); text-align: left; cursor: pointer; }
.group-menu-content button:hover:not(:disabled) { background: var(--color-tabs-bg); }
.group-menu-content button:disabled { color: var(--color-muted); cursor: not-allowed; opacity: .5; }
.group-menu-content button.danger:not(:disabled) { color: var(--color-danger-text); }
.system-group-hint { margin: 5px 4px 2px; padding-top: 7px; border-top: 1px solid var(--color-panel-border); color: var(--color-muted); font-size: 11px; line-height: 1.45; }
.package-assign { border-color: var(--color-accent-border); color: var(--color-accent); }
.package-field { display: flex; flex-direction: column; gap: 7px; margin-top: 14px; color: var(--color-muted); font-size: 12px; }
.package-field .input, .package-select { width: 100%; }
.package-select { margin-top: 16px; }
.repository-details { margin-top: 18px; font-size: 12px; color: var(--color-muted); }
.repository-details summary { cursor: pointer; width: fit-content; }
.skill-title-button { border: 0; background: transparent; padding: 0; color: var(--color-text); font-size: 15px; font-weight: 600; text-align: left; cursor: pointer; }
.skill-title-button:hover { color: var(--color-accent); }
.skill-menu { position: relative; }
.skill-menu summary { list-style: none; padding: 7px 10px; cursor: pointer; color: var(--color-muted); border-radius: 7px; }
.skill-menu summary:hover { background: var(--color-tabs-bg); }
.skill-menu-content { position: absolute; right: 0; top: 36px; z-index: 5; display: flex; flex-direction: column; min-width: 155px; padding: 6px; background: var(--color-modal-bg); border: 1px solid var(--color-panel-border); border-radius: 10px; box-shadow: 0 8px 30px #0002; }
.skill-menu-content button { text-align: left; border: 0; }
.cards { display: flex; flex-direction: column; gap: 0; }
.cards .local-card { padding: 19px 4px; border: 0; border-bottom: 1px solid var(--color-panel-border); border-radius: 0; background: transparent; box-shadow: none; }
.cards .local-card:hover { background: var(--color-card-bg); }
.cards .card-desc { margin: 6px 0 0 34px; font-size: 13px; color: var(--color-muted); display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
.cards .ide-badges { justify-content: flex-start; margin-left: 34px; margin-top: 8px; }
.cards .card-actions > .primary { background: transparent; color: var(--color-accent); border: 1px solid var(--color-panel-border); font-size: 12px; box-shadow: none; }
.cards .card-meta { font-size: 11px; margin-top: 4px; }
.panel-summary {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  margin-top: 8px;
  font-size: 13px;
  color: var(--color-muted);
}

.buttons {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 12px;
}

.search-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
  margin-top: 12px;
}

.search-row .input {
  flex: 1 1 280px;
}

.search-summary {
  white-space: nowrap;
}

.select-all {
  justify-content: flex-end;
}

.local-card {
  position: relative;
}

.local-card.linked {
  border-color: var(--color-success-border);
  box-shadow: inset 0 0 0 1px var(--color-success-border);
}
.favorite-filter { display: inline-flex; align-items: center; gap: 7px; color: var(--color-muted); font-size: 12px; white-space: nowrap; }
.tag-filter { flex: 0 1 170px !important; }
.favorite-button { border: 0; background: transparent; color: var(--color-panel-border); font-size: 21px; cursor: pointer; transition: color .15s, transform .15s; }
.favorite-button:hover { transform: scale(1.08); color: var(--color-accent); }
.favorite-button.active { color: #9a6de0; }
.skill-tags { display: flex; flex-wrap: wrap; gap: 6px; margin: 9px 0 0 34px; }
.skill-tags button { border: 0; border-radius: 999px; padding: 4px 9px; background: var(--color-chip-bg); color: var(--color-accent); font-size: 11px; cursor: pointer; }
.tag-backdrop { position: fixed; inset: 0; z-index: 1200; display: grid; place-items: center; padding: 24px; background: var(--color-overlay, #17122166); }
.tag-dialog { width: min(520px, 100%); padding: 26px; border: 1px solid var(--color-panel-border); border-radius: 18px; background: var(--color-modal-bg); box-shadow: 0 24px 70px #15102033; }
.tag-dialog h2 { margin: 0 0 8px; font-size: 18px; }
.tag-dialog .input { width: 100%; margin-top: 16px; }
.tag-actions { display: flex; justify-content: flex-end; gap: 10px; margin-top: 20px; }

@media (max-width: 640px) {
  .panel-heading { flex-direction: column; }
  .panel-heading-actions { width: 100%; justify-content: flex-start; }
}

.storage-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-top: 14px;
  padding: 12px 14px;
  border: 1px solid var(--color-panel-border);
  border-radius: 12px;
  background: var(--color-card-bg);
}

.storage-label {
  font-size: 13px;
  font-weight: 700;
}

.legacy-badge {
  display: inline-block;
  margin-left: 6px;
  padding: 2px 6px;
  border-radius: 999px;
  border: 1px solid var(--color-warning-border);
  background: var(--color-warning-bg);
  color: var(--color-warning-text);
  font-size: 11px;
}

.card-title-row,
.card-actions {
  display: flex;
  align-items: flex-start;
  gap: 10px;
}

.card-actions {
  flex-wrap: wrap;
  justify-content: flex-end;
}

.card-select {
  padding-top: 2px;
}

.skill-uuid {
  margin-top: 5px;
  color: var(--color-muted);
  font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  font-size: 11px;
  overflow-wrap: anywhere;
}

.ide-badges {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 6px;
  margin-top: 12px;
}

.ide-badge {
  padding: 4px 8px;
  border-radius: 999px;
  border: 1px solid var(--color-chip-border);
  background: transparent;
  color: var(--color-meta);
  font-size: 11px;
  line-height: 1.2;
}

.ide-badge.active {
  border-color: var(--color-success-border);
  background: var(--color-success-bg);
  color: var(--color-success-text);
  font-weight: 600;
}
</style>
