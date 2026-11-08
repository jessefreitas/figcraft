<!-- TECHNICAL TEST FIXTURE — not ERP / customer data -->
<template>
  <div class="technical-root" data-ai-id="technical-button-fixture">
    <button type="button" @click="emitClick">{{ label }}</button>
  </div>
</template>
<script setup lang="ts">
const props = withDefaults(defineProps<{ label?: string; dense?: boolean }>(), { label: "Action", dense: false });
const emit = defineEmits<{ click: [] }>();
function emitClick() {
  emit("click");
}
</script>
<style scoped>
.technical-root {
  display: flex;
  gap: 8px;
}
</style>
