<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { openUrl } from "@tauri-apps/plugin-opener";
import { i18n, supportedLocales, type SupportedLocale } from "../i18n";
import { useUpdateStore } from "../composables/useUpdateStore";
import { useToast } from "../composables/useToast";
import GithubSyncPanel from "./GithubSyncPanel.vue";
import TranslationSettingsPanel from "./TranslationSettingsPanel.vue";

type SettingsPage = "update" | "backup" | "appearance" | "translation";
type ThemeMode = "light" | "dark" | "system";
const { t, locale: activeLocale } = useI18n();
const text = (zh: string, en: string) => activeLocale.value.startsWith("zh") ? zh : en;
const toast = useToast();
const page = ref<SettingsPage>("update");
const theme = ref<ThemeMode>("system");
const locale = ref<SupportedLocale>("zh-CN");
const themeKey = "skillsManager.theme";
const localeKey = "skillsManager.locale";

const navigation = computed(() => [
  { id: "update" as const, icon: "↻", title: text("检查更新", "Updates"), description: text("版本与应用信息", "Version and app info") },
  { id: "backup" as const, icon: "☁", title: text("云端备份", "Cloud backup"), description: text("GitHub 自动同步", "GitHub sync") },
  { id: "appearance" as const, icon: "◐", title: text("外观设置", "Appearance"), description: text("主题与语言", "Theme and language") },
  { id: "translation" as const, icon: "文", title: text("翻译配置", "Translation"), description: text("翻译 API 与大模型", "APIs and LLMs") },
]);

const { appName, currentVersion, checking, updateAvailable, latestVersion, downloading, downloadProgress, downloaded, upToDate, error, loadAppInfo, checkUpdate, downloadUpdate, installAndRestart, resetState } = useUpdateStore();

async function handleCheckUpdate() { await checkUpdate(); if (error.value) toast.error(error.value); else if (upToDate.value) toast.info(t("settings.update.upToDate")); }
async function handleDownloadUpdate() { await downloadUpdate(); if (error.value) toast.error(error.value); }
async function handleInstallAndRestart() { await installAndRestart(); if (error.value) toast.error(error.value); }
function openGitHub() { void openUrl("https://github.com/S-zhi/skills-manager"); }
function applyTheme(mode: ThemeMode) { const effective = mode === "system" ? (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light") : mode; document.documentElement.setAttribute("data-theme", effective); }
function loadTheme(): ThemeMode { const stored = localStorage.getItem(themeKey); return stored === "dark" || stored === "light" || stored === "system" ? stored : "system"; }
function loadLocale(): SupportedLocale { const stored = localStorage.getItem(localeKey) as SupportedLocale | null; if (stored && supportedLocales.includes(stored)) return stored; return navigator.language.startsWith("zh") ? "zh-CN" : "en-US"; }

watch(theme, next => { applyTheme(next); localStorage.setItem(themeKey, next); });
watch(locale, next => { i18n.global.locale.value = next; localStorage.setItem(localeKey, next); });
onMounted(async () => {
  await loadAppInfo(); theme.value = loadTheme(); locale.value = loadLocale(); i18n.global.locale.value = locale.value; applyTheme(theme.value); resetState();
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => { if (theme.value === "system") applyTheme("system"); });
});
</script>

<template>
  <div class="settings-shell">
    <aside class="settings-nav" aria-label="Settings sections">
      <div class="nav-heading"><span>{{ text("偏好设置", "PREFERENCES") }}</span><strong>{{ text("设置", "Settings") }}</strong></div>
      <button v-for="item in navigation" :key="item.id" class="nav-item" :class="{ active: page === item.id }" :aria-current="page === item.id ? 'page' : undefined" @click="page = item.id">
        <span class="nav-icon" aria-hidden="true">{{ item.icon }}</span><span class="nav-copy"><strong>{{ item.title }}</strong><small>{{ item.description }}</small></span><span class="chevron">›</span>
      </button>
      <div class="nav-footer"><span class="app-dot"></span><span><strong>Skill Manager</strong><small>v{{ currentVersion }}</small></span></div>
    </aside>

    <main class="settings-content">
      <template v-if="page === 'update'">
        <header class="page-header"><div><h1>{{ text("检查更新", "Updates") }}</h1><p>{{ text("查看当前版本并从项目仓库获取最新版本。", "Check your version and get releases from the project repository.") }}</p></div></header>
        <section class="settings-card hero-card">
          <div class="app-mark">SM</div><div class="app-info"><span class="eyebrow">DESKTOP APP</span><h2>{{ appName }}</h2><p>{{ text("当前版本", "Current version") }} <b>v{{ currentVersion }}</b></p></div><button class="ghost" @click="openGitHub">GitHub ↗</button>
        </section>
        <section class="settings-card">
          <div class="section-row"><div><h2>{{ text("软件更新", "Software update") }}</h2><p>{{ text("更新来源：github.com/S-zhi/skills-manager", "Source: github.com/S-zhi/skills-manager") }}</p></div><button class="primary" :disabled="checking || downloading" @click="handleCheckUpdate">{{ checking ? t("settings.update.checking") : t("settings.about.checkUpdate") }}</button></div>
          <div v-if="updateAvailable && !downloaded" class="status-box emphasis"><span>{{ t("settings.update.newVersionAvailable", { version: latestVersion }) }}</span><button class="primary" :disabled="downloading" @click="handleDownloadUpdate">{{ t("settings.update.downloadAndInstall") }}</button></div>
          <div v-if="downloading" class="status-box"><span>{{ t("settings.update.downloading") }} {{ downloadProgress }}%</span><div class="progress"><span :style="{ width: downloadProgress + '%' }"></span></div></div>
          <div v-if="downloaded" class="status-box emphasis"><span>{{ t("settings.update.installAndRestart") }}</span><button class="primary" @click="handleInstallAndRestart">{{ t("settings.update.installAndRestart") }}</button></div>
          <div v-if="upToDate" class="status-box success">✓ {{ t("settings.update.upToDate") }}</div>
        </section>
      </template>

      <template v-else-if="page === 'backup'">
        <header class="page-header"><div><h1>{{ text("云端备份", "Cloud backup") }}</h1><p>{{ text("绑定 GitHub 仓库，备份统一管理的 Skills 与包配置。", "Connect a GitHub repository to back up managed skills and package settings.") }}</p></div></header>
        <GithubSyncPanel />
      </template>

      <template v-else-if="page === 'appearance'">
        <header class="page-header"><div><h1>{{ text("外观设置", "Appearance") }}</h1><p>{{ text("调整 Skill Manager 的显示主题与界面语言。", "Choose how Skill Manager looks and reads.") }}</p></div></header>
        <section class="settings-card"><h2>{{ t("settings.appearance.theme") }}</h2><div class="option-grid"><button v-for="mode in (['light','dark','system'] as ThemeMode[])" :key="mode" class="option-card" :class="{ active: theme === mode }" @click="theme = mode"><span class="theme-preview" :class="mode"><i></i><i></i><i></i></span><strong>{{ t(`settings.appearance.${mode}`) }}</strong><small>{{ mode === 'system' ? text('跟随 Windows 设置', 'Follow Windows settings') : text('固定显示模式', 'Fixed display mode') }}</small></button></div></section>
        <section class="settings-card"><h2>{{ t("settings.appearance.language") }}</h2><div class="language-row"><button :class="{ active: locale === 'zh-CN' }" @click="locale = 'zh-CN'"><strong>中文</strong><small>简体中文</small></button><button :class="{ active: locale === 'en-US' }" @click="locale = 'en-US'"><strong>English</strong><small>English (US)</small></button></div></section>
      </template>

      <TranslationSettingsPanel v-else />
    </main>
  </div>
</template>

<style scoped>
.settings-shell{display:grid;grid-template-columns:220px minmax(0,1fr);flex:1;min-height:0;overflow:hidden;background:var(--color-bg)}.settings-nav{display:flex;flex-direction:column;gap:6px;padding:22px 14px 14px;border-right:1px solid var(--color-panel-border);background:color-mix(in srgb,var(--color-panel-bg) 90%,var(--color-primary-bg) 10%)}.nav-heading{display:flex;flex-direction:column;gap:4px;padding:0 10px 16px}.nav-heading span,.eyebrow{font-size:10px;letter-spacing:.13em;color:var(--color-accent);font-weight:700}.nav-heading strong{font-size:19px;color:var(--color-text)}.nav-item{display:grid;grid-template-columns:34px 1fr 12px;gap:9px;align-items:center;width:100%;padding:10px;border:1px solid transparent;border-radius:12px;background:transparent;color:var(--color-muted);text-align:left;cursor:pointer}.nav-item:hover{background:var(--color-card-bg);color:var(--color-text)}.nav-item.active{border-color:var(--color-chip-border);background:var(--color-tab-active-bg);color:var(--color-tab-active-text);box-shadow:0 5px 16px var(--color-panel-shadow)}.nav-icon{display:grid;place-items:center;width:32px;height:32px;border-radius:9px;background:var(--color-chip-bg);font-size:16px}.nav-item.active .nav-icon{background:color-mix(in srgb,var(--color-primary-bg) 22%,transparent)}.nav-copy{display:flex;flex-direction:column;gap:2px;min-width:0}.nav-copy strong{font-size:13px}.nav-copy small{font-size:10px;opacity:.72;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.chevron{font-size:18px;opacity:.55}.nav-footer{display:flex;align-items:center;gap:9px;margin-top:auto;padding:12px 10px;border-top:1px solid var(--color-panel-border);color:var(--color-text)}.nav-footer>span:last-child{display:flex;flex-direction:column;font-size:11px}.nav-footer small{color:var(--color-muted)}.app-dot{width:9px;height:9px;border-radius:50%;background:var(--color-primary-bg);box-shadow:0 0 0 4px color-mix(in srgb,var(--color-primary-bg) 16%,transparent)}.settings-content{min-width:0;overflow:auto;padding:26px clamp(20px,4vw,44px);display:flex;flex-direction:column;gap:16px}.page-header h1{margin:0;color:var(--color-text);font-size:23px}.page-header p{margin:7px 0 0;color:var(--color-muted);font-size:13px}.settings-card{background:var(--color-panel-bg);border:1px solid var(--color-panel-border);border-radius:16px;padding:20px;box-shadow:0 4px 16px var(--color-panel-shadow)}.settings-card h2{margin:0 0 16px;font-size:15px;color:var(--color-text)}.hero-card{display:flex;align-items:center;gap:15px;background:linear-gradient(135deg,color-mix(in srgb,var(--color-panel-bg) 90%,var(--color-primary-bg) 10%),var(--color-panel-bg))}.app-mark{display:grid;place-items:center;width:52px;height:52px;border-radius:15px;background:var(--color-primary-bg);color:white;font-weight:800;box-shadow:0 8px 20px color-mix(in srgb,var(--color-primary-bg) 28%,transparent)}.app-info{flex:1}.app-info h2{margin:3px 0;font-size:17px}.app-info p,.section-row p{margin:0;color:var(--color-muted);font-size:12px}.section-row{display:flex;align-items:center;justify-content:space-between;gap:16px}.section-row h2{margin-bottom:5px}.primary,.ghost{border-radius:10px;padding:9px 14px;font-size:12px;font-weight:600;cursor:pointer}.primary{border:0;background:var(--color-primary-bg);color:var(--color-primary-text)}.ghost{border:1px solid var(--color-ghost-border);background:transparent;color:var(--color-ghost-text)}button:disabled{cursor:not-allowed;opacity:.55}.status-box{display:flex;justify-content:space-between;align-items:center;gap:12px;margin-top:16px;padding:12px;border:1px solid var(--color-card-border);border-radius:11px;background:var(--color-card-bg);color:var(--color-muted);font-size:13px}.status-box.emphasis{border-color:var(--color-chip-border)}.status-box.success{color:var(--color-success-text)}.progress{flex:1;height:7px;border-radius:99px;background:var(--color-progress-bg);overflow:hidden}.progress span{display:block;height:100%;background:var(--color-primary-bg)}.option-grid{display:grid;grid-template-columns:repeat(3,1fr);gap:12px}.option-card,.language-row button{display:flex;flex-direction:column;gap:5px;border:1px solid var(--color-card-border);background:var(--color-card-bg);color:var(--color-text);border-radius:12px;padding:12px;text-align:left;cursor:pointer}.option-card.active,.language-row button.active{border-color:var(--color-primary-bg);box-shadow:0 0 0 2px color-mix(in srgb,var(--color-primary-bg) 14%,transparent)}.option-card small,.language-row small{color:var(--color-muted);font-size:10px}.theme-preview{display:flex;gap:4px;height:38px;border:1px solid var(--color-card-border);border-radius:8px;padding:7px;background:#fff}.theme-preview.dark{background:#25232b}.theme-preview.system{background:linear-gradient(105deg,#fff 50%,#25232b 50%)}.theme-preview i{width:8px;border-radius:3px;background:#ddd}.theme-preview i:nth-child(2){flex:1;background:#7656c9}.language-row{display:grid;grid-template-columns:1fr 1fr;gap:12px}.language-row button{padding:14px}.settings-content :deep(.github-sync){margin:0}.settings-content :deep(.github-sync h2){margin-top:0}
@media(max-width:760px){.settings-shell{grid-template-columns:1fr;grid-template-rows:auto minmax(0,1fr)}.settings-nav{flex-direction:row;overflow-x:auto;border-right:0;border-bottom:1px solid var(--color-panel-border);padding:10px}.nav-heading,.nav-footer,.nav-copy small,.chevron{display:none}.nav-item{display:flex;width:auto;min-width:max-content;padding:7px 10px}.nav-icon{width:26px;height:26px}.settings-content{padding:18px}.option-grid{grid-template-columns:1fr}.section-row,.status-box{align-items:flex-start;flex-direction:column}}
</style>
