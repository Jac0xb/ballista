<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useData } from 'vitepress';

// A repeating circuit drawn behind every page: an AND, an OR and a NOT gate joined by wires that
// branch at junctions. A signal enters each tile, lights the gates it passes and splits at every
// junction; the wires leave each tile where the next one's begin, so the pulses run across the page.

const { isDark } = useData();
const animated = ref(false);
onMounted(() => {
  animated.value = !window.matchMedia('(prefers-reduced-motion: reduce)').matches;
});

const CYCLE = 4.2;
// Each wire: its path, and when (seconds into the cycle) a pulse enters and leaves it.
const wires = [
  { d: 'M0 40H70', from: 0, to: 0.7 },
  { d: 'M0 130H34V62H70', from: 0, to: 0.95 },
  { d: 'M34 130H96', from: 0.35, to: 0.9 },
  { d: 'M114 50H140V40H260', from: 1.05, to: 2.0 },
  { d: 'M140 50V106H167', from: 1.35, to: 1.9 },
  { d: 'M118 130H167', from: 1.0, to: 1.5 },
  { d: 'M210 118H228V130H260', from: 2.15, to: 2.7 },
  { d: 'M228 118V160H246', from: 2.35, to: 2.8 },
];
const gates = [
  { d: 'M70 24h18a26 26 0 0 1 0 52h-18z', at: 0.95 },
  { d: 'M96 120v20l16-10z', at: 0.9 },
  { d: 'M164 96q11 22 0 44q30 0 46-22q-16-22-46-22z', at: 1.95 },
];
const nodes = [
  [34, 130],
  [140, 50],
  [228, 118],
];

/** One circuit, its pulses shifted by `phase` seconds, placed at (x, y) within the larger tile. */
function circuit(dark: boolean, motion: boolean, phase: number, x: number, y: number) {
  const line = dark ? 'rgba(232,231,223,0.085)' : 'rgba(34,35,31,0.085)';
  const pulse = dark ? '#f0613f' : '#be3b23';
  const key = (seconds: number) => Math.min(1, Math.max(0, seconds / CYCLE)).toFixed(4);
  const begin = `begin="-${phase}s"`;
  const travel = (from: number, to: number) =>
    `<animate attributeName="stroke-dashoffset" values="110;110;-100;-100" keyTimes="0;${key(from)};${key(to)};1" dur="${CYCLE}s" ${begin} repeatCount="indefinite"/>`;
  const light = (at: number) =>
    `<animate attributeName="stroke-opacity" values="0;0;0.35;0;0" keyTimes="0;${key(at - 0.05)};${key(at)};${key(at + 0.6)};1" dur="${CYCLE}s" ${begin} repeatCount="indefinite"/>`;
  const parts = [
    `<g transform="translate(${x} ${y})">`,
    `<g fill="none" stroke="${line}" stroke-width="1" stroke-linejoin="round">`,
    ...wires.map((wire) => `<path d="${wire.d}"/>`),
    ...gates.map((gate) => `<path d="${gate.d}"/>`),
    `<circle cx="115" cy="130" r="3"/><circle cx="249" cy="160" r="3"/>`,
    `</g>`,
    `<g fill="${line}">${nodes.map(([x, y]) => `<circle cx="${x}" cy="${y}" r="2.6"/>`).join('')}</g>`,
  ];
  if (motion) {
    parts.push(
      `<g fill="none" stroke="${pulse}" stroke-opacity="0.5" stroke-width="1.4" stroke-linecap="round">`,
      ...wires.map((wire) => `<path d="${wire.d}" pathLength="100" stroke-dasharray="10 200">${travel(wire.from, wire.to)}</path>`),
      `</g>`,
      `<g fill="none" stroke="${pulse}" stroke-opacity="0" stroke-width="1.1">`,
      ...gates.map((gate) => `<path d="${gate.d}">${light(gate.at)}</path>`),
      `</g>`,
    );
  }
  parts.push('</g>');
  return parts.join('');
}

// Four copies out of step with each other, so neighbouring circuits never pulse together.
function tile(dark: boolean, motion: boolean) {
  const svg = [
    '<svg xmlns="http://www.w3.org/2000/svg" width="520" height="360" viewBox="0 0 520 360">',
    circuit(dark, motion, 0, 0, 0),
    circuit(dark, motion, 1.7, 260, 0),
    circuit(dark, motion, 3.1, 0, 180),
    circuit(dark, motion, 0.9, 260, 180),
    '</svg>',
  ].join('');
  return `url("data:image/svg+xml,${encodeURIComponent(svg)}")`;
}

const image = computed(() => tile(isDark.value, animated.value));
</script>

<template>
  <div class="circuit-background" aria-hidden="true">
    <div class="circuit-sheet" :style="{ backgroundImage: image }" />
  </div>
</template>

<style scoped>
.circuit-background {
  position: fixed;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  overflow: hidden;
  /* Quiet behind the reading column, fuller at the edges. */
  mask-image: linear-gradient(90deg, #000 0%, rgba(0, 0, 0, 0.45) 28%, rgba(0, 0, 0, 0.3) 50%, rgba(0, 0, 0, 0.45) 72%, #000 100%);
}
/* The sheet is drawn small and turned 45° so the wires run down to the right; it overhangs the
   viewport so the turned corners never show. */
.circuit-sheet {
  position: absolute;
  top: 50%;
  left: 50%;
  width: 200vmax;
  height: 200vmax;
  transform: translate(-50%, -50%) rotate(45deg);
  background-repeat: repeat;
  background-size: 312px 216px;
}
</style>
