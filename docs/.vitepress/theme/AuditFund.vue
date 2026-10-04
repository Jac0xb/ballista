<script setup lang="ts">
import { computed, ref } from 'vue';
import { useData } from 'vitepress';

// The audit donation address, from BALLISTA_AUDIT_FUND at build time. Renders nothing when unset.
const { theme } = useData();
const address = computed(() => (theme.value as { auditFund?: string }).auditFund);
const copied = ref(false);

async function copy() {
  if (!address.value) return;
  try {
    await navigator.clipboard.writeText(address.value);
    copied.value = true;
    setTimeout(() => (copied.value = false), 1500);
  } catch {
    // Clipboard blocked; the address is still on screen to select.
  }
}
</script>

<template>
  <span v-if="address" class="audit-fund">
    Help fund an audit: send SOL or USDC to
    <code class="audit-fund-address">{{ address }}</code>
    <button type="button" class="audit-fund-copy" @click="copy">{{ copied ? 'Copied' : 'Copy' }}</button>
    <a :href="`https://explorer.solana.com/address/${address}`" target="_blank" rel="noopener">Explorer</a>
  </span>
</template>

<style scoped>
.audit-fund {
  display: inline;
}
.audit-fund-address {
  word-break: break-all;
}
.audit-fund-copy {
  margin: 0 6px;
  padding: 0 6px;
  border: 1px solid currentColor;
  font-size: 0.85em;
  cursor: pointer;
}
</style>
