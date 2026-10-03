<script setup lang="ts">
// The finalization checks on the Trust model page, drawn as the gates an upload passes through
// before the template becomes immutable. Each gate shows its full rule, and folds away on a click.

const gates = [
  {
    title: 'Sound bytes',
    short: 'Complete, hash-matched, well formed',
    detail:
      'The upload is complete and matches the hash recorded when it began, and the bytes are well formed: a known format and version, no unknown instructions, and no reserved bits or fields set.',
  },
  {
    title: 'Typed values',
    short: 'Set before read, right type',
    detail:
      'Each value is set before it is read and has the type each instruction expects. A call’s return data is read only straight after that call, and only if the call always runs.',
  },
  {
    title: 'Declared accounts',
    short: 'Every reference is declared',
    detail:
      'Each account reference points at a declared account, and row accounts and row inputs appear only inside a row loop.',
  },
  {
    title: 'Privileges',
    short: 'Calls stay within declarations',
    detail:
      'No call passes a declared account as a signer or as writable unless its declaration requires that privilege, and every program the template calls, or derives a PDA with, is declared executable.',
  },
  {
    title: 'Bounded reads',
    short: 'Every read stays in bounds',
    detail:
      'Each fixed-offset read stays within the account’s declared minimum length, introspection reads only the Instructions sysvar, pinned to its address, and a group filter names its programs and compares only bytes its minimum length covers.',
  },
  {
    title: 'Bounded work',
    short: 'No nested loops, fixed maximums',
    detail:
      'Loops are never nested, and each has a fixed maximum, so even the worst case stays within every program limit: inputs, registers, instructions, accounts, loops, calls, call data, seeds, output and registries.',
  },
  {
    title: 'Registries and output',
    short: 'Opened first, tagged, set once',
    detail:
      'A registry entry is opened at the top level before it is used, read and written only through its declared fields, and never passed writable to a call. Return data is set at most once, after the last call, and every emit starts with a literal tag.',
  },
];
</script>

<template>
  <figure class="gates" aria-label="The checks a template passes before it becomes immutable">
    <div class="gates-end gates-start">
      <span class="gates-glyph" aria-hidden="true">⇡</span>
      <strong>Upload</strong>
      <span>the template’s bytes</span>
    </div>
    <ol class="gates-list">
      <li v-for="(gate, index) in gates" :key="gate.title" class="gate">
        <details open>
          <summary>
            <span class="gate-number">{{ String(index + 1).padStart(2, '0') }}</span>
            <span class="gate-title">{{ gate.title }}</span>
            <span class="gate-short">{{ gate.short }}</span>
          </summary>
          <p>{{ gate.detail }}</p>
        </details>
      </li>
    </ol>
    <div class="gates-end gates-locked">
      <span class="gates-glyph" aria-hidden="true">🔒</span>
      <strong>Finalized</strong>
      <span>immutable: it can never change</span>
    </div>
    <figcaption>Fail any gate and the template is never finalized. Click a gate to fold it away.</figcaption>
  </figure>
</template>

<style scoped>
.gates {
  margin: 24px 0;
  display: grid;
  gap: 10px;
  font-family: var(--vp-font-family-mono);
}
.gates-end {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 10px 14px;
  border: 1px solid var(--ink);
  font-size: 13px;
}
.gates-end span:last-child {
  color: var(--vp-c-text-2);
}
.gates-locked {
  background: var(--ink);
  color: var(--paper);
}
.gates-locked span:last-child {
  color: var(--faint);
}
.gates-glyph {
  font-size: 15px;
}
.gates-list {
  list-style: none;
  margin: 0;
  padding: 0 0 0 22px;
  border-left: 2px dashed var(--signal);
  margin-left: 18px;
  display: grid;
  gap: 8px;
}
.gates .gate {
  position: relative;
  margin: 0;
  line-height: 1.4;
}
/* Each gate hangs off the dashed line like a checkpoint. */
.gate::before {
  content: '';
  position: absolute;
  left: -29px;
  top: 14px;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: var(--vp-c-bg);
  border: 2px solid var(--signal);
}
.gate details {
  margin: 0;
  padding: 0;
  border: 1px solid var(--rule);
  background: var(--vp-c-bg-soft);
  transition: border-color 0.15s;
}
.gate details[open],
.gate details:hover {
  border-color: var(--signal);
}
.gate summary {
  display: grid;
  grid-template-columns: auto auto 1fr;
  align-items: baseline;
  gap: 12px;
  padding: 9px 12px;
  cursor: pointer;
  margin: 0;
  list-style: none;
  font-size: 13px;
  line-height: 1.4;
}
.gate summary::-webkit-details-marker {
  display: none;
}
.gate-number {
  color: var(--signal);
  font-weight: 600;
}
.gate-title {
  font-weight: 600;
  color: var(--vp-c-text-1);
}
.gate-short {
  color: var(--vp-c-text-2);
  text-align: right;
}
.gate details p {
  margin: 0;
  padding: 0 12px 12px 46px;
  font-family: var(--vp-font-family-base);
  font-size: 14px;
  line-height: 1.6;
  color: var(--vp-c-text-1);
}
figcaption {
  font-size: 12px;
  color: var(--vp-c-text-2);
}
@media (max-width: 640px) {
  .gate summary {
    grid-template-columns: auto 1fr;
  }
  .gate-short {
    grid-column: 2;
    text-align: left;
  }
  .gate details p {
    padding-left: 12px;
  }
}
</style>
