<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";

interface SyncView {
  config: { repository: string; branch: string; automatic: boolean; lastSuccess: number | null };
  status: { busy: boolean; message: string; error: string | null; files: number };
}
const { locale } = useI18n();
const text = (zh: string, en: string) => locale.value.startsWith("zh") ? zh : en;
const view = ref<SyncView | null>(null);
const repository = ref("");
const branch = ref("main");
const automatic = ref(false);
const acknowledged = ref(false);
const working = ref(false);
const error = ref("");
const connection = ref("");
const loading = ref(true);
let timer: ReturnType<typeof setInterval> | undefined;
let disposed = false;
const busy = computed(() => working.value || !!view.value?.status.busy || loading.value);
const dirty = computed(() => repository.value !== (view.value?.config.repository || "") || branch.value !== (view.value?.config.branch || "main") || automatic.value !== !!view.value?.config.automatic);
const lastSync = computed(() => view.value?.config.lastSuccess ? new Date(view.value.config.lastSuccess * 1000).toLocaleString(locale.value) : text("尚未同步", "Never synced"));

function apply(result: SyncView) {
  view.value = result;
  repository.value = result.config.repository;
  branch.value = result.config.branch || "main";
  automatic.value = result.config.automatic;
  acknowledged.value = false;
}
async function refresh(initial = false) {
  try {
    if (!isTauri()) throw new Error(text("请在桌面应用中使用 GitHub 同步", "Open the desktop app to use GitHub sync"));
    const result = await invoke<SyncView>("github_sync_status");
    if (disposed) return;
    if (initial) apply(result);
    else view.value = result;
  } catch (e) { error.value = String(e); }
  finally { loading.value = false; }
}
async function save(unbind = false) {
  working.value = true; error.value = ""; connection.value = "";
  try {
    apply(await invoke<SyncView>("save_github_sync", { request: {
      repository: unbind ? "" : repository.value,
      branch: branch.value,
      automatic: unbind ? false : automatic.value,
    } }));
  } catch (e) { error.value = String(e); }
  finally { working.value = false; }
}
async function sync() {
  working.value = true; error.value = "";
  try { view.value = await invoke<SyncView>("sync_github_now"); }
  catch (e) { error.value = String(e); }
  finally { working.value = false; }
}
async function testConnection() {
  working.value = true; error.value = ""; connection.value = "";
  try {
    const result = await invoke<{ private: boolean }>("test_github_sync", { request: { repository: repository.value, branch: branch.value, automatic: false } });
    connection.value = result.private
      ? text("连接成功：私有仓库，分支存在且拥有写入权限。", "Connected: private repository, branch exists and write access is available.")
      : text("连接成功：公开仓库！上传内容将对所有人可见。", "Connected: PUBLIC repository! Uploaded files will be visible to everyone.");
  } catch (e) { error.value = String(e); }
  finally { working.value = false; }
}
onMounted(async () => {
  await refresh(true);
  if (!disposed && isTauri()) timer = setInterval(() => { if (!working.value) void refresh(); }, 3000);
});
onBeforeUnmount(() => { disposed = true; if (timer) clearInterval(timer); });
</script>

<template>
  <section class="github-sync" aria-labelledby="github-sync-title">
    <div class="sync-heading">
      <div><h2 id="github-sync-title">{{ text('GitHub 云端备份', 'GitHub backup') }}</h2><p>{{ text('将管理库安全备份到你自己的 GitHub 仓库。', 'Back up your managed library to your own GitHub repository.') }}</p></div>
      <span class="sync-badge">{{ view?.config.automatic ? text('自动上传已开启', 'Auto upload on') : text('手动上传', 'Manual upload') }}</span>
    </div>
    <div class="sync-help">
      {{ text('准备：安装 GitHub CLI，在终端执行下方命令登录；创建一个包含 README 的仓库（推荐私有）。', 'Setup: install GitHub CLI, sign in using the command below, then create a repository with a README (private recommended).') }}
      <code>gh auth login --hostname github.com</code>
    </div>
    <fieldset :disabled="busy">
      <div class="sync-fields">
        <label>{{ text('GitHub 仓库', 'GitHub repository') }}<input v-model="repository" class="input" placeholder="owner/my-skills" autocomplete="off" @input="connection = ''; acknowledged = false" /></label>
        <label>{{ text('已有分支', 'Existing branch') }}<input v-model="branch" class="input" placeholder="main" autocomplete="off" @input="connection = ''; acknowledged = false" /></label>
      </div>
      <label class="sync-check"><input v-model="automatic" type="checkbox" />{{ text('应用运行期间，每 5 分钟自动同步上传', 'Automatically upload every 5 minutes while the app is running') }}</label>
      <p class="sync-warning">{{ text('上传范围：Skill Manager/Skills 内的文件和 Skill 包配置 → 仓库 SkillManager/ 目录。本地删除也会同步到该目录；不下载远端内容、不强制覆盖冲突。常见密钥文件会排除，但请自行检查文档、脚本中是否含敏感信息。公开仓库的内容所有人都能看到。', 'Scope: managed Skills and package configuration → SkillManager/ in the repository. Local deletions are mirrored within that folder. No downloads or forced conflict overwrites. Common key files are excluded, but review documents and scripts for secrets yourself. Public repositories expose all uploaded content.') }}</p>
      <label v-if="dirty && repository.trim()" class="sync-check"><input v-model="acknowledged" type="checkbox" />{{ text('我已确认仓库和上传范围；开启自动上传即允许定期上传这些文件。', 'I confirm the destination and upload scope; enabling auto upload permits scheduled uploads of these files.') }}</label>
      <div class="sync-actions">
        <button type="button" class="ghost" :disabled="!repository.trim() || !branch.trim()" @click="testConnection">{{ text('测试连接', 'Test connection') }}</button>
        <button type="button" class="primary" :disabled="!dirty || !repository.trim() || !acknowledged" @click="save()">{{ text('保存绑定', 'Save binding') }}</button>
        <button type="button" class="ghost" :disabled="dirty || !view?.config.repository" @click="sync">{{ text('立即上传', 'Upload now') }}</button>
        <button v-if="view?.config.repository" type="button" class="ghost" @click="save(true)">{{ text('解除绑定', 'Unbind') }}</button>
      </div>
    </fieldset>
    <div class="sync-status" role="status" aria-live="polite">
      <p>{{ text('最后成功同步：', 'Last successful sync: ') }}{{ lastSync }}</p>
      <p v-if="busy">{{ text('正在处理，请稍候…', 'Working, please wait…') }}</p>
      <p v-else-if="view?.status.message">{{ view.status.message }}<span v-if="view.status.files"> · {{ view.status.files }} {{ text('个文件', 'files') }}</span></p>
      <p v-if="connection">{{ connection }}</p>
      <p v-if="error || view?.status.error" class="sync-error">{{ error || view?.status.error }}</p>
    </div>
  </section>
</template>

<style scoped>
.github-sync { padding: 24px; margin-bottom: 24px; border: 1px solid var(--color-panel-border); border-radius: 14px; background: var(--color-panel-bg); }
.sync-heading { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; }
h2 { font-size: 17px; font-weight: 600; margin-bottom: 6px; }
p, .sync-help { font-size: 12px; color: var(--color-muted); line-height: 1.7; }
.sync-badge { font-size: 11px; padding: 4px 9px; white-space: nowrap; background: var(--color-tabs-bg); border-radius: 12px; }
.sync-help { margin: 18px 0; }
code { display: block; margin-top: 8px; padding: 10px 12px; border-radius: 8px; background: var(--color-input-bg); color: var(--color-text); user-select: text; }
fieldset { border: 0; padding: 0; min-width: 0; }
.sync-fields { display: grid; grid-template-columns: 2fr 1fr; gap: 14px; }
.sync-fields label { display: grid; gap: 8px; font-size: 12px; }
.input { width: 100%; color: var(--color-text); background: var(--color-input-bg); border: 1px solid var(--color-input-border); }
.sync-check { display: flex; align-items: flex-start; gap: 8px; margin-top: 16px; font-size: 12px; line-height: 1.6; }
.sync-check input { margin-top: 3px; flex-shrink: 0; }
.sync-warning { margin-top: 16px; }
.sync-actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 18px; }
.sync-status { margin-top: 16px; overflow-wrap: anywhere; }
.sync-error { color: var(--color-error-text); }
@media (max-width: 650px) { .sync-fields { grid-template-columns: 1fr; } .sync-heading { flex-direction: column; } .github-sync { padding: 16px; } }
</style>
