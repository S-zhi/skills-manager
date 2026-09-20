<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { RemoteSkill, DownloadTask } from "../composables/types";
import ManualAddSkillModal from "./ManualAddSkillModal.vue";

const { t, locale } = useI18n();

const props = defineProps<{
  query: string;
  marketSource: "cached" | "skillsmp";
  marketError: string;
  dailyRemaining: number | null;
  loading: boolean;
  results: RemoteSkill[];
  hasMore: boolean;
  installingId: string | null;
  updatingId: string | null;
  localSkillSourceSet: Set<string>;
  downloadQueue: DownloadTask[];
  recentTaskStatus: Record<string, "download" | "update">;
}>();

const downloadingIds = computed(() => new Set(props.downloadQueue.map((task) => task.id)));
const actionState = (skill: RemoteSkill) => props.recentTaskStatus[skill.id] ?? null;
const isInstalled = (skill: RemoteSkill) =>
  props.localSkillSourceSet.has(skill.sourceUrl.trim().toLowerCase());
const text = (zh: string, en: string) => locale.value === "zh-CN" ? zh : en;
const selected = ref<string[]>([]);
const selectable = computed(() => props.results.filter(skill => !!skill.sourceUrl && !isInstalled(skill) && !downloadingIds.value.has(skill.id) && !actionState(skill)));
const selectedSkills = computed(() => selectable.value.filter(skill => selected.value.includes(skill.id)));
const failedTasks = computed(() => props.downloadQueue.filter(task => task.status === "error"));
watch(() => props.results, () => { selected.value = []; });
watch(() => props.marketSource, () => { selected.value = []; });
function downloadSelected() {
  for (const skill of selectedSkills.value) emit("download", skill);
  selected.value = [];
}

const emit = defineEmits<{
  (e: "source", value: "cached" | "skillsmp"): void;
  (e: "retry", id: string): void;
  (e: "update:query", value: string): void;
  (e: "search"): void;
  (e: "refresh"): void;
  (e: "loadMore"): void;
  (e: "download", skill: RemoteSkill): void;
  (e: "update", skill: RemoteSkill): void;
  (e: "manualAdd", payload: { sourceUrl: string; name: string }): void;
}>();

const showManualAdd = ref(false);

async function openSource(skill: RemoteSkill) {
  if (!skill.sourceUrl?.trim()) return;
  await openUrl(skill.sourceUrl.trim());
}
</script>

<template>
  <section class="panel">
    <div class="panel-header-row">
      <div class="panel-title">{{ t("market.title") }}</div>
    </div>

    <div class="market-sources" role="group" :aria-label="text('搜索来源', 'Search source')">
      <button type="button" class="ghost" :class="{ 'source-active': marketSource === 'cached' }" :aria-pressed="marketSource === 'cached'" :disabled="loading" @click="$emit('source', 'cached')">{{ text('内置目录', 'Built-in directory') }}</button>
      <button type="button" class="ghost" :class="{ 'source-active': marketSource === 'skillsmp' }" :aria-pressed="marketSource === 'skillsmp'" :disabled="loading" @click="$emit('source', 'skillsmp')">SkillsMP · {{ text('在线搜索', 'Online') }}</button>
    </div>
    <p class="hint source-hint" v-if="marketSource === 'skillsmp'">
      {{ text('无需登录。关键词会发送至 SkillsMP；匿名配额通常为 50 次/天、10 次/分钟。同一搜索缓存 10 分钟，“刷新”会重新请求。', 'No sign-in required. Keywords are sent to SkillsMP. Anonymous quota is normally 50/day, 10/min. Searches are cached for 10 minutes; Refresh requests fresh results.') }}
      <span v-if="dailyRemaining !== null">{{ text('最近请求返回的今日剩余次数：', 'Daily requests remaining at last response: ') }}{{ dailyRemaining }}</span>
    </p>
    <p class="hint source-hint" v-else>{{ text('搜索应用内置索引；刷新不会从网络更新目录。', 'Search the bundled index; refreshing does not update it from the internet.') }}</p>

    <div class="search-row">
      <input
        :value="query"
        class="input"
        :placeholder="t('market.searchPlaceholder')"
        :aria-label="t('market.searchPlaceholder')"
        :maxlength="marketSource === 'skillsmp' ? 200 : undefined"
        :disabled="loading"
        @input="$emit('update:query', ($event.target as HTMLInputElement).value)"
        @keydown.enter.prevent="$emit('search')"
      />
      <button class="primary" :disabled="loading" @click="$emit('search')">
        {{ loading ? t("market.searching") : t("market.search") }}
      </button>
      <button class="ghost" :disabled="loading" @click="$emit('refresh')">
        {{ loading ? t("market.refreshing") : t("market.refresh") }}
      </button>
      <button class="ghost" :disabled="loading" @click="showManualAdd = true">
        {{ t("market.manualAdd") }}
      </button>
    </div>
    <p v-if="marketError" class="market-error" role="alert">{{ marketError }}</p>
    <p v-if="marketSource === 'skillsmp'" class="hint source-hint">{{ text('下载进入“我的 Skills”，不执行外部脚本。请核查来源和许可证；大型 GitHub 仓库可能超出下载限制。', 'Downloads go to My Skills without executing scripts. Review the source and license; large GitHub repositories may exceed download limits.') }}</p>
  </section>

  <section class="panel">
    <div class="panel-title">{{ t("market.resultsTitle") }}</div>
    <div v-if="results.length" class="market-selection">
      <button type="button" class="ghost" :disabled="loading || !selectable.length" @click="selected = selectable.map(skill => skill.id)">{{ text('全选当前结果', 'Select loaded results') }}</button>
      <button type="button" class="ghost" :disabled="!selected.length" @click="selected = []">{{ text('取消选择', 'Clear selection') }}</button>
      <button type="button" class="primary" :disabled="loading || !selectedSkills.length" @click="downloadSelected">{{ text('下载所选', 'Download selected') }} ({{ selectedSkills.length }})</button>
    </div>
    <div v-for="task in failedTasks" :key="task.id" class="market-error" role="status">
      {{ task.name }}: {{ task.error }}
      <button type="button" class="ghost" @click="$emit('retry', task.id)">{{ text('重试下载', 'Retry download') }}</button>
    </div>
    <div v-if="loading && results.length === 0" class="hint">{{ t("market.loadingHint") }}</div>
    <div v-if="results.length === 0 && !loading && !marketError" class="hint">{{ marketSource === 'skillsmp' && !query.trim() ? text('输入关键词，开始搜索 SkillsMP。', 'Enter a keyword to search SkillsMP.') : t("market.emptyHint") }}</div>

    <div class="cards market-cards">
      <article v-for="skill in results" :key="skill.id" class="card">
        <div class="card-header">
          <input v-model="selected" type="checkbox" :value="skill.id" :disabled="loading || !selectable.some(item => item.id === skill.id)" :aria-label="text('选择 ', 'Select ') + skill.name" />
          <div>
            <div class="card-title">{{ skill.name }}</div>
            <div class="card-meta">
              {{ t("market.meta", { author: skill.author }) }}
            </div>
          </div>
          <template v-if="isInstalled(skill)">
            <button
              class="ghost"
              :disabled="downloadingIds.has(skill.id) || actionState(skill) === 'update' || !skill.sourceUrl || !skill.sourceUrl.trim()"
              :title="(!skill.sourceUrl || !skill.sourceUrl.trim()) ? t('market.unavailable') : ''"
              @click="$emit('update', skill)"
            >
              {{
                (!skill.sourceUrl || !skill.sourceUrl.trim())
                  ? t("market.unavailable")
                  : downloadingIds.has(skill.id)
                    ? t("market.queued")
                    : actionState(skill) === "update"
                      ? t("market.updated")
                      : t("market.update")
              }}
            </button>
          </template>
          <template v-else>
            <button
              class="primary"
              :disabled="downloadingIds.has(skill.id) || actionState(skill) === 'download' || !skill.sourceUrl || !skill.sourceUrl.trim()"
              :title="(!skill.sourceUrl || !skill.sourceUrl.trim()) ? t('market.unavailable') : ''"
              @click="$emit('download', skill)"
            >
              {{
                (!skill.sourceUrl || !skill.sourceUrl.trim())
                  ? t("market.unavailable")
                  : downloadingIds.has(skill.id)
                    ? t("market.queued")
                    : actionState(skill) === "download"
                      ? t("market.downloaded")
                      : t("market.download")
              }}
            </button>
          </template>
        </div>
        <p class="card-desc">{{ locale === 'zh-CN' && skill.descriptionZh ? skill.descriptionZh : skill.description }}</p>
        <div class="card-source">{{ t("market.source", { source: skill.marketLabel }) }}</div>
        <div v-if="skill.marketId === 'skillsmp'" class="hint">GitHub Stars: {{ skill.stars.toLocaleString() }}</div>
        <div class="card-link">{{ skill.sourceUrl }}</div>
        <div class="card-actions market-card-actions">
          <button
            class="ghost"
            :disabled="!skill.sourceUrl || !skill.sourceUrl.trim()"
            @click="openSource(skill)"
          >
            {{ t("market.viewSource") }}
          </button>
        </div>
      </article>
    </div>

    <div v-if="hasMore" class="more">
      <button class="ghost" :disabled="loading" @click="$emit('loadMore')">
        {{ t("market.loadMore") }}
      </button>
    </div>
  </section>

  <ManualAddSkillModal
    :show="showManualAdd"
    @close="showManualAdd = false"
    @submit="$emit('manualAdd', $event)"
  />
</template>

<style scoped>
.market-sources, .market-selection { display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 14px; }
.market-sources .source-active { background: var(--color-tabs-bg); border-color: var(--color-input-focus); color: var(--color-text); }
.source-hint { margin: 10px 0 16px; line-height: 1.7; }
.source-hint span { display: block; }
.market-error { color: var(--color-error-text); background: var(--color-error-bg); padding: 12px; border-radius: 8px; margin: 12px 0; font-size: 12px; overflow-wrap: anywhere; }
.market-error button { margin-left: 10px; }
.card-header > input { margin-top: 5px; flex-shrink: 0; }
.panel-header-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}

.icon-btn {
  padding: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.market-card-actions {
  margin-top: 12px;
  gap: 8px;
  flex-wrap: wrap;
}
</style>
