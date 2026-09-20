<script setup lang="ts">
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { ProjectConfig, LocalSkill, IdeOption } from "../composables/types";
import { useI18n } from "vue-i18n";

const { t } = useI18n();

const props = defineProps<{
  projects: ProjectConfig[];
  selectedProjectId: string | null;
  localSkills: LocalSkill[];
  ideOptions: IdeOption[];
  localLoading: boolean;
}>();

const emit = defineEmits<{
  (e: "addProject"): void;
  (e: "removeProject", projectId: string): void;
  (e: "selectProject", projectId: string | null): void;
  (e: "configureProject", projectId: string): void;
  (e: "linkSkills", projectId: string): void;
}>();

function handleAddProject() {
  emit("addProject");
}

function handleRemoveProject(projectId: string) {
  emit("removeProject", projectId);
}

function handleSelectProject(projectId: string) {
  emit("selectProject", projectId === props.selectedProjectId ? null : projectId);
}

function handleConfigureProject(projectId: string) {
  emit("configureProject", projectId);
}

function handleLinkSkills(projectId: string) {
  emit("linkSkills", projectId);
}

async function handleOpenDirectory(project: ProjectConfig) {
  try {
    await revealItemInDir(project.path);
  } catch (err) {
    console.error("Failed to open directory:", err);
  }
}

function buildIdeBadgeList(project: ProjectConfig) {
  return project.ideTargets.map((ideLabel) => ({
    label: ideLabel,
    active: true
  }));
}
</script>

<template>
  <section class="panel">
    <div class="panel-header">
      <div class="panel-title">{{ t("projects.title") }}</div>
      <button class="primary" @click="handleAddProject">
        {{ t("projects.add") }}
      </button>
    </div>
    <div class="hint">{{ t("projects.hint") }}</div>

    <div v-if="projects.length === 0" class="hint">{{ t("projects.emptyHint") }}</div>

    <div v-else class="project-list">
      <div
        v-for="project in projects"
        :key="project.id"
        class="project-item"
        :class="{ selected: selectedProjectId === project.id }"
      >
        <div class="project-header">
          <div class="project-info">
            <div class="project-name">{{ project.name }}</div>
            <div class="project-path">{{ project.path }}</div>
          </div>
          <div class="project-actions">
            <button
              class="ghost small"
              @click="handleSelectProject(project.id)"
            >
              {{ selectedProjectId === project.id ? t("projects.deselect") : t("projects.select") }}
            </button>
            <button
              class="ghost small"
              @click="handleConfigureProject(project.id)"
            >
              {{ t("projects.configure") }}
            </button>
            <button
              class="ghost small"
              @click="handleOpenDirectory(project)"
            >
              {{ t("projects.openDirectory") }}
            </button>
            <button
              class="primary small"
              :disabled="localLoading || project.ideTargets.length === 0"
              @click="handleLinkSkills(project.id)"
            >
              {{ t("projects.linkSkills") }}
            </button>
            <button
              class="ghost danger small"
              @click="handleRemoveProject(project.id)"
            >
              {{ t("projects.remove") }}
            </button>
          </div>
        </div>
        <div class="project-meta">
          <span class="meta-item">
            {{ t("projects.ideTargets", { count: project.ideTargets.length }) }}
          </span>
          <span v-if="project.detectedIdeDirs.length > 0" class="meta-item">
            {{ t("projects.detected", { count: project.detectedIdeDirs.length }) }}
          </span>
        </div>
        <div class="ide-badges">
          <span
            v-for="badge in buildIdeBadgeList(project)"
            :key="badge.label"
            class="ide-badge"
            :class="{ active: badge.active }"
          >
            {{ badge.label }}
          </span>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
}

.panel-title {
  font-size: 18px;
  font-weight: 600;
  color: var(--color-text);
  margin: 0;
}

.project-list {
  margin-top: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.project-item {
  padding: 16px;
  background: var(--color-card-bg);
  border: 1px solid var(--color-card-border);
  border-radius: 8px;
  transition: all 0.2s ease;
}

.project-item.selected {
  border-color: var(--color-primary-bg);
  box-shadow: 0 0 0 2px rgba(0, 113, 227, 0.2);
}

.project-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 16px;
  flex-wrap: wrap;
}

.project-info {
  flex: 1;
  min-width: 200px;
}

.project-name {
  font-size: 15px;
  font-weight: 600;
  color: var(--color-text);
  margin-bottom: 4px;
}

.project-path {
  font-size: 12px;
  color: var(--color-muted);
  word-break: break-all;
}

.project-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.project-actions button {
  padding: 6px 12px;
  font-size: 13px;
}

.project-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
  margin-top: 12px;
  font-size: 12px;
  color: var(--color-muted);
}

.meta-item {
  padding: 2px 8px;
  background: var(--color-chip-bg);
  border-radius: 999px;
  font-size: 11px;
}

.ide-badges {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 8px;
}

.ide-badge {
  padding: 4px 8px;
  border-radius: 999px;
  border: 1px solid var(--color-chip-border);
  background: transparent;
  color: var(--color-muted);
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
