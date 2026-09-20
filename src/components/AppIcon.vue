<script setup lang="ts">
withDefaults(defineProps<{
  name: 'library' | 'package' | 'create' | 'discover' | 'market' | 'editor' | 'folder' | 'settings' | 'trash';
  size?: number;
}>(), { size: 18 });

const paths = {
  trash: ['M3 6h18', 'M9 6V3h6v3', 'm5 6 1 15h12l1-15', 'M10 10v7', 'M14 10v7'],
  library: ['M4 4v16', 'M8 4v16', 'M12 4v16', 'm16 4 4 16', 'M3 7h2', 'M7 17h2', 'M11 7h2'],
  package: ['m12 3 9 5-9 5-9-5 9-5Z', 'M3 8v9l9 5 9-5V8', 'M12 13v9', 'm7.5 5.5 9 5V15'],
  create: ['M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9Z', 'M14 3v6h6', 'M12 12v6', 'M9 15h6'],
  discover: ['M21 11a9 9 0 1 1-8-8', 'm21 3-9 9', 'M16 3h5v5', 'M15 12a3 3 0 1 1-3-3'],
  market: ['M4 9v11h16V9', 'M3 9h18l-2-6H5L3 9Z', 'M8 9V3', 'M16 9V3', 'M9 20v-6h6v6'],
  editor: ['M4 4h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1Z', 'M8 22h8', 'M12 18v4', 'm7 8 3 3-3 3', 'M13 14h4'],
  folder: ['M3 7V5a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z', 'M3 10h18'],
  settings: ['m9 3-1 3-3 1-2 3 2 2-1 3 2 3 3-1 3 2 3-2 3 1 2-3-1-3 2-2-2-3-3-1-1-3H9Z', 'M15 11a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z'],
};
</script>

<template>
  <svg :width="size" :height="size" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false" class="app-icon">
    <path v-for="(path, index) in paths[name]" :key="index" :d="path" />
  </svg>
</template>

<style scoped>
.app-icon { display: inline-block; flex-shrink: 0; vertical-align: middle; }
</style>
