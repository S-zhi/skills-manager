<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
import type { LocalSkill, LocalSkillPreview } from "../composables/types";

const props = defineProps<{ skill: LocalSkill }>();
const emit = defineEmits<{ (e: "close"): void; (e: "saved"): void }>();
const { locale } = useI18n();
const text = (zh: string, en: string) => locale.value.startsWith("zh") ? zh : en;

const original = ref("");
const content = ref("");
const skillMdPath = ref("");
const loading = ref(true);
const saving = ref(false);
const error = ref("");
const notice = ref("");
const externalConflict = ref(false);
const dirty = computed(() => content.value !== original.value);

async function load() {
  loading.value = true;
  error.value = "";
  externalConflict.value = false;
  try {
    const result = await invoke<LocalSkillPreview>("read_local_skill_preview", {
      skillPath: props.skill.path
    });
    original.value = result.skillMdContent;
    content.value = result.skillMdContent;
    skillMdPath.value = result.skillMdPath;
    notice.value = "";
  } catch (reason) {
    error.value = String(reason);
  } finally {
    loading.value = false;
  }
}

async function save() {
  if (!dirty.value || saving.value) return;
  saving.value = true;
  error.value = "";
  notice.value = "";
  externalConflict.value = false;
  try {
    const result = await invoke<LocalSkillPreview>("save_local_skill", {
      request: {
        skillPath: props.skill.path,
        expectedContent: original.value,
        content: content.value
      }
    });
    original.value = result.skillMdContent;
    content.value = result.skillMdContent;
    skillMdPath.value = result.skillMdPath;
    notice.value = text("已保存，并创建了可恢复的编辑前快照。", "Saved with a restorable pre-edit snapshot.");
    emit("saved");
  } catch (reason) {
    error.value = String(reason);
    externalConflict.value = error.value.includes("外部修改") || error.value.includes("changed externally");
  } finally {
    saving.value = false;
  }
}

function close() {
  if (dirty.value && !window.confirm(text("有尚未保存的更改，确定关闭吗？", "Discard unsaved changes?"))) return;
  emit("close");
}

onMounted(load);
</script>

<template>
  <div class="editor-backdrop" @click.self="close">
    <section class="editor-panel" role="dialog" aria-modal="true" :aria-label="text('编辑 Skill', 'Edit Skill')">
      <header class="editor-header">
        <div>
          <span class="editor-kicker">SKILL.md</span>
          <h2>{{ text('编辑', 'Edit') }} · {{ skill.name }}</h2>
          <p>{{ text('直接编辑完整文档；UUID 和名称受到保护。', 'Edit the full document; UUID and name are protected.') }}</p>
        </div>
        <button class="editor-close" type="button" :aria-label="text('关闭', 'Close')" @click="close">×</button>
      </header>

      <div class="editor-meta">
        <span>UUID · {{ skill.uuid }}</span>
        <span :title="skillMdPath">{{ skillMdPath || skill.path }}</span>
      </div>

      <div v-if="loading" class="editor-state">{{ text('正在读取…', 'Loading…') }}</div>
      <textarea
        v-else
        v-model="content"
        class="editor-textarea"
        spellcheck="false"
        :disabled="saving"
        :aria-label="text('SKILL.md 内容', 'SKILL.md content')"
      />

      <p v-if="error" class="editor-error" role="alert">{{ error }}</p>
      <p v-if="notice" class="editor-notice" role="status">{{ notice }}</p>
      <footer class="editor-footer">
        <span class="editor-status">
          {{ dirty ? text('有未保存的更改', 'Unsaved changes') : text('内容已同步', 'Content is up to date') }}
        </span>
        <div class="editor-actions">
          <button v-if="externalConflict" class="ghost" type="button" :disabled="saving" @click="load">
            {{ text('重新加载磁盘版本', 'Reload disk version') }}
          </button>
          <button class="ghost" type="button" :disabled="saving" @click="close">{{ text('关闭', 'Close') }}</button>
          <button class="primary" type="button" :disabled="loading || saving || !dirty" @click="save">
            {{ saving ? text('保存中…', 'Saving…') : text('保存', 'Save') }}
          </button>
        </div>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.editor-backdrop { position: fixed; inset: 0; z-index: 1100; display: flex; justify-content: flex-end; background: var(--color-overlay, rgba(24, 18, 35, .38)); }
.editor-panel { width: min(780px, 94vw); height: 100vh; display: flex; flex-direction: column; background: var(--color-panel-bg); border-left: 1px solid var(--color-panel-border); box-shadow: -24px 0 70px rgba(22, 16, 34, .2); }
.editor-header { display: flex; justify-content: space-between; gap: 24px; padding: 28px 30px 20px; border-bottom: 1px solid var(--color-panel-border); }
.editor-header h2 { margin: 6px 0 7px; font-size: 23px; }
.editor-header p { margin: 0; color: var(--color-muted); font-size: 13px; }
.editor-kicker { color: var(--color-accent); font-size: 11px; font-weight: 750; letter-spacing: .12em; }
.editor-close { align-self: flex-start; border: 0; background: transparent; color: var(--color-muted); font-size: 30px; cursor: pointer; }
.editor-meta { display: grid; grid-template-columns: minmax(160px, .8fr) minmax(0, 1.5fr); gap: 12px; padding: 13px 30px; color: var(--color-muted); background: var(--color-card-bg); border-bottom: 1px solid var(--color-panel-border); font: 11px/1.5 Consolas, monospace; }
.editor-meta span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.editor-textarea { flex: 1; min-height: 0; width: 100%; resize: none; padding: 24px 30px; border: 0; outline: 0; color: var(--color-text); background: var(--color-panel-bg); font: 13px/1.72 Consolas, "SFMono-Regular", monospace; tab-size: 2; }
.editor-state { flex: 1; padding: 30px; color: var(--color-muted); }
.editor-error, .editor-notice { margin: 0; padding: 11px 30px; font-size: 12px; overflow-wrap: anywhere; }
.editor-error { color: var(--color-error-text); background: color-mix(in srgb, var(--color-error-text) 8%, transparent); }
.editor-notice { color: var(--color-success-text, #297a52); background: color-mix(in srgb, #35a46f 9%, transparent); }
.editor-footer { display: flex; align-items: center; justify-content: space-between; gap: 18px; padding: 16px 30px; border-top: 1px solid var(--color-panel-border); background: var(--color-card-bg); }
.editor-status { color: var(--color-muted); font-size: 12px; }
.editor-actions { display: flex; gap: 10px; }
@media (max-width: 640px) { .editor-panel { width: 100%; } .editor-header, .editor-textarea { padding-left: 20px; padding-right: 20px; } .editor-meta { grid-template-columns: 1fr; padding-left: 20px; padding-right: 20px; } .editor-footer { align-items: stretch; flex-direction: column; padding: 14px 20px; } .editor-actions { flex-wrap: wrap; } }
</style>
