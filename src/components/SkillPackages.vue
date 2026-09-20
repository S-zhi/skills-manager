<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import AppIcon from './AppIcon.vue';
import { useI18n } from "vue-i18n";
import { useToast } from "../composables/useToast";
import type { LocalSkill, SkillPackage, SkillPackageStore } from "../composables/types";

const props = defineProps<{ skills: LocalSkill[]; loading: boolean }>();
const emit = defineEmits<{
  (e: "install", skills: LocalSkill[]): void;
  (e: "export", skills: LocalSkill[]): void;
}>();
const { t } = useI18n();
const toast = useToast();
const store = ref<SkillPackageStore>({ schemaVersion: 1, revision: 0, packages: [] });
const busy = ref(false);
const ready = ref(false);
const error = ref("");
const selectedId = ref("");
const editing = ref(false);
const editId = ref<string | null>(null);
const editRevision = ref(0);
const name = ref("");
const description = ref("");
const memberIds = ref<string[]>([]);
const search = ref("");
const deleteId = ref<string | null>(null);
const dialog = ref<HTMLFormElement | null>(null);
let previousFocus: HTMLElement | null = null;
watch(editing, async (open) => {
  if (open) {
    previousFocus = document.activeElement as HTMLElement | null;
    await nextTick();
    dialog.value?.querySelector<HTMLInputElement>('input')?.focus();
  } else { previousFocus?.focus(); }
});
function trapFocus(event: KeyboardEvent) {
  if (event.key !== 'Tab') return;
  const elements = Array.from(dialog.value?.querySelectorAll<HTMLElement>('input:not(:disabled), textarea:not(:disabled), button:not(:disabled)') ?? []);
  const first = elements[0];
  const last = elements[elements.length - 1];
  if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
}
const skillMap = computed(() => new Map(props.skills.map(skill => [skill.uuid, skill])));
const selected = computed(() => store.value.packages.find(pkg => pkg.id === selectedId.value));
const members = computed(() => (selected.value?.skillUuids ?? []).flatMap(id => {
  const skill = skillMap.value.get(id);
  return skill ? [skill] : [];
}));
const filteredSkills = computed(() => {
  const keyword = search.value.trim().toLowerCase();
  return [...skillMap.value.values()].filter(skill =>
    [skill.name, skill.uuid, skill.path].some(value => value.toLowerCase().includes(keyword)));
});
const missingMembers = computed(() => memberIds.value.filter(id => !skillMap.value.has(id)));

async function load() {
  busy.value = true;
  error.value = "";
  try {
    store.value = await invoke<SkillPackageStore>("list_skill_packages");
    ready.value = true;
    if (!store.value.packages.some(pkg => pkg.id === selectedId.value)) {
      selectedId.value = store.value.packages[0]?.id ?? "";
    }
  } catch (err) {
    ready.value = false;
    error.value = String(err);
  } finally { busy.value = false; }
}

function edit(pkg?: SkillPackage) {
  editId.value = pkg?.id ?? null;
  editRevision.value = store.value.revision;
  name.value = pkg?.name ?? "";
  description.value = pkg?.description ?? "";
  memberIds.value = [...(pkg?.skillUuids ?? [])];
  search.value = "";
  deleteId.value = null;
  editing.value = true;
}

function selectVisible() {
  memberIds.value = [...new Set([...memberIds.value, ...filteredSkills.value.map(skill => skill.uuid)])];
}

async function save() {
  busy.value = true;
  try {
    const result = await invoke<SkillPackageStore>("save_skill_package", {
      request: { id: editId.value, name: name.value, description: description.value,
        skillUuids: memberIds.value, revision: editRevision.value }
    });
    selectedId.value = editId.value ?? result.packages[result.packages.length - 1]?.id ?? "";
    store.value = result;
    editing.value = false;
    toast.success(t("packages.saved"));
  } catch (err) { toast.error(String(err)); }
  finally { busy.value = false; }
}

async function remove() {
  if (!deleteId.value) return;
  busy.value = true;
  try {
    store.value = await invoke<SkillPackageStore>("delete_skill_package", {
      id: deleteId.value, revision: store.value.revision
    });
    selectedId.value = store.value.packages[0]?.id ?? "";
    deleteId.value = null;
    toast.success(t("packages.deleted"));
  } catch (err) { toast.error(String(err)); }
  finally { busy.value = false; }
}

onMounted(load);
</script>

<template>
  <section class="package-section" :aria-label="t('packages.title')">
    <div class="package-toolbar">
      <strong>{{ store.packages.length }} {{ t('packages.title') }}</strong>
      <button class="primary" :disabled="busy || !ready || editing || loading" @click="edit()">{{ t('packages.create') }}</button>
      <button class="ghost" :disabled="busy || editing" @click="load">{{ t('market.refresh') }}</button>
    </div>
    <p v-if="error" role="alert">{{ error }}</p>
    <p v-else-if="!ready" role="status">{{ t('packages.loading') }}</p>
    <p v-else-if="store.packages.length === 0 && !editing" class="hint">{{ t('packages.empty') }}</p>

    <template v-if="ready && store.packages.length && !editing">
      <div class="package-grid">
        <button v-for="(pkg, index) in store.packages" :key="pkg.id" class="package-tile" :class="{ selected: selectedId === pkg.id }" :disabled="busy" @click="selectedId = pkg.id; deleteId = null">
          <span class="package-symbol" :class="`tone-${index % 3}`"><AppIcon name="package" :size="23" /></span>
          <strong>{{ pkg.name }}</strong>
          <span class="tile-description">{{ pkg.description || t('local.previewEmptyDescription') }}</span>
          <span class="tile-count">{{ pkg.skillUuids.length }} Skills <span>↗</span></span>
        </button>
      </div>
      <div v-if="selected">
        <p class="package-description">{{ selected.description || t('local.previewEmptyDescription') }}</p>
        <details class="package-id"><summary>UUID</summary><p class="package-uuid">{{ selected.id }}</p></details>
        <div class="package-toolbar">
          <button class="ghost" :disabled="busy || loading" @click="edit(selected)">{{ t('packages.edit') }}</button>
          <button class="ghost" :disabled="busy || loading || members.length === 0" @click="emit('install', members)">{{ t('packages.install') }}</button>
          <button class="ghost" :disabled="busy || loading || members.length === 0" @click="emit('export', members)">{{ t('packages.export') }}</button>
          <button class="ghost danger" :disabled="busy" @click="deleteId = selected.id">{{ t('packages.delete') }}</button>
        </div>
        <div v-if="deleteId" class="package-confirm" role="alert">
          <p>{{ t('packages.deleteConfirm') }}</p>
          <button class="ghost danger" :disabled="busy" @click="remove">{{ t('packages.confirmDelete') }}</button>
          <button class="ghost" :disabled="busy" @click="deleteId = null">{{ t('packages.cancel') }}</button>
        </div>
        <p v-if="!selected.skillUuids.length" class="hint">{{ t('packages.noMembers') }}</p>
        <ul v-else class="package-members">
          <li v-for="id in selected.skillUuids" :key="id">
            {{ skillMap.get(id)?.name ?? (loading ? t('packages.loading') : t('packages.missing')) }}
            <span v-if="!skillMap.has(id)" class="package-uuid">{{ id }}</span>
          </li>
        </ul>
      </div>
    </template>

    <div v-if="editing" class="package-overlay" @keydown.esc="!busy && (editing = false)">
    <form ref="dialog" role="dialog" aria-modal="true" :aria-label="editId ? t('packages.edit') : t('packages.create')" class="package-dialog" @submit.prevent="save" @keydown="trapFocus">
      <fieldset :disabled="busy || loading" class="package-editor">
        <legend>{{ editId ? t('packages.edit') : t('packages.create') }}</legend>
        <label class="package-field">{{ t('packages.name') }}
          <input v-model="name" class="input" required maxlength="100" autofocus />
        </label>
        <label class="package-field">{{ t('packages.description') }}
          <textarea v-model="description" class="input" rows="3" maxlength="4000" />
        </label>
        <label class="package-field">{{ t('packages.members') }} ({{ memberIds.length }})
          <input v-model="search" class="input" :placeholder="t('packages.search')" />
        </label>
        <div class="package-toolbar">
          <button type="button" class="ghost" :disabled="!filteredSkills.length" @click="selectVisible">{{ t('discovery.selectVisible') }}</button>
          <button type="button" class="ghost" :disabled="!memberIds.length" @click="memberIds = []">{{ t('discovery.clearSelection') }}</button>
        </div>
        <div class="package-members">
          <label v-for="skill in filteredSkills" :key="skill.uuid" class="package-member">
            <input v-model="memberIds" type="checkbox" :value="skill.uuid" />
            <span>{{ skill.name }}<span class="package-uuid">{{ skill.uuid }} · {{ skill.path }}</span></span>
          </label>
          <p v-if="!filteredSkills.length" class="hint">{{ t('local.searchEmptyHint') }}</p>
          <label v-for="id in missingMembers" :key="id" class="package-member">
            <input v-model="memberIds" type="checkbox" :value="id" />
            <span>{{ t('packages.missing') }}<span class="package-uuid">{{ id }}</span></span>
          </label>
        </div>
        <div class="package-toolbar">
          <button type="submit" class="primary" :disabled="!name.trim()">{{ busy ? t('local.processing') : t('packages.save') }}</button>
          <button type="button" class="ghost" @click="editing = false">{{ t('packages.cancel') }}</button>
        </div>
      </fieldset>
    </form>
    </div>
  </section>
</template>

<style scoped>
.package-section { margin-top: 26px; padding: 0; }
.package-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(210px, 1fr)); gap: 16px; margin: 24px 0; }
.package-tile { display: flex; flex-direction: column; gap: 12px; padding: 20px; text-align: left; border: 1px solid var(--color-panel-border); border-radius: 14px; background: var(--color-panel-bg); color: var(--color-text); cursor: pointer; min-height: 192px; }
.package-tile:hover { box-shadow: var(--shadow-soft); border-color: var(--color-accent-border); }
.package-tile.selected { border-color: var(--color-accent); background: var(--color-accent-soft); box-shadow: 0 0 0 1px var(--color-focus-ring); }
.package-symbol { font-size: 24px; width: 43px; height: 43px; display: grid; place-items: center; background: var(--color-accent-soft); color: var(--color-accent); border-radius: 11px; }
.tone-1 { background: var(--color-warning-bg); color: var(--color-warning-text); }
.tone-2 { background: var(--color-success-bg); color: var(--color-success-text); }
.package-tile strong { font-size: 15px; }
.tile-description { font-size: 12px; color: var(--color-muted); display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
.tile-count { display: flex; justify-content: space-between; width: 100%; margin-top: auto; font-size: 11px; color: var(--color-muted); }
.package-id { margin: 10px 0; font-size: 11px; color: var(--color-muted); }
.package-id summary { cursor: pointer; }
.package-overlay { position: fixed; inset: 0; z-index: 900; background: var(--color-overlay); display: grid; place-items: center; padding: 24px; }
.package-dialog { background: var(--color-modal-bg); border: 1px solid var(--color-panel-border); border-radius: 16px; width: min(620px, 100%); padding: 26px; max-height: 90vh; overflow-y: auto; box-shadow: 0 24px 80px #0003; }
.package-dialog legend { font-size: 21px; font-weight: 650; margin-bottom: 12px; }
.package-toolbar { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; }
.package-toolbar strong { margin-right: auto; }
.package-field { display: flex; flex-direction: column; gap: 6px; margin: 12px 0; }
.package-editor { border: 0; padding: 0; min-width: 0; margin-top: 14px; }
.package-description { white-space: pre-wrap; overflow-wrap: anywhere; }
.package-uuid { display: block; font-size: 11px; color: var(--color-muted); overflow-wrap: anywhere; }
.package-members { max-height: 280px; overflow-y: auto; padding: 8px; margin: 10px 0; list-style: none; }
.package-members li { padding: 5px 0; }
.package-member { display: flex; align-items: flex-start; gap: 10px; padding: 8px 0; cursor: pointer; }
.package-member input { margin-top: 4px; }
.package-confirm { padding: 10px 0; }
.package-confirm button { margin-right: 10px; }
</style>
