<script setup lang="ts">
import { useI18n } from "vue-i18n";

defineProps<{
  visible: boolean;
  targetName: string;
  mode: "ide" | "local";
}>();

defineEmits<{
  (e: "confirm"): void;
  (e: "cancel"): void;
}>();

const { t, locale } = useI18n();
</script>

<template>
  <div v-if="visible" class="modal-backdrop">
    <div class="modal">
      <div class="modal-title">
        {{ mode === "local" ? (locale === 'zh-CN' ? '移入回收站' : 'Move to recycle bin') : t("uninstallModal.title") }}
      </div>
      <div class="hint">
        {{ mode === "local" ? (locale === 'zh-CN' ? '文件将保留在回收站，可恢复。相关 IDE 链接可能暂时失效，包引用会保留；开启 GitHub 自动备份时，该删除也会同步到远端。' : 'Files remain recoverable in the recycle bin. IDE links may temporarily break; package references are retained. If GitHub auto backup is enabled, this removal is also mirrored remotely.') : t("uninstallModal.hint") }}
      </div>
      <div class="card-link">{{ targetName }}</div>
      <div class="modal-actions">
        <button class="ghost" @click="$emit('cancel')">{{ t("uninstallModal.cancel") }}</button>
        <button class="primary" @click="$emit('confirm')">
          {{ mode === "local" ? (locale === 'zh-CN' ? '移入回收站' : 'Move to recycle bin') : t("uninstallModal.confirm") }}
        </button>
      </div>
    </div>
  </div>
</template>
