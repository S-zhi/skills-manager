<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
import type { TranslationSettingsView } from "../composables/types";

const { locale } = useI18n();
const text = (zh: string, en: string) => locale.value.startsWith("zh") ? zh : en;
const loading = ref(true);
const saving = ref(false);
const error = ref("");
const success = ref("");
const revision = ref(0);
const configPath = ref("");
const basicConfigured = ref(false);
const advancedConfigured = ref(false);
const basicApiKey = ref("");
const advancedApiKey = ref("");

const basic = reactive({ provider: "azure", endpoint: "https://api.cognitive.microsofttranslator.com", region: "", sourceLanguage: "auto", targetLanguage: "zh-CN" });
const advanced = reactive({ provider: "gemini", baseUrl: "https://generativelanguage.googleapis.com", model: "gemini-2.5-flash", temperature: 0.2, preserveStructure: true });

const endpointLabel = computed(() => basic.provider === "libretranslate" ? text("服务地址", "Service URL") : text("API 地址", "API endpoint"));
const showRegion = computed(() => basic.provider === "azure");

function apply(view: TranslationSettingsView) {
  revision.value = view.revision;
  Object.assign(basic, view.basic);
  Object.assign(advanced, view.advanced);
  basicConfigured.value = view.basicApiKeyConfigured;
  advancedConfigured.value = view.advancedApiKeyConfigured;
  configPath.value = view.configPath;
  basicApiKey.value = "";
  advancedApiKey.value = "";
}

async function load() {
  loading.value = true;
  error.value = "";
  try {
    if (!isTauri()) throw new Error(text("请在 Windows 桌面应用中保存翻译配置。", "Open the Windows desktop app to save translation settings."));
    apply(await invoke<TranslationSettingsView>("get_translation_settings"));
  } catch (e) { error.value = String(e); }
  finally { loading.value = false; }
}

async function save() {
  saving.value = true; error.value = ""; success.value = "";
  try {
    if (!isTauri()) throw new Error(text("浏览器预览不支持保存，请启动 Tauri 桌面应用。", "Browser preview cannot save settings. Start the Tauri desktop app."));
    const request = {
      revision: revision.value,
      basic: { ...basic },
      advanced: { ...advanced, temperature: Number(advanced.temperature) },
      basicApiKey: basicApiKey.value ? basicApiKey.value : undefined,
      advancedApiKey: advancedApiKey.value ? advancedApiKey.value : undefined,
    };
    apply(await invoke<TranslationSettingsView>("save_translation_settings", { request }));
    success.value = text("翻译配置已保存。API Key 仅在本次运行中保留。", "Translation settings saved. API keys are kept for this session only.");
  } catch (e) { error.value = String(e); }
  finally { saving.value = false; }
}

async function clearKey(kind: "basic" | "advanced") {
  try {
    apply(await invoke<TranslationSettingsView>("clear_translation_session_key", { kind }));
    success.value = text("会话密钥已清除。", "Session key cleared.");
  } catch (e) { error.value = String(e); }
}

onMounted(load);
</script>

<template>
  <div class="translation-settings">
    <div class="page-intro">
      <div><h2>{{ text("翻译配置", "Translation") }}</h2><p>{{ text("先配置翻译服务；实际翻译操作将在 Skill 编辑与批处理流程中调用。", "Configure providers now; translation actions will use them from skill editing and batch workflows.") }}</p></div>
      <span class="stage-badge">{{ text("配置接入", "Configuration") }}</span>
    </div>

    <div v-if="loading" class="state-card">{{ text("正在读取配置…", "Loading settings…") }}</div>
    <template v-else>
      <section class="config-card">
        <div class="card-heading"><div><span class="eyebrow">{{ text("基础", "BASIC") }}</span><h3>{{ text("专用翻译 API", "Translation API") }}</h3><p>{{ text("适合稳定、低成本的日常文本翻译。", "For predictable, low-cost everyday translation.") }}</p></div><span class="status" :class="{ ready: basicConfigured }">{{ basicConfigured ? text("密钥已就绪", "Key ready") : text("未配置密钥", "No key") }}</span></div>
        <div class="form-grid">
          <label><span>{{ text("服务商", "Provider") }}</span><select v-model="basic.provider"><option value="azure">Azure Translator</option><option value="deepl">DeepL</option><option value="google">Google Cloud Translation</option><option value="mymemory">MyMemory</option><option value="libretranslate">LibreTranslate</option></select></label>
          <label><span>{{ text("目标语言", "Target language") }}</span><input v-model="basic.targetLanguage" placeholder="zh-CN" /></label>
          <label class="wide"><span>{{ endpointLabel }}</span><input v-model="basic.endpoint" placeholder="https://…" /></label>
          <label v-if="showRegion"><span>{{ text("Azure 区域", "Azure region") }}</span><input v-model="basic.region" placeholder="eastasia" /></label>
          <label><span>{{ text("源语言", "Source language") }}</span><input v-model="basic.sourceLanguage" placeholder="auto" /></label>
          <label class="wide"><span>API Key <small>{{ text("仅本次会话", "session only") }}</small></span><div class="key-row"><input v-model="basicApiKey" type="password" autocomplete="off" :placeholder="basicConfigured ? text('已配置；留空则保持', 'Configured; leave blank to keep') : text('输入密钥', 'Enter API key')" /><button v-if="basicConfigured" class="text-button" @click="clearKey('basic')">{{ text("清除", "Clear") }}</button></div></label>
        </div>
        <div v-if="basic.provider === 'mymemory'" class="notice">{{ text("MyMemory 可匿名体验（公共免费服务通常约 5,000 字符/天），请勿提交私密 Skill 内容。", "MyMemory supports anonymous trials (public free service is typically about 5,000 characters/day). Do not submit private skill content.") }}</div>
      </section>

      <section class="config-card advanced-card">
        <div class="card-heading"><div><span class="eyebrow">{{ text("高级", "ADVANCED") }}</span><h3>{{ text("大模型翻译", "LLM translation") }}</h3><p>{{ text("适合保持上下文、术语与 Markdown 结构。", "For context, terminology, and Markdown-aware translation.") }}</p></div><span class="status" :class="{ ready: advancedConfigured }">{{ advancedConfigured ? text("密钥已就绪", "Key ready") : text("未配置密钥", "No key") }}</span></div>
        <div class="form-grid">
          <label><span>{{ text("服务商", "Provider") }}</span><select v-model="advanced.provider"><option value="gemini">Gemini</option><option value="openai-compatible">OpenAI Compatible</option></select></label>
          <label><span>{{ text("模型", "Model") }}</span><input v-model="advanced.model" placeholder="gemini-2.5-flash" /></label>
          <label class="wide"><span>Base URL</span><input v-model="advanced.baseUrl" placeholder="https://…" /></label>
          <label><span>Temperature</span><input v-model.number="advanced.temperature" type="number" min="0" max="1" step="0.1" /></label>
          <label class="toggle-label"><input v-model="advanced.preserveStructure" type="checkbox" /><span>{{ text("严格保持 Markdown、YAML 与代码结构", "Preserve Markdown, YAML, and code structure") }}</span></label>
          <label class="wide"><span>API Key <small>{{ text("仅本次会话", "session only") }}</small></span><div class="key-row"><input v-model="advancedApiKey" type="password" autocomplete="off" :placeholder="advancedConfigured ? text('已配置；留空则保持', 'Configured; leave blank to keep') : text('输入密钥', 'Enter API key')" /><button v-if="advancedConfigured" class="text-button" @click="clearKey('advanced')">{{ text("清除", "Clear") }}</button></div></label>
        </div>
      </section>

      <div class="security-note"><strong>{{ text("密钥安全", "Key safety") }}</strong><span>{{ text("API Key 不会写入磁盘或回传界面，关闭应用后需重新输入。非敏感配置保存到：", "API keys are never written to disk or returned to the UI and must be entered again after restart. Non-sensitive settings are saved to:") }} {{ configPath || "%USERPROFILE%\\Skill Manager\\.metadata\\translation-settings.json" }}</span></div>
      <p v-if="error" class="message error" role="alert">{{ error }}</p><p v-if="success" class="message success" role="status">{{ success }}</p>
      <div class="actions"><button class="primary" :disabled="saving" @click="save">{{ saving ? text("保存中…", "Saving…") : text("保存翻译配置", "Save translation settings") }}</button></div>
    </template>
  </div>
</template>

<style scoped>
.translation-settings{display:flex;flex-direction:column;gap:16px}.page-intro,.card-heading{display:flex;justify-content:space-between;gap:20px;align-items:flex-start}.page-intro h2,.card-heading h3{margin:0;color:var(--color-text)}.page-intro h2{font-size:22px}.page-intro p,.card-heading p{margin:6px 0 0;color:var(--color-muted);font-size:13px;line-height:1.55}.stage-badge,.status{white-space:nowrap;border:1px solid var(--color-chip-border);background:var(--color-chip-bg);color:var(--color-muted);border-radius:999px;padding:6px 10px;font-size:11px;font-weight:600}.status.ready{color:var(--color-success-text);background:var(--color-success-bg);border-color:var(--color-success-border)}.config-card,.state-card{background:var(--color-panel-bg);border:1px solid var(--color-panel-border);border-radius:16px;padding:20px;box-shadow:0 4px 16px var(--color-panel-shadow)}.advanced-card{border-top-color:var(--color-primary-bg)}.eyebrow{display:block;color:var(--color-accent);font-size:10px;letter-spacing:.12em;font-weight:700;margin-bottom:6px}.form-grid{display:grid;grid-template-columns:1fr 1fr;gap:15px;margin-top:20px}.form-grid label{display:flex;flex-direction:column;gap:7px;color:var(--color-muted);font-size:12px;font-weight:600}.form-grid label.wide{grid-column:1/-1}.form-grid input,.form-grid select{min-width:0;border:1px solid var(--color-input-border);background:var(--color-input-bg);color:var(--color-text);border-radius:10px;padding:10px 12px;font:inherit;font-weight:400;outline:none}.form-grid input:focus,.form-grid select:focus{border-color:var(--color-input-focus);box-shadow:0 0 0 3px color-mix(in srgb,var(--color-primary-bg) 15%,transparent)}small{font-weight:400;color:var(--color-muted)}.key-row{display:flex;gap:8px}.key-row input{flex:1}.text-button{border:1px solid var(--color-ghost-border);background:transparent;color:var(--color-ghost-text);border-radius:9px;padding:0 13px;cursor:pointer}.toggle-label{flex-direction:row!important;align-items:center;align-self:end;min-height:38px}.toggle-label input{width:16px;height:16px;accent-color:var(--color-primary-bg)}.notice,.security-note{border-radius:11px;padding:12px 14px;font-size:12px;line-height:1.55}.notice{margin-top:15px;background:var(--color-chip-bg);color:var(--color-muted);border:1px solid var(--color-chip-border)}.security-note{display:flex;flex-direction:column;gap:4px;background:var(--color-card-bg);color:var(--color-muted);border:1px solid var(--color-card-border);word-break:break-all}.security-note strong{color:var(--color-text)}.actions{display:flex;justify-content:flex-end}.primary{border:0;border-radius:10px;padding:10px 18px;background:var(--color-primary-bg);color:var(--color-primary-text);font-weight:600;cursor:pointer}.primary:disabled{opacity:.6}.message{margin:0;border-radius:9px;padding:10px 13px;font-size:13px}.message.error{color:var(--color-danger-text);background:var(--color-danger-bg)}.message.success{color:var(--color-success-text);background:var(--color-success-bg)}
@media(max-width:680px){.form-grid{grid-template-columns:1fr}.form-grid label.wide{grid-column:auto}.page-intro,.card-heading{flex-direction:column}.stage-badge,.status{align-self:flex-start}}
</style>
