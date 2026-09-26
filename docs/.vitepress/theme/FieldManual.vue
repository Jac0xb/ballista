<script setup lang="ts">
import { ref } from 'vue';
import { withBase } from 'vitepress';

const accountExists = ref(false);
const patterns = [
  { number: '01', name: 'Pay thirty people in one run', category: 'BATCH / SOL', href: '/examples/payments#bounded-sol-payroll' },
  { number: '02', name: 'Create an account only if it is missing', category: 'GUARD / ATA', href: '/examples/token-accounts#conditional-ata-setup' },
  { number: '03', name: 'Put a ceiling on treasury spend', category: 'ASSERT / DELTA', href: '/examples/guardrails#maximum-lamport-spend' },
  { number: '04', name: 'Claim rewards, then distribute them', category: 'COMPOSE / CPI', href: '/examples/composition#claim-then-distribute' },
];
</script>

<template>
  <div class="manual">
    <div class="folio cover-folio">
      <span>A field guide to on-chain execution</span>
      <span class="edition">Solana · v0.3</span>
    </div>

    <section class="cover" aria-labelledby="cover-title">
      <div class="cover-copy">
        <h1 id="cover-title">Execution,<em>composed.</em></h1>
        <p class="cover-description">
          A small machine for complex transactions. Arrange calls, add conditions, and store the
          sequence on chain. Run it again with fresh inputs.
        </p>
        <div class="cover-links">
          <a class="start-link" :href="withBase('/guide/getting-started')">Write your first template <span aria-hidden="true">↗</span></a>
          <a class="plain-link" :href="withBase('/guide/mental-model')">How it works</a>
        </div>
        <p class="cover-note"><span aria-hidden="true">↳</span><span>Open source. Bounded execution.<br>All steps succeed, or the transaction rolls back.</span></p>
      </div>

      <figure class="cover-exhibit">
        <div class="execution-plate">
          <div class="plate-title"><span>Fig. 01</span><span>A condition changes the execution</span></div>
          <div class="plate-control">
            <span>Token account</span>
            <div class="state-switch" role="group" aria-label="Illustrated token account state">
              <button type="button" :aria-pressed="!accountExists" @click="accountExists = false">Empty</button>
              <button type="button" :aria-pressed="accountExists" @click="accountExists = true">Already exists</button>
            </div>
          </div>

          <svg class="flow-drawing" viewBox="0 0 480 296" role="img" aria-labelledby="flow-title flow-description">
            <title id="flow-title">Conditional token account creation</title>
            <desc id="flow-description">{{ accountExists ? 'The account already exists. Ballista skips ordinary Create and completes the run with zero CPIs.' : 'The account is empty. Ballista invokes ordinary Create and completes the run with one CPI.' }}</desc>
            <defs>
              <pattern id="drawing-grid" width="16" height="16" patternUnits="userSpaceOnUse"><circle cx="8" cy="8" r=".6" fill="#d4d4ca" /></pattern>
            </defs>
            <rect x="16" y="16" width="448" height="266" fill="url(#drawing-grid)" />
            <path d="M240 32v26" stroke="#22231f" fill="none" />
            <circle cx="240" cy="30" r="3" fill="#22231f" />
            <text x="254" y="33" class="small-label">RUN</text>
            <path d="M240 58l64 38-64 38-64-38z" stroke="#22231f" fill="#f7f6f2" />
            <text x="240" y="100" text-anchor="middle">isEmpty(ata)</text>

            <g class="branch" :class="{ 'is-active': !accountExists }">
              <path d="M176 96h-64v82" stroke="currentColor" fill="none" />
              <path d="m108 173 4 5 4-5" stroke="currentColor" fill="none" />
              <text x="127" y="87" class="small-label">TRUE</text>
              <rect x="40" y="179" width="144" height="39" />
              <text x="112" y="203" text-anchor="middle">invoke Create</text>
              <path d="M112 218v31h128" stroke="currentColor" fill="none" />
              <text x="44" y="168" class="small-label">ASSOCIATED TOKEN PROGRAM</text>
            </g>
            <g class="branch" :class="{ 'is-active': accountExists }">
              <path d="M304 96h64v82" stroke="currentColor" fill="none" :stroke-dasharray="accountExists ? undefined : '3 3'" />
              <path d="m364 173 4 5 4-5" stroke="currentColor" fill="none" />
              <text x="327" y="87" class="small-label">FALSE</text>
              <rect x="296" y="179" width="144" height="39" />
              <text x="368" y="203" text-anchor="middle">skip</text>
              <path d="M368 218v31H240" stroke="currentColor" fill="none" :stroke-dasharray="accountExists ? undefined : '3 3'" />
              <text x="301" y="168" class="small-label">NO INNER INSTRUCTION</text>
            </g>
            <path d="M240 249v22" stroke="#22231f" />
            <circle cx="240" cy="273" r="3" fill="#22231f" />
            <text x="254" y="276" class="small-label">COMPLETE</text>
            <path d="M16 24v-8h8m432 0h8v8M16 274v8h8m432 0h8v-8" fill="none" stroke="#686960" stroke-width=".7" />
          </svg>

          <div class="plate-readout" aria-live="polite" aria-atomic="true">
            <span>{{ accountExists ? 'Same template. The guard skips Create.' : 'Empty account. The guard permits Create.' }}</span>
            <strong>{{ accountExists ? '0 CPIs' : '1 CPI' }}</strong>
          </div>
        </div>
        <figcaption class="plate-caption"><span>↳ Try both states.</span> Ordinary Create would fail if the account existed. The guard makes the sequence reusable.</figcaption>
      </figure>
    </section>

    <section class="operating-principle" aria-labelledby="principle-title">
      <h2 class="section-label" id="principle-title"><span class="section-number">§ 01</span>Operating principle</h2>
      <div class="principle-steps">
        <div class="principle-step"><h2>Compose</h2><p>Describe accounts, inputs, guards, and program calls in TypeScript.</p></div>
        <div class="principle-step"><h2>Store</h2><p>Compile once. Upload a verified, immutable template to Solana.</p></div>
        <div class="principle-step"><h2>Execute</h2><p>Supply new inputs and accounts. Call from TypeScript or Rust.</p></div>
      </div>
    </section>

    <section class="pattern-section" aria-labelledby="patterns-title">
      <div>
        <p class="section-label"><span class="section-number">§ 02</span>Selected patterns</p>
        <h2 class="pattern-heading" id="patterns-title">A few things<br>you can set in motion.</h2>
      </div>
      <div>
        <div class="pattern-list">
          <a v-for="pattern in patterns" :key="pattern.number" :href="withBase(pattern.href)" class="pattern-link">
            <span class="pattern-number">{{ pattern.number }}</span>
            <span class="pattern-name">{{ pattern.name }}</span>
            <span class="pattern-category">{{ pattern.category }}</span>
            <span class="pattern-arrow" aria-hidden="true">↗</span>
          </a>
        </div>
        <div class="all-patterns"><span>TypeScript + Rust</span><a :href="withBase('/examples/')">All 20 examples ↗</a></div>
      </div>
    </section>

    <aside class="limits-strip" aria-label="Runtime limits">
      <p class="section-label">Finite, by design.<br><a :href="withBase('/reference/limits')">Read the limits ↗</a></p>
      <div class="limit"><strong>10 KiB</strong><span>template payload</span></div>
      <div class="limit"><strong>60</strong><span>runtime accounts</span></div>
      <div class="limit"><strong>128</strong><span>VM instructions</span></div>
      <div class="limit"><strong>64</strong><span>expanded CPIs</span></div>
    </aside>
  </div>
</template>
