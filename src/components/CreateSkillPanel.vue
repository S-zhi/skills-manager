<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import type { ManagerStorageInfo } from '../composables/types';

defineProps<{ storage: ManagerStorageInfo | null }>();
const emit = defineEmits<{ (e: 'close'): void; (e: 'created'): void; (e: 'view'): void; (e: 'openDir', path: string): void }>();
const { t } = useI18n();
const dialog = ref<HTMLElement | null>(null);
const name = ref('');
const description = ref('');
const body = ref('');
const busy = ref(false);
const error = ref('');
const result = ref<{ uuid: string; path: string } | null>(null);
const previouslyFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null;
const preview = computed(() => `---\nname: ${name.value.trim() || 'my-skill'}\nuuid: ${t('createSkill.uuidPending')}\ndescription: |-\n${description.value.trim().split('\n').map(line => `  ${line}`).join('\n')}\n---\n\n${body.value}`);
async function create() {
  if (busy.value || result.value) return;
  busy.value = true;
  error.value = '';
  try {
    result.value = await invoke('create_local_skill', { request: { name: name.value, description: description.value, body: body.value } });
    emit('created');
  } catch (err) { error.value = String(err); }
  finally { busy.value = false; }
}
function reset() {
  name.value = ''; description.value = ''; body.value = ''; result.value = null; error.value = '';
}

function close() {
  emit('close');
}

function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault();
    close();
    return;
  }
  if (event.key !== 'Tab' || !dialog.value) return;
  const focusable = Array.from(dialog.value.querySelectorAll<HTMLElement>(
    'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), summary, [tabindex]:not([tabindex="-1"])'
  ));
  if (!focusable.length) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}

onMounted(async () => {
  document.addEventListener('keydown', handleKeydown);
  await nextTick();
  dialog.value?.querySelector<HTMLElement>('.modal-close')?.focus();
});

onBeforeUnmount(() => {
  document.removeEventListener('keydown', handleKeydown);
  previouslyFocused?.focus();
});
</script>

<template>
  <Teleport to="body">
    <div class="create-skill-backdrop">
      <section
        ref="dialog"
        class="create-skill-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="create-skill-title"
        aria-describedby="create-skill-hint"
      >
        <header class="modal-header">
          <div>
            <h2 id="create-skill-title" class="panel-title">{{ t('createSkill.title') }}</h2>
            <p id="create-skill-hint" class="hint">{{ t('createSkill.hint') }}</p>
          </div>
          <button class="modal-close" type="button" :aria-label="t('createSkill.close')" @click="close">×</button>
        </header>
        <div class="modal-body">
          <div v-if="result" class="success-card" role="status">
            <h2>{{ t('createSkill.success') }}</h2>
            <p>{{ name }}</p>
            <p class="path">UUID: {{ result.uuid }}</p>
            <p class="path">{{ result.path }}</p>
            <div class="form-actions">
              <button class="primary" @click="emit('view')">{{ t('createSkill.view') }}</button>
              <button class="ghost" @click="emit('openDir', result.path)">{{ t('local.openDir') }}</button>
              <button class="ghost" @click="reset">{{ t('createSkill.another') }}</button>
            </div>
          </div>
          <form v-else @submit.prevent="create">
            <fieldset :disabled="busy">
              <label>{{ t('createSkill.name') }}
                <input v-model="name" class="input" required maxlength="64" pattern="[a-z0-9]+(-[a-z0-9]+)*" placeholder="my-skill" aria-describedby="skill-name-help" />
              </label>
              <p id="skill-name-help" class="hint">{{ t('createSkill.nameHelp') }}</p>
              <label>{{ t('createSkill.description') }}
                <textarea v-model="description" class="input" required maxlength="1024" rows="3" :placeholder="t('createSkill.descriptionPlaceholder')" />
              </label>
              <label>{{ t('createSkill.body') }}
                <textarea v-model="body" class="input body-editor" required rows="10" :placeholder="t('createSkill.bodyPlaceholder')" />
              </label>
              <p class="hint">{{ t('createSkill.bodyHelp') }}</p>
              <details><summary>{{ t('createSkill.preview') }}</summary><pre>{{ preview }}</pre></details>
              <div class="destination"><span>{{ t('createSkill.destination') }}</span><p class="path">{{ storage?.skillsPath ?? t('discovery.storageLoading') }}{{ name ? ` / ${name.trim()} / SKILL.md` : '' }}</p></div>
              <p v-if="error" class="error" role="alert">{{ error }}</p>
              <div class="form-actions">
                <button class="primary" type="submit" :disabled="!name.trim() || !description.trim() || !body.trim()">{{ busy ? t('local.processing') : t('createSkill.submit') }}</button>
                <span class="hint">{{ t('createSkill.identityHint') }}</span>
              </div>
            </fieldset>
          </form>
        </div>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.create-skill-backdrop { position: fixed; inset: 0; z-index: 1400; display: grid; place-items: center; padding: 28px; background: var(--color-overlay); }
.create-skill-dialog { width: min(720px, 100%); max-height: min(84vh, 820px); overflow: hidden; border: 1px solid var(--color-modal-border); border-radius: 18px; background: var(--color-modal-bg); box-shadow: 0 28px 80px #0005; }
.modal-header { display: flex; align-items: flex-start; justify-content: space-between; gap: 20px; padding: 22px 24px 18px; border-bottom: 1px solid var(--color-panel-border); }
.modal-header .panel-title { margin: 0 0 5px; }
.modal-header .hint { margin: 0; }
.modal-close { display: grid; place-items: center; flex: 0 0 36px; width: 36px; height: 36px; padding: 0; border: 0; border-radius: 9px; background: transparent; color: var(--color-muted); font-size: 24px; line-height: 1; cursor: pointer; }
.modal-close:hover { background: var(--color-tabs-bg); color: var(--color-text); }
.modal-body { max-height: calc(min(84vh, 820px) - 94px); overflow-y: auto; padding: 0 24px 24px; }
fieldset { border: 0; padding: 0; margin-top: 22px; min-width: 0; }
label { display: flex; flex-direction: column; gap: 9px; font-size: 13px; font-weight: 600; margin-top: 22px; }
label:first-child { margin-top: 0; }
.input { width: 100%; font-weight: 400; }
.body-editor { font-family: Consolas, monospace; font-size: 13px; line-height: 1.7; resize: vertical; }
.hint { margin-top: 7px; }
details { margin: 20px 0; font-size: 12px; }
summary { cursor: pointer; color: var(--color-muted); }
pre { white-space: pre-wrap; overflow-wrap: anywhere; background: var(--color-input-bg); padding: 18px; border-radius: 10px; max-height: 350px; overflow: auto; margin-top: 12px; }
.destination { border-top: 1px solid var(--color-panel-border); padding-top: 16px; font-size: 12px; color: var(--color-muted); }
.path { overflow-wrap: anywhere; font-size: 12px; margin-top: 6px; font-weight: 400; }
.form-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; margin-top: 22px; }
.error { margin-top: 15px; color: var(--color-error-text); }
.success-card { margin-top: 30px; padding: 26px; border: 1px solid var(--color-success-border); border-radius: 14px; background: var(--color-success-bg); }
.success-card h2 { font-size: 20px; margin-bottom: 12px; }
@media (max-width: 640px) {
  .create-skill-backdrop { align-items: stretch; padding: 12px; }
  .create-skill-dialog { max-height: calc(100vh - 24px); border-radius: 14px; }
  .modal-header { padding: 18px; }
  .modal-body { max-height: calc(100vh - 112px); padding: 0 18px 18px; }
}
</style>
