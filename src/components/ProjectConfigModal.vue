<script setup lang="ts">
import { ref, computed } from "vue";
import type { ProjectConfig, IdeOption } from "../composables/types";
import { useI18n } from "vue-i18n";

const { t } = useI18n();

const props = defineProps<{
  visible: boolean;
  project: ProjectConfig | null;
  ideOptions: IdeOption[];
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "save", projectId: string, ideTargets: string[]): void;
}>();

const selectedTargets = ref<string[]>([]);

const selectedIdeSet = computed(() => new Set(selectedTargets.value));

function toggleIdeTarget(ideLabel: string) {
  if (selectedIdeSet.value.has(ideLabel)) {
    selectedTargets.value = selectedTargets.value.filter((t) => t !== ideLabel);
  } else {
    selectedTargets.value = [...selectedTargets.value, ideLabel];
  }
}

function handleSave() {
  if (props.project) {
    emit("save", props.project.id, selectedTargets.value);
  }
}

function handleClose() {
  emit("close");
}

// Reset selected targets when project changes
if (props.project) {
  selectedTargets.value = [...props.project.ideTargets];
}
</script>

<template>
  <Teleport to="body">
    <Transition name="modal-fade">
      <div v-if="visible" class="modal-overlay" @click.self="handleClose">
        <div class="modal">
          <div class="modal-header">
            <h2 class="modal-title">{{ t("projects.configureTitle") }}</h2>
            <button class="modal-close" @click="handleClose">×</button>
          </div>

          <div v-if="project" class="modal-body">
            <div class="project-info">
              <div class="info-label">{{ t("projects.projectName") }}</div>
              <div class="info-value">{{ project.name }}</div>
            </div>
            <div class="project-info">
              <div class="info-label">{{ t("projects.projectPath") }}</div>
              <div class="info-value path">{{ project.path }}</div>
            </div>

            <div class="ide-selection">
              <div class="section-title">{{ t("projects.selectIdeTargets") }}</div>
              <div class="ide-grid">
                <button
                  v-for="option in ideOptions"
                  :key="option.id"
                  class="ide-checkbox"
                  :class="{ active: selectedIdeSet.has(option.label) }"
                  @click="toggleIdeTarget(option.label)"
                >
                  <input
                    type="checkbox"
                    :checked="selectedIdeSet.has(option.label)"
                    :disabled="false"
                  />
                  <span>{{ option.label }}</span>
                </button>
              </div>
            </div>

            <div class="hint">
              {{ t("projects.configureHint") }}
            </div>
          </div>

          <div class="modal-footer">
            <button class="ghost" @click="handleClose">{{ t("projects.cancel") }}</button>
            <button class="primary" @click="handleSave">{{ t("projects.save") }}</button>
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
  max-width: 600px;
  width: 100%;
  max-height: 80vh;
  overflow-y: auto;
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

.project-info {
  margin-bottom: 16px;
}

.info-label {
  font-size: 12px;
  color: var(--color-muted);
  margin-bottom: 4px;
}

.info-value {
  font-size: 14px;
  color: var(--color-text);
  font-weight: 500;
}

.info-value.path {
  word-break: break-all;
  font-family: monospace;
  background: var(--color-card-bg);
  padding: 8px 12px;
  border-radius: 6px;
}

.ide-selection {
  margin-top: 24px;
}

.section-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--color-text);
  margin-bottom: 12px;
}

.ide-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 8px;
}

.ide-checkbox {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  background: var(--color-card-bg);
  border: 1px solid var(--color-card-border);
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.2s ease;
  font-size: 13px;
  color: var(--color-text);
}

.ide-checkbox:hover {
  border-color: var(--color-primary-bg);
  background: var(--color-tabs-bg);
}

/* Active state removed - no color change on selection */

.ide-checkbox input[type="checkbox"] {
  pointer-events: none;
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
