<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useI18n } from "vue-i18n";
import { useToast } from "../composables/useToast";

const { locale } = useI18n();
const toast = useToast();
const visible = ref(false);
const maximized = ref(false);
const chinese = computed(() => locale.value.startsWith("zh"));
const maximizeLabel = computed(() => maximized.value
  ? (chinese.value ? "还原窗口" : "Restore")
  : (chinese.value ? "最大化" : "Maximize"));
let disposed = false;
let unlisten: UnlistenFn | undefined;

async function perform(action: "minimize" | "toggleMaximize" | "close") {
  try {
    const window = getCurrentWindow();
    await window[action]();
    if (action === "toggleMaximize") maximized.value = await window.isMaximized();
  } catch (error) {
    console.error("Window action failed", error);
    toast.error(chinese.value ? "窗口操作失败，请重试" : "Window action failed. Please retry.");
  }
}

onMounted(async () => {
  // Browser previews and platforms with native decorations keep their original layout.
  if (!isTauri()) return;
  try {
    const window = getCurrentWindow();
    if (await window.isDecorated() || disposed) return;
    visible.value = true;
    maximized.value = await window.isMaximized();
    const stop = await window.onResized(async () => {
      try { maximized.value = await window.isMaximized(); }
      catch (error) { console.error("Cannot read window state", error); }
    });
    if (disposed) stop();
    else unlisten = stop;
  } catch (error) {
    console.error("Cannot initialize title bar", error);
  }
});
onBeforeUnmount(() => { disposed = true; unlisten?.(); });
</script>

<template>
  <div v-if="visible" class="window-titlebar">
    <div class="window-drag-region" data-tauri-drag-region>
      <span class="window-name">Skill Manager</span>
    </div>
    <div class="window-controls">
      <button type="button" :aria-label="chinese ? '最小化' : 'Minimize'" :title="chinese ? '最小化' : 'Minimize'" @click="perform('minimize')">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8h10" /></svg>
      </button>
      <button type="button" :aria-label="maximizeLabel" :title="maximizeLabel" @click="perform('toggleMaximize')">
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path v-if="maximized" d="M5.5 5.5v-3h8v8h-3m-8-5h8v8h-8z" />
          <rect v-else x="3.5" y="3.5" width="9" height="9" rx=".5" />
        </svg>
      </button>
      <button type="button" class="window-close" :aria-label="chinese ? '关闭' : 'Close'" :title="chinese ? '关闭' : 'Close'" @click="perform('close')">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m3.5 3.5 9 9m0-9-9 9" /></svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.window-titlebar { display: flex; height: 36px; flex-shrink: 0; background: var(--color-bg); border-bottom: 1px solid var(--color-panel-border); user-select: none; }
.window-drag-region { display: flex; align-items: center; flex: 1; min-width: 0; padding-left: 18px; }
.window-name { pointer-events: none; font-size: 12px; font-weight: 500; letter-spacing: .2px; color: var(--color-muted); }
.window-controls { display: flex; }
.window-controls button { display: grid; place-items: center; width: 46px; height: 35px; padding: 0; border: 0; border-radius: 0; background: transparent; color: var(--color-text); }
.window-controls button:hover { background: var(--color-tabs-bg); }
.window-controls button:focus-visible { outline: 2px solid var(--color-input-focus); outline-offset: -3px; }
.window-controls .window-close:hover { background: #c42b1c; color: #fff; }
.window-controls svg { width: 14px; height: 14px; fill: none; stroke: currentColor; stroke-width: 1; }
</style>
