<script setup lang="ts">
import { withBase } from 'vitepress';
import TemplateGallery from './TemplateGallery.vue';

// Each step is the SDK call that does it.
const steps = [
  { call: 'defineTemplate()', text: 'Describe the accounts, inputs, checks and calls.' },
  { call: 'planTemplateUpload()', text: 'Store it on chain once. The program checks it, then locks it.' },
  { call: 'buildRunInstruction()', text: 'Anyone runs it with new inputs, all or nothing.' },
];
</script>

<template>
  <div class="manual">
    <section class="cover" aria-labelledby="cover-title">
      <h1 id="cover-title">Execution, <em>composed.</em></h1>
      <div class="cover-copy">
        <p class="cover-description">
          Write a multi-step Solana transaction once, store it on chain, and let anyone run it again
          with new inputs.
        </p>
        <div class="cover-links">
          <a class="start-link" :href="withBase('/guide/getting-started')">Get started <span aria-hidden="true">↗</span></a>
          <a class="plain-link" :href="withBase('/guide/mental-model')">How it works</a>
        </div>
      </div>
    </section>

    <TemplateGallery />

    <section class="operating-principle" aria-labelledby="principle-title">
      <h2 class="section-label" id="principle-title">How it works</h2>
      <ol class="principle-steps">
        <li v-for="step in steps" :key="step.call" class="principle-step">
          <code>{{ step.call }}</code>
          <p>{{ step.text }}</p>
        </li>
      </ol>
    </section>
  </div>
</template>
