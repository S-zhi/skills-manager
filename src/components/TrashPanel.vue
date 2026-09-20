<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
const { locale } = useI18n();
const text = (zh: string, en: string) => locale.value.startsWith('zh') ? zh : en;
interface Item { id: string; name: string; uuid: string | null; originalPath: string; deletedAt: number }
const items = ref<Item[]>([]);
const busy = ref(false);
const error = ref('');
const notice = ref('');
const deleting = ref<string | null>(null);
const confirmation = ref('');
const emit = defineEmits<{ (e: 'changed'): void }>();
async function refresh() {
  busy.value = true; error.value = '';
  try { items.value = await invoke<Item[]>('list_trashed_skills'); }
  catch (e) { error.value = String(e); }
  finally { busy.value = false; }
}
async function act(item: Item, permanent: boolean) {
  busy.value = true; error.value = ''; notice.value = '';
  try {
    if (permanent) await invoke('permanently_delete_trashed_skill', { id: item.id, confirmation: confirmation.value });
    else await invoke('restore_trashed_skill', { id: item.id });
    deleting.value = null; confirmation.value = '';
    notice.value = permanent ? text('已永久删除，无法从回收站恢复。', 'Permanently deleted; cannot be restored from the recycle bin.') : text('已恢复至原位置。', 'Restored to its original location.');
    emit('changed');
    items.value = await invoke<Item[]>('list_trashed_skills');
  } catch (e) { error.value = String(e); }
  finally { busy.value = false; }
}
onMounted(refresh);
</script>

<template>
  <section class="panel">
    <h1 class="panel-title">{{ text('回收站', 'Recycle bin') }}</h1>
    <p class="hint">{{ text('保留文件和 UUID，不自动清理。恢复不会覆盖同名目录；恢复原路径后，原有包引用及仍保留的 IDE 链接可重新生效。回收站不上传至 GitHub。', 'Files and UUIDs are retained without automatic cleanup. Restore never overwrites an occupied path. Existing package references and retained IDE links can reconnect after restore. Recycled files are not uploaded to GitHub.') }}</p>
    <button class="ghost" :disabled="busy" @click="refresh">{{ text('刷新', 'Refresh') }}</button>
    <p v-if="error" class="trash-error" role="alert">{{ error }}</p>
    <p v-if="notice" role="status" class="hint">{{ notice }}</p>
    <p v-if="!items.length && !busy && !error" class="hint">{{ text('回收站是空的。', 'The recycle bin is empty.') }}</p>
    <div class="cards" :aria-busy="busy">
      <article v-for="item in items" :key="item.id" class="card">
        <h2 class="card-title">{{ item.name }}</h2>
        <p class="hint">{{ new Date(item.deletedAt * 1000).toLocaleString(locale) }}</p>
        <p class="trash-path">{{ item.originalPath }}</p>
        <p v-if="item.uuid" class="hint">UUID: {{ item.uuid }}</p>
        <div class="trash-actions">
          <button class="primary" :disabled="busy" @click="act(item, false)">{{ text('恢复', 'Restore') }}</button>
          <button class="ghost" :disabled="busy" @click="deleting = item.id; confirmation = ''">{{ text('永久删除…', 'Delete permanently…') }}</button>
        </div>
        <form v-if="deleting === item.id" class="trash-confirm" @submit.prevent="act(item, true)">
          <label :for="'confirm-' + item.id">{{ text('永久删除无法撤销。输入 DELETE 确认：', 'Permanent deletion cannot be undone. Type DELETE to confirm:') }}</label>
          <input :id="'confirm-' + item.id" v-model="confirmation" class="input" autocomplete="off" :disabled="busy" />
          <div class="trash-actions">
            <button type="submit" class="ghost" :disabled="busy || confirmation !== 'DELETE'">{{ text('确认永久删除', 'Confirm permanent deletion') }}</button>
            <button type="button" class="ghost" :disabled="busy" @click="deleting = null">{{ text('取消', 'Cancel') }}</button>
          </div>
        </form>
      </article>
    </div>
  </section>
</template>
<style scoped>
.trash-actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
.trash-path { font-size: 12px; color: var(--color-muted); overflow-wrap: anywhere; margin-top: 8px; }
.trash-confirm { display: grid; gap: 10px; margin-top: 16px; padding: 16px; border: 1px solid var(--color-error-border); border-radius: 10px; font-size: 13px; }
.trash-error { color: var(--color-error-text); overflow-wrap: anywhere; margin-top: 12px; }
section > button { margin-top: 16px; }
</style>
