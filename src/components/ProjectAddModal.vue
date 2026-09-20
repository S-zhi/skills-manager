<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";

const { t } = useI18n();

defineProps<{
  visible: boolean;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "confirm", path: string, name: string): void;
}>();

const projectPath = ref("");
const projectName = ref("");

async function handleSelectFolder() {
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      directory: true,
      multiple: false,
      title: t("projects.selectFolder")
    });
    
    if (selected && typeof selected === "string") {
      projectPath.value = selected;
      // Extract folder name from path
      const parts = selected.split(/[\\/]/).filter(Boolean);
      projectName.value = parts[parts.length - 1] || t("projects.untitled");
    }
  } catch (err) {
    console.error("Failed to select folder:", err);
  }
}

function handleConfirm() {
  if (projectPath.value.trim() && projectName.value.trim()) {
    emit("confirm", projectPath.value.trim(), projectName.value.trim());
    projectPath.value = "";
    projectName.value = "";
  }
}

function handleClose() {
  emit("close");
  projectPath.value = "";
  projectName.value = "";
}
</script>

<template>
  <Teleport to="body">
    <Transition name="modal-fade">
      <div v-if="visible" class="modal-overlay" @click.self="handleClose">
        <div class="modal">
          <div class="modal-header">
            <h2 class="modal-title">{{ t("projects.addTitle") }}</h2>
            <button class="modal-close" @click="handleClose">×</button>
          </div>

          <div class="modal-body">
            <div class="form-group">
              <label class="form-label">{{ t("projects.projectPath") }}</label>
              <div class="input-with-button">
                <input
                  v-model="projectPath"
                  class="input"
                  :placeholder="t('projects.pathPlaceholder')"
                  readonly
                />
                <button class="icon-button" @click="handleSelectFolder" type="button">
                  {{ t("projects.selectFolderButton") }}
                </button>
              </div>
            </div>

            <div class="form-group">
              <label class="form-label">{{ t("projects.projectName") }}</label>
              <input
                v-model="projectName"
                class="input"
                :placeholder="t('projects.namePlaceholder')"
              />
            </div>

            <div class="hint">
              {{ t("projects.addHint") }}
            </div>
          </div>

          <div class="modal-footer">
            <button class="ghost" @click="handleClose">{{ t("projects.cancel") }}</button>
            <button
              class="primary"
              :disabled="!projectPath.trim() || !projectName.trim()"
              @click="handleConfirm"
            >
              {{ t("projects.add") }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: var(--color-overlay-bg);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: 20px;
}

.modal {
  background: var(--color-panel-bg);
  border-radius: 12px;
  max-width: 500px;
  width: 100%;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.12);
}

.modal-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 20px 24px;
  border-bottom: 1px solid var(--color-panel-border);
}

.modal-title {
  font-size: 18px;
  font-weight: 600;
  color: var(--color-text);
  margin: 0;
}

.modal-close {
  background: transparent;
  border: none;
  font-size: 28px;
  color: var(--color-muted);
  cursor: pointer;
  padding: 0;
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  transition: background 0.2s ease;
}

.modal-close:hover {
  background: var(--color-tabs-bg);
  color: var(--color-text);
}

.modal-body {
  padding: 24px;
}

.form-group {
  margin-bottom: 20px;
}

.form-label {
  display: block;
  font-size: 13px;
  font-weight: 500;
  color: var(--color-text);
  margin-bottom: 8px;
}

.input {
  width: 100%;
  padding: 10px 14px;
  background: var(--color-input-bg);
  border: 1px solid var(--color-input-border);
  border-radius: 6px;
  color: var(--color-text);
  font-size: 14px;
  transition: border-color 0.2s ease;
}

.input:focus {
  outline: none;
  border-color: var(--color-input-focus);
}

.input-with-button {
  display: flex;
  gap: 8px;
}

.input-with-button .input {
  flex: 1;
}

.icon-button {
  padding: 10px 16px;
  background: var(--color-primary-bg);
  border: 1px solid var(--color-primary-bg);
  border-radius: 6px;
  color: var(--color-primary-text);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
  white-space: nowrap;
}

.icon-button:hover {
  background: var(--color-primary-hover);
  border-color: var(--color-primary-hover);
  color: var(--color-primary-text);
}

.icon-button:active {
  background: var(--color-primary-active);
  border-color: var(--color-primary-active);
  color: var(--color-primary-text);
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  padding: 20px 24px;
  border-top: 1px solid var(--color-panel-border);
}

.modal-fade-enter-active,
.modal-fade-leave-active {
  transition: opacity 0.2s ease;
}

.modal-fade-enter-from,
.modal-fade-leave-to {
  opacity: 0;
}
</style>
