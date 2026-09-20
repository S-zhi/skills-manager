<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import type { LocalSkill } from '../composables/types';
const props = defineProps<{ skill: LocalSkill }>();
const emit = defineEmits<{ (e: 'close'): void; (e: 'restored'): void }>();
const { locale } = useI18n();
const text = (zh: string, en: string) => locale.value.startsWith('zh') ? zh : en;
interface Snapshot { id: string; createdAt: number; reason: string }
const records = ref<Snapshot[]>([]);
const busy = ref(false);
const error = ref('');
const confirm = ref<string | null>(null);
const notice = ref('');
async function load() { records.value = await invoke('list_skill_history', { skillPath: props.skill.path }); }
function reasonLabel(reason: string) {
  if (reason === 'before-update') return text('更新前', 'Before update');
  if (reason === 'before-edit') return text('编辑前', 'Before edit');
  return text('恢复前', 'Before restore');
}
async function restore(id: string) {
  busy.value = true; error.value = ''; notice.value = '';
  try {
    await invoke('restore_skill_history', { skillPath: props.skill.path, id });
    confirm.value = null; emit('restored');
    notice.value = text('已恢复，恢复前的版本也已保存。', 'Restored. The previous version was also saved.');
    await load();
  } catch (e) { error.value = String(e); }
  finally { busy.value = false; }
}
onMounted(async () => { busy.value = true; try { await load(); } catch(e) { error.value = String(e); } finally { busy.value = false; } });
</script>
<template>
  <section class="history" :aria-busy="busy">
    <div class="history-heading"><h2>{{ text('版本历史', 'Version history') }} · {{ skill.name }}</h2><button class="ghost" :disabled="busy" @click="emit('close')">{{ text('关闭', 'Close') }}</button></div>
    <p class="hint">{{ text('更新前保存完整快照。恢复会替换当前文件，但保留 UUID 和原路径，并先备份当前版本。IDE 链接会使用恢复后的内容；自动云备份会同步恢复结果。', 'Complete snapshots are saved before updates. Restore replaces current files, preserves UUID/path and first saves the current version. Linked IDEs use the restored content; automatic cloud backup mirrors it.') }}</p>
    <p v-if="error" role="alert" class="history-error">{{ error }}</p>
    <p v-if="notice" role="status">{{ notice }}</p>
    <p v-if="busy" class="hint">{{ text('处理中…', 'Working…') }}</p>
    <p v-else-if="!records.length" class="hint">{{ text('尚无快照，下一次更新前会自动保存。', 'No snapshots yet. One will be saved before your next update.') }}</p>
    <article v-for="record in records" :key="record.id" class="history-record">
      <span>{{ new Date(record.createdAt * 1000).toLocaleString(locale) }} · {{ reasonLabel(record.reason) }}</span>
      <button v-if="confirm !== record.id" class="ghost" :disabled="busy" @click="confirm = record.id">{{ text('恢复此版本', 'Restore this version') }}</button>
      <template v-else><span>{{ text('确认替换当前文件？', 'Replace current files?') }}</span><button class="primary" :disabled="busy" @click="restore(record.id)">{{ text('确认恢复', 'Confirm restore') }}</button><button class="ghost" :disabled="busy" @click="confirm = null">{{ text('取消', 'Cancel') }}</button></template>
    </article>
  </section>
</template>
<style scoped>
.history { padding: 20px; border: 1px solid var(--color-accent-border); background: var(--color-card-bg); border-radius: 12px; margin: 16px 0; }
.history-heading, .history-record { display: flex; flex-wrap: wrap; gap: 12px; align-items: center; justify-content: space-between; }
h2 { font-size: 16px; }
.history-record { padding: 12px 0; border-bottom: 1px solid var(--color-panel-border); font-size: 12px; }
.history-error { color: var(--color-error-text); overflow-wrap: anywhere; }
</style>
