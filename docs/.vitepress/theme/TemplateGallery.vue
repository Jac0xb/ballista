<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue';
import { withBase } from 'vitepress';
import { data as facts } from './facts.data';

type Side = 'left' | 'right';
type Point = [number, number];
interface Case {
  label: string;
  branch: Side;
  outcome: string;
  result: string;
}
/** A box or decision diamond in a diagram, in the drawing's coordinates. */
type Shape = { box: [number, number, number, number] } | { diamond: [number, number, number, number] };
/** The path one case takes through its diagram, and the boxes and diamonds it passes. */
interface Route {
  points: Point[];
  nodes: Shape[];
  /** Where the path ends: DONE, a failed check, or back at the top of the loop. */
  end: 'done' | 'fail' | 'loop';
  /** Where the end is marked, when that is not the last point: the FAILS cross. */
  mark?: Point;
}
interface Example {
  name: string;
  title: string;
  href: string;
  code: string;
  cases: Case[];
  route: (branch: Side, index: number) => Route;
}

// The waterfall's rows are the ones its end-to-end test runs: 2,500 lamports to hand out and
// 1,000 owed to each of four creditors, which pays 1,000, 1,000, 500, and nothing.
const waterfallRows = (() => {
  let remaining = 2_500;
  return [1_000, 1_000, 1_000, 1_000].map((owed) => {
    const pay = Math.min(remaining, owed);
    const row = { remaining, owed, pay, after: remaining - pay };
    remaining -= pay;
    return row;
  });
})();

const amount = (value: number) => value.toLocaleString('en-US');

/** Source text written indented in this file, with the common indent removed. */
function source(text: string): string {
  const lines = text.replace(/^\n/, '').replace(/\n\s*$/, '').split('\n');
  const indent = Math.min(...lines.filter((line) => line.trim()).map((line) => line.length - line.trimStart().length));
  return lines.map((line) => line.slice(indent)).join('\n');
}

function outline(shape: Shape): string {
  if ('box' in shape) {
    const [x, y, width, height] = shape.box;
    return `M${x} ${y}h${width}v${height}h${-width}z`;
  }
  const [x, top, halfWidth, halfHeight] = shape.diamond;
  return `M${x} ${top}l${halfWidth} ${halfHeight}l${-halfWidth} ${halfHeight}l${-halfWidth} ${-halfHeight}z`;
}

// Each example is a complete template. The three that call only the System, Token and Associated
// Token programs compile to exactly the template `pnpm benchmarks` measures under that name. The
// two that call another protocol take its details as parameters, and compile to the measured
// template when given the benchmark's stand-in: the System Program and a transfer's data.
const examples: Example[] = [
  {
    name: "Create a token account only if it's missing",
    title: 'A check decides whether a program is called',
    href: '/examples/token-accounts#conditional-ata-setup',
    code: source(`
      import {
        ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES,
        SYSTEM_PROGRAM_ADDRESS_BYTES,
        TOKEN_PROGRAM_ADDRESS_BYTES,
        account,
        defineTemplate,
        ensureAssociatedTokenAccount,
      } from '@jac0xb/ballista';

      const createIfMissing = defineTemplate({
        accounts: {
          // The programs it calls, pinned to their real addresses.
          ataProgram: {
            executable: true,
            address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES,
          },
          tokenProgram: {
            executable: true,
            address: TOKEN_PROGRAM_ADDRESS_BYTES,
          },
          systemProgram: {
            executable: true,
            address: SYSTEM_PROGRAM_ADDRESS_BYTES,
          },
          // The accounts the caller passes in.
          mint: { owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 82 },
          payer: { signer: true, writable: true },
          wallet: {},
          ata: { writable: true },
        },
        steps: [
          // Calls Create only while the token account is still empty.
          ensureAssociatedTokenAccount({
            associatedTokenProgram: account.fixed('ataProgram'),
            payer: account.fixed('payer'),
            associatedTokenAccount: account.fixed('ata'),
            owner: account.fixed('wallet'),
            mint: account.fixed('mint'),
            systemProgram: account.fixed('systemProgram'),
            tokenProgram: account.fixed('tokenProgram'),
          }),
        ],
      });
    `),
    cases: [
      { label: 'Empty', branch: 'left', outcome: 'The account is empty, so the template calls Create.', result: '1 call' },
      { label: 'Already exists', branch: 'right', outcome: 'The account exists, so Create is skipped and the run still succeeds.', result: 'No call' },
    ],
    route: (branch) => {
      const left = branch === 'left';
      const x = left ? 112 : 368;
      return {
        points: [[240, 50], [240, 78], [left ? 176 : 304, 116], [x, 116], [x, 199], [x, 238], [x, 269], [240, 269], [240, 293]],
        nodes: [{ diamond: [240, 78, 64, 38] }, { box: [left ? 40 : 296, 199, 144, 39] }],
        end: 'done',
      };
    },
  },
  {
    name: 'Send whatever a token account holds',
    title: 'The amount is read when the transaction runs',
    href: '/guide/runtime-values#forward-the-whole-token-balance',
    code: source(`
      import {
        TOKEN_PROGRAM_ADDRESS_BYTES,
        account,
        defineTemplate,
        expression,
        step,
        tokenTransfer,
      } from '@jac0xb/ballista';

      const forwardAll = defineTemplate({
        accounts: {
          tokenProgram: {
            executable: true,
            address: TOKEN_PROGRAM_ADDRESS_BYTES,
          },
          // Owner and size pins make the read at byte 64 a token balance.
          source: {
            writable: true,
            owner: TOKEN_PROGRAM_ADDRESS_BYTES,
            minDataLength: 165,
          },
          destination: {
            writable: true,
            owner: TOKEN_PROGRAM_ADDRESS_BYTES,
            minDataLength: 165,
          },
          authority: { signer: true },
        },
        steps: [
          // Read the token balance while the transaction runs.
          step.let(
            'balance',
            expression.accountData(account.fixed('source'), 64, 'u64'),
          ),
          // Fail the whole run if there is nothing to send.
          step.require(
            expression.greaterThan(
              expression.variable('balance'),
              expression.u64(0),
            ),
          ),
          // Send all of it.
          tokenTransfer({
            tokenProgram: account.fixed('tokenProgram'),
            source: account.fixed('source'),
            destination: account.fixed('destination'),
            authority: account.fixed('authority'),
            amount: expression.variable('balance'),
          }),
        ],
      });
    `),
    cases: [
      { label: 'Holds tokens', branch: 'left', outcome: 'The account holds tokens, so all of them are sent.', result: '1 call' },
      { label: 'Empty', branch: 'right', outcome: 'The account is empty. The check fails and nothing changes.', result: 'Fails' },
    ],
    route: (branch) => {
      const left = branch === 'left';
      const x = left ? 112 : 368;
      const head: Point[] = [[240, 36], [240, 56], [240, 86], [240, 104], [left ? 166 : 314, 142], [x, 142], [x, 213], [x, 253]];
      const nodes: Shape[] = [{ box: [138, 56, 204, 30] }, { diamond: [240, 104, 74, 38] }, { box: [left ? 32 : 288, 213, 160, 40] }];
      return left
        ? { points: [...head, [112, 280], [240, 280], [240, 304]], nodes, end: 'done' }
        : { points: [...head, [368, 277]], nodes, end: 'fail', mark: [368, 289] };
    },
  },
  {
    name: 'Liquidate only if the position is unhealthy',
    title: 'If someone got there first, nothing happens',
    href: '/guide/conditional#liquidate-only-when-unhealthy',
    code: source(`
      import {
        account,
        data,
        defineTemplate,
        expression,
        step,
      } from '@jac0xb/ballista';

      // Works with any lending program. Pass its address, where a
      // position stores its health, and its liquidate instruction:
      // the opening bytes, then the amount to repay.
      function liquidateIfUnhealthy(
        programAddress: Uint8Array<ArrayBuffer>,
        healthOffset: number,
        liquidate: Uint8Array,
        repayAmount: bigint,
      ) {
        return defineTemplate({
          inputs: { threshold: { type: 'u64' } },
          accounts: {
            lendingProgram: { executable: true, address: programAddress },
            position: { owner: programAddress, minDataLength: 128 },
            liquidator: { signer: true, writable: true },
            vault: { writable: true },
          },
          steps: [
            step.invoke({
              program: account.fixed('lendingProgram'),
              // A real liquidation passes more accounts than these two.
              accounts: [
                {
                  account: account.fixed('liquidator'),
                  signer: true,
                  writable: true,
                },
                {
                  account: account.fixed('vault'),
                  signer: false,
                  writable: true,
                },
              ],
              data: [
                data.literal(liquidate),
                data.encode('u64', expression.u64(repayAmount)),
              ],
              // Call only while the position's health is below the
              // threshold. Otherwise skip it; the run still succeeds.
              when: expression.lessThan(
                expression.accountData(
                  account.fixed('position'),
                  healthOffset,
                  'u64',
                ),
                expression.input('threshold'),
              ),
            }),
          ],
        });
      }
    `),
    cases: [
      { label: 'Unhealthy', branch: 'left', outcome: 'The position is unhealthy, so the template liquidates it.', result: '1 call' },
      { label: 'Healthy', branch: 'right', outcome: 'The position is healthy, perhaps because someone liquidated it first. Nothing happens and the run still succeeds.', result: 'No call' },
    ],
    route: (branch) => {
      const left = branch === 'left';
      const x = left ? 112 : 368;
      return {
        points: [[240, 36], [240, 56], [240, 86], [240, 104], [left ? 166 : 314, 142], [x, 142], [x, 213], [x, 253], [x, 280], [240, 280], [240, 304]],
        nodes: [{ box: [138, 56, 204, 30] }, { diamond: [240, 104, 74, 38] }, { box: [left ? 32 : 288, 213, 160, 40] }],
        end: 'done',
      };
    },
  },
  {
    name: 'Pay creditors in order until the money runs out',
    title: 'Each payment depends on what the last one left',
    href: '/guide/loops#waterfall-until-the-money-runs-out',
    code: source(`
      import {
        SYSTEM_PROGRAM_ADDRESS_BYTES,
        account,
        defineTemplate,
        expression,
        step,
        systemTransfer,
      } from '@jac0xb/ballista';

      const waterfall = defineTemplate({
        inputs: { reserve: { type: 'u64' } },
        accounts: {
          systemProgram: {
            executable: true,
            address: SYSTEM_PROGRAM_ADDRESS_BYTES,
          },
          treasury: { signer: true, writable: true },
        },
        // One row per creditor, in the order they get paid.
        batch: {
          maxIterations: 8,
          minIterations: 1,
          row: { creditor: { writable: true } },
          rowInputs: { owed: { type: 'u64' } },
        },
        steps: [
          // What can be paid out: the balance above the reserve.
          step.let(
            'remaining',
            expression.subtract(
              expression.accountField(
                account.fixed('treasury'),
                'lamports',
              ),
              expression.input('reserve'),
            ),
          ),
          step.forEach(
            [
              // Pay what is owed, or what is left if that is less.
              step.let(
                'pay',
                expression.min(
                  expression.variable('remaining'),
                  expression.rowInput('owed'),
                ),
              ),
              // Skip the transfer once nothing is left.
              systemTransfer({
                systemProgram: account.fixed('systemProgram'),
                from: account.fixed('treasury'),
                to: account.iteration('creditor'),
                lamports: expression.variable('pay'),
                when: expression.greaterThan(
                  expression.variable('pay'),
                  expression.u64(0),
                ),
              }),
              step.assign(
                'remaining',
                expression.subtract(
                  expression.variable('remaining'),
                  expression.variable('pay'),
                ),
              ),
            ],
            // Carry what is left into the next row.
            { carry: ['remaining'] },
          ),
        ],
      });
    `),
    cases: waterfallRows.map((row, index) => ({
      label: `Row ${index + 1}`,
      branch: row.pay > 0 ? 'left' : 'right',
      outcome:
        row.pay > 0
          ? `${amount(row.remaining)} left and ${amount(row.owed)} owed, so this creditor gets ${amount(row.pay)}. ${amount(row.after)} is left for the next row.`
          : `Nothing is left, so this creditor gets nothing and the payment is skipped.`,
      result: row.pay > 0 ? '1 call' : 'No call',
    })),
    // The first row enters from START; later rows come back around from NEXT ROW. The last row
    // leaves the loop for DONE.
    route: (branch, index) => {
      const left = branch === 'left';
      const x = left ? 112 : 368;
      const first = index === 0;
      const last = index === waterfallRows.length - 1;
      const entry: Point[] = first
        ? [[240, 30], [240, 46], [240, 74], [240, 94], [240, 116], [240, 144]]
        : [[340, 130], [240, 130], [240, 144]];
      const body: Point[] = [[240, 156], [left ? 188 : 292, 184], [x, 184], [x, 215], [x, 245], [x, 256], [240, 256]];
      const exit: Point[] = last ? [[240, 280], [240, 312]] : [[240, 268], [330, 268], [438, 268], [438, 130], [340, 130]];
      const start: Shape[] = first ? [{ box: [124, 46, 232, 28] }] : [];
      return {
        points: [...entry, ...body, ...exit],
        nodes: [
          ...start,
          { box: [140, 116, 200, 28] },
          { diamond: [240, 156, 52, 28] },
          { box: [left ? 44 : 300, 215, 136, 30] },
          { box: [150, 256, 180, 24] },
        ],
        end: last ? 'done' : 'loop',
      };
    },
  },
  {
    name: 'Cap how much a call can spend',
    title: 'A check after the call can undo it',
    href: '/guide/guardrails#maximum-lamport-spend',
    code: source(`
      import {
        account,
        data,
        defineTemplate,
        expression,
        step,
      } from '@jac0xb/ballista';

      // Deposit into any program's pool, but fail if the payer ends
      // up losing more than maximumSpend. Pass the program's address
      // and its deposit instruction: the opening bytes, then the amount.
      function cappedDeposit(
        programAddress: Uint8Array<ArrayBuffer>,
        deposit: Uint8Array,
        amount: bigint,
      ) {
        return defineTemplate({
          inputs: { maximumSpend: { type: 'u64' } },
          accounts: {
            program: { executable: true, address: programAddress },
            payer: { signer: true, writable: true },
            pool: { writable: true },
          },
          steps: [
            // Note the payer's balance before the call.
            step.snapshot(
              'before',
              expression.accountField(account.fixed('payer'), 'lamports'),
            ),
            step.invoke({
              program: account.fixed('program'),
              accounts: [
                {
                  account: account.fixed('payer'),
                  signer: true,
                  writable: true,
                },
                {
                  account: account.fixed('pool'),
                  signer: false,
                  writable: true,
                },
              ],
              data: [
                data.literal(deposit),
                data.encode('u64', expression.u64(amount)),
              ],
            }),
            // Afterwards, fail if the payer lost more than the limit.
            // Failing undoes the call too.
            step.require(
              expression.lessThanOrEqual(
                expression.subtract(
                  expression.snapshot('before'),
                  expression.accountField(
                    account.fixed('payer'),
                    'lamports',
                  ),
                ),
                expression.input('maximumSpend'),
              ),
            ),
          ],
        });
      }
    `),
    cases: [
      { label: 'Within the limit', branch: 'left', outcome: 'The call spent no more than the limit, so it stands.', result: '1 call' },
      { label: 'Over the limit', branch: 'right', outcome: 'The call spent more than the limit. The check fails and the call is undone.', result: 'Fails' },
    ],
    route: (branch) => {
      const head: Point[] = [[240, 26], [240, 40], [240, 66], [240, 80], [240, 110], [240, 128]];
      const nodes: Shape[] = [{ box: [140, 40, 200, 26] }, { box: [140, 80, 200, 30] }, { diamond: [240, 128, 76, 36] }];
      return branch === 'left'
        ? { points: [...head, [164, 164], [112, 164], [112, 276], [240, 276], [240, 298]], nodes, end: 'done' }
        : {
            points: [...head, [316, 164], [368, 164], [368, 215], [368, 255], [368, 275]],
            nodes: [...nodes, { box: [288, 215, 160, 40] }],
            end: 'fail',
            mark: [368, 287],
          };
    },
  },
];

const selected = ref(0);
const caseByExample = reactive(examples.map(() => 0));
const current = computed(() => examples[selected.value]);
const caseIndex = computed(() => caseByExample[selected.value]);
const active = computed(() => current.value.cases[caseIndex.value]);
const branch = computed(() => active.value.branch);
const row = computed(() => waterfallRows[caseByExample[3]]);
const dash = (side: Side) => (branch.value === side ? undefined : '3 3');

// The chosen case's path draws in from START as a solid line, filling each box and diamond as it
// reaches them; then its dashes march and a token runs along it on a loop. The line is drawn only
// between boxes, and the token runs under the drawing, so both disappear inside a box. Timings
// scale with the path's length so every path moves at a similar pace. A new key replays it all.
const pathOf = (points: Point[]) => `M${points.map(([x, y]) => `${x} ${y}`).join('L')}`;
const route = computed(() => {
  const { points, nodes, end, mark } = current.value.route(branch.value, caseIndex.value);
  const reached = [0];
  for (let index = 1; index < points.length; index++) {
    const [x0, y0] = points[index - 1];
    const [x1, y1] = points[index];
    reached.push(reached[index - 1] + Math.hypot(x1 - x0, y1 - y0));
  }
  const length = reached[reached.length - 1];
  const fraction = (distance: number) => (distance / length).toFixed(3);
  // A segment is inside a box when its midpoint is.
  const inside = (shape: Shape, index: number) => {
    if (!('box' in shape)) return false;
    const [x, y, width, height] = shape.box;
    const middleX = (points[index][0] + points[index + 1][0]) / 2;
    const middleY = (points[index][1] + points[index + 1][1]) / 2;
    return middleX > x && middleX < x + width && middleY > y && middleY < y + height;
  };
  // A box is reached where the path enters it; a diamond, at the first point on its outline.
  const onOutline = (shape: Shape, [px, py]: Point) => {
    if (!('diamond' in shape)) return false;
    const [x, top, halfWidth, halfHeight] = shape.diamond;
    return Math.abs(Math.abs(px - x) / halfWidth + Math.abs(py - top - halfHeight) / halfHeight - 1) < 1e-6;
  };
  const reachedAt = (shape: Shape) =>
    'box' in shape
      ? points.findIndex((_, index) => index < points.length - 1 && inside(shape, index))
      : points.findIndex((point) => onOutline(shape, point));
  // The visible line: runs of segments outside every box.
  const legs: { points: Point[]; first: number; last: number }[] = [];
  for (let index = 0; index < points.length - 1; index++) {
    if (nodes.some((shape) => inside(shape, index))) continue;
    const previous = legs[legs.length - 1];
    if (previous && previous.last === index) {
      previous.points.push(points[index + 1]);
      previous.last = index + 1;
    } else {
      legs.push({ points: [points[index], points[index + 1]], first: index, last: index + 1 });
    }
  }
  return {
    token: pathOf(points),
    legs: legs.map((leg) => ({
      d: pathOf(leg.points),
      at: fraction(reached[leg.first]),
      span: fraction(reached[leg.last] - reached[leg.first]),
    })),
    nodes: nodes.map((shape) => ({ d: outline(shape), at: fraction(reached[reachedAt(shape)]) })),
    start: points[0],
    mark: mark ?? points[points.length - 1],
    end,
    style: {
      '--draw': Math.min(1.4, Math.max(0.7, length / 420)).toFixed(2),
      '--period': (Math.min(2.4, Math.max(1.1, length / 300)) / 0.62).toFixed(2),
    },
  };
});
const routeKey = computed(() => `${selected.value}-${caseIndex.value}`);

// A small highlighter in the github-light colors of the docs' own code blocks. The snippets are
// constants in this file, so escaping is all the input handling they need. A keyword counts only
// when it is not a property or an object key: `step.let(` is a call and `from:` is a key.
const token =
  /(\/\*.*?\*\/|\/\/.*$)|('(?:[^'\\]|\\.)*')|(?<![.\w$])\b(const|let|import|from|export|function|return)\b(?!\s*:)|\b(true|false|\d[\d_]*|[A-Z][A-Z0-9_]*[A-Z0-9])\b|\b([A-Za-z_$][\w$]*)(?=\()/g;
const escape = (text: string) => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
function highlight(source: string): string[] {
  return source.split('\n').map((line) => {
    let html = '';
    let last = 0;
    for (const match of line.matchAll(token)) {
      const [text, comment, string, keyword, constant] = match;
      const kind = comment ? 'comment' : string ? 'string' : keyword ? 'keyword' : constant ? 'constant' : 'call';
      html += `${escape(line.slice(last, match.index))}<span class="tok-${kind}">${escape(text)}</span>`;
      last = (match.index ?? 0) + text.length;
    }
    return html + escape(line.slice(last));
  });
}
const lines = computed(() => highlight(current.value.code));

// The code scrolls inside its panel; a fade at the bottom shows there is more below.
const codePanel = ref<HTMLElement | null>(null);
const hasMore = ref(false);
function measure() {
  const panel = codePanel.value;
  hasMore.value = !!panel && panel.scrollTop + panel.clientHeight < panel.scrollHeight - 4;
}
watch(selected, async () => {
  await nextTick();
  if (codePanel.value) codePanel.value.scrollTop = 0;
  measure();
});
// On phones the drawing is cropped to its content and its labels set larger (see style.css), so
// they stay readable; one label that would no longer fit its box is shortened.
const narrow = ref(false);
let narrowQuery: MediaQueryList | null = null;
const onNarrow = () => (narrow.value = !!narrowQuery?.matches);
onMounted(() => {
  measure();
  window.addEventListener('resize', measure);
  narrowQuery = window.matchMedia('(max-width: 767px)');
  onNarrow();
  narrowQuery.addEventListener('change', onNarrow);
});
onBeforeUnmount(() => {
  window.removeEventListener('resize', measure);
  narrowQuery?.removeEventListener('change', onNarrow);
});

const tabs = ref<HTMLButtonElement[]>([]);
function select(index: number) {
  selected.value = (index + examples.length) % examples.length;
}
async function move(step: number) {
  select(selected.value + step);
  await nextTick();
  tabs.value[selected.value]?.focus();
}
</script>

<template>
  <section class="gallery" aria-labelledby="gallery-title">
    <div class="execution-plate gallery-plate">
      <div class="plate-title">
        <span>Examples</span>
        <span id="gallery-title">{{ current.title }}</span>
      </div>
      <div class="gallery-body">
        <div
          class="gallery-list"
          role="tablist"
          aria-label="Examples"
          aria-orientation="horizontal"
          @keydown.down.prevent="move(1)"
          @keydown.right.prevent="move(1)"
          @keydown.up.prevent="move(-1)"
          @keydown.left.prevent="move(-1)"
        >
          <button
            v-for="(example, index) in examples"
            :key="example.name"
            ref="tabs"
            type="button"
            role="tab"
            class="gallery-tab"
            :id="`gallery-tab-${index}`"
            :aria-selected="index === selected"
            aria-controls="gallery-panel"
            :tabindex="index === selected ? 0 : -1"
            @click="select(index)"
          >
            <span class="tab-number">{{ index + 1 }}</span>
            <span class="tab-name">{{ example.name }}</span>
          </button>
        </div>

        <div class="gallery-code" :class="{ 'has-more': hasMore }">
          <div
            id="gallery-panel"
            ref="codePanel"
            class="gallery-code-scroll"
            role="tabpanel"
            :aria-labelledby="`gallery-tab-${selected}`"
            tabindex="0"
            @scroll="measure"
          >
            <pre><code><span v-for="(line, index) in lines" :key="index" class="code-line" v-html="line" /></code></pre>
          </div>
        </div>

        <div class="gallery-diagram">
          <div class="plate-control">
            <span>{{ selected === 3 ? 'Row' : 'Case' }}</span>
            <div class="state-switch" role="group" :aria-label="`${current.name}: case`">
              <button
                v-for="(option, index) in current.cases"
                :key="option.label"
                type="button"
                :aria-pressed="index === caseByExample[selected]"
                @click="caseByExample[selected] = index"
              >{{ option.label }}</button>
            </div>
          </div>

          <svg class="flow-drawing" :viewBox="narrow ? '24 12 432 316' : '0 0 480 340'" role="img" :aria-label="`${current.name}. ${active.outcome}`">
            <defs>
              <pattern id="gallery-grid" width="16" height="16" patternUnits="userSpaceOnUse"><circle cx="8" cy="8" r=".6" fill="#d4d4ca" /></pattern>
            </defs>
            <rect x="16" y="16" width="448" height="308" fill="url(#gallery-grid)" />
            <path d="M16 24v-8h8m432 0h8v8M16 316v8h8m432 0h8v-8" fill="none" stroke="#686960" stroke-width=".7" />

            <!-- Under the drawing, so it disappears inside each box: a token that runs the chosen path. -->
            <g :key="`token-${routeKey}`" class="route" :style="route.style" aria-hidden="true">
              <path class="route-token" :d="route.token" pathLength="100" />
            </g>

            <!-- 1: create an associated token account only when it is empty. -->
            <g v-if="selected === 0">
              <circle cx="240" cy="50" r="3" fill="#22231f" />
              <text x="254" y="53" class="small-label">START</text>
              <path d="M240 52v26" stroke="#22231f" fill="none" />
              <path d="M240 78l64 38-64 38-64-38z" stroke="#22231f" fill="#f7f6f2" />
              <text x="240" y="120" text-anchor="middle">account empty?</text>
              <g class="branch" :class="{ 'is-active': branch === 'left' }">
                <path d="M176 116h-64v82" stroke="currentColor" fill="none" :stroke-dasharray="dash('left')" />
                <path d="m108 193 4 5 4-5" stroke="currentColor" fill="none" />
                <text x="137" y="107" class="small-label">YES</text>
                <rect x="40" y="199" width="144" height="39" />
                <text x="112" y="216" text-anchor="middle">call Create</text>
                <text x="112" y="231" text-anchor="middle" class="small-label">{{ narrow ? 'ATA PROGRAM' : 'ASSOCIATED TOKEN PROGRAM' }}</text>
                <path d="M112 238v31h128" stroke="currentColor" fill="none" :stroke-dasharray="dash('left')" />
              </g>
              <g class="branch" :class="{ 'is-active': branch === 'right' }">
                <path d="M304 116h64v82" stroke="currentColor" fill="none" :stroke-dasharray="dash('right')" />
                <path d="m364 193 4 5 4-5" stroke="currentColor" fill="none" />
                <text x="327" y="107" class="small-label">NO</text>
                <rect x="296" y="199" width="144" height="39" />
                <text x="368" y="223" text-anchor="middle">skip</text>
                <path d="M368 238v31H240" stroke="currentColor" fill="none" :stroke-dasharray="dash('right')" />
              </g>
              <path d="M240 269v22" stroke="#22231f" />
              <circle cx="240" cy="293" r="3" fill="#22231f" />
              <text x="254" y="296" class="small-label">DONE</text>
            </g>

            <!-- 2: read a token balance, check it is not zero, send all of it. -->
            <g v-else-if="selected === 1">
              <circle cx="240" cy="36" r="3" fill="#22231f" />
              <text x="254" y="39" class="small-label">START</text>
              <path d="M240 39v17" stroke="#22231f" />
              <rect x="138" y="56" width="204" height="30" fill="#f7f6f2" stroke="#22231f" />
              <text x="240" y="75" text-anchor="middle">read the token balance</text>
              <path d="M240 86v18" stroke="#22231f" />
              <path d="M240 104l74 38-74 38-74-38z" stroke="#22231f" fill="#f7f6f2" />
              <text x="240" y="146" text-anchor="middle">more than zero?</text>
              <g class="branch" :class="{ 'is-active': branch === 'left' }">
                <path d="M166 142h-54v70" stroke="currentColor" fill="none" :stroke-dasharray="dash('left')" />
                <path d="m108 207 4 5 4-5" stroke="currentColor" fill="none" />
                <text x="132" y="133" class="small-label">YES</text>
                <rect x="32" y="213" width="160" height="40" />
                <text x="112" y="230" text-anchor="middle">send all of it</text>
                <text x="112" y="245" text-anchor="middle" class="small-label">TOKEN PROGRAM</text>
                <path d="M112 253v27h128" stroke="currentColor" fill="none" :stroke-dasharray="dash('left')" />
              </g>
              <g class="branch" :class="{ 'is-active': branch === 'right' }">
                <path d="M314 142h54v70" stroke="currentColor" fill="none" :stroke-dasharray="dash('right')" />
                <path d="m364 207 4 5 4-5" stroke="currentColor" fill="none" />
                <text x="334" y="133" class="small-label">NO</text>
                <rect x="288" y="213" width="160" height="40" />
                <text x="368" y="230" text-anchor="middle">check fails</text>
                <text x="368" y="245" text-anchor="middle" class="small-label">NOTHING CHANGES</text>
                <path d="M368 253v24" stroke="currentColor" fill="none" :stroke-dasharray="dash('right')" />
                <path d="M362 283l12 12m0-12l-12 12" stroke="currentColor" fill="none" />
                <text x="382" y="293" class="small-label">FAILS</text>
              </g>
              <path d="M240 280v22" stroke="#22231f" />
              <circle cx="240" cy="304" r="3" fill="#22231f" />
              <text x="254" y="307" class="small-label">DONE</text>
            </g>

            <!-- 3: read the position's health and liquidate only below the threshold. -->
            <g v-else-if="selected === 2">
              <circle cx="240" cy="36" r="3" fill="#22231f" />
              <text x="254" y="39" class="small-label">START</text>
              <path d="M240 39v17" stroke="#22231f" />
              <rect x="138" y="56" width="204" height="30" fill="#f7f6f2" stroke="#22231f" />
              <text x="240" y="75" text-anchor="middle">read the position's health</text>
              <path d="M240 86v18" stroke="#22231f" />
              <path d="M240 104l74 38-74 38-74-38z" stroke="#22231f" fill="#f7f6f2" />
              <text x="240" y="146" text-anchor="middle">below threshold?</text>
              <g class="branch" :class="{ 'is-active': branch === 'left' }">
                <path d="M166 142h-54v70" stroke="currentColor" fill="none" :stroke-dasharray="dash('left')" />
                <path d="m108 207 4 5 4-5" stroke="currentColor" fill="none" />
                <text x="132" y="133" class="small-label">YES</text>
                <rect x="32" y="213" width="160" height="40" />
                <text x="112" y="230" text-anchor="middle">call liquidate</text>
                <text x="112" y="245" text-anchor="middle" class="small-label">LENDING PROGRAM</text>
                <path d="M112 253v27h128" stroke="currentColor" fill="none" :stroke-dasharray="dash('left')" />
              </g>
              <g class="branch" :class="{ 'is-active': branch === 'right' }">
                <path d="M314 142h54v70" stroke="currentColor" fill="none" :stroke-dasharray="dash('right')" />
                <path d="m364 207 4 5 4-5" stroke="currentColor" fill="none" />
                <text x="334" y="133" class="small-label">NO</text>
                <rect x="288" y="213" width="160" height="40" />
                <text x="368" y="237" text-anchor="middle">skip</text>
                <path d="M368 253v27H240" stroke="currentColor" fill="none" :stroke-dasharray="dash('right')" />
              </g>
              <path d="M240 280v22" stroke="#22231f" />
              <circle cx="240" cy="304" r="3" fill="#22231f" />
              <text x="254" y="307" class="small-label">DONE</text>
            </g>

            <!-- 4: one loop; what is left after each payment carries into the next row. -->
            <g v-else-if="selected === 3">
              <circle cx="240" cy="30" r="3" fill="#22231f" />
              <text x="254" y="33" class="small-label">START</text>
              <path d="M240 33v13" stroke="#22231f" />
              <rect x="124" y="46" width="232" height="28" fill="#f7f6f2" stroke="#22231f" />
              <text x="240" y="64" text-anchor="middle">remaining = treasury − reserve</text>
              <path d="M240 74v20" stroke="#22231f" />
              <rect x="28" y="94" width="424" height="194" fill="none" stroke="#686960" stroke-dasharray="4 3" />
              <text x="36" y="108" class="small-label">FOR EACH CREDITOR</text>
              <text x="444" y="108" class="small-label" text-anchor="end">ROW {{ caseByExample[3] + 1 }} OF {{ waterfallRows.length }}</text>
              <rect x="140" y="116" width="200" height="28" fill="#f7f6f2" stroke="#22231f" />
              <text x="240" y="134" text-anchor="middle">pay = min(remaining, owed)</text>
              <text x="134" y="134" class="small-label value-label" text-anchor="end">= {{ amount(row.pay) }}</text>
              <path d="M240 144v12" stroke="#22231f" />
              <path d="M240 156l52 28-52 28-52-28z" stroke="#22231f" fill="#f7f6f2" />
              <text x="240" y="188" text-anchor="middle">pay &gt; 0?</text>
              <g class="branch" :class="{ 'is-active': branch === 'left' }">
                <path d="M188 184h-76v30" stroke="currentColor" fill="none" :stroke-dasharray="dash('left')" />
                <path d="m108 209 4 5 4-5" stroke="currentColor" fill="none" />
                <text x="160" y="176" class="small-label">YES</text>
                <rect x="44" y="215" width="136" height="30" />
                <text x="112" y="234" text-anchor="middle">send pay</text>
                <path d="M112 245v11h128" stroke="currentColor" fill="none" :stroke-dasharray="dash('left')" />
              </g>
              <g class="branch" :class="{ 'is-active': branch === 'right' }">
                <path d="M292 184h76v30" stroke="currentColor" fill="none" :stroke-dasharray="dash('right')" />
                <path d="m364 209 4 5 4-5" stroke="currentColor" fill="none" />
                <text x="304" y="176" class="small-label">NO</text>
                <rect x="300" y="215" width="136" height="30" />
                <text x="368" y="234" text-anchor="middle">skip</text>
                <path d="M368 245v11H240" stroke="currentColor" fill="none" :stroke-dasharray="dash('right')" />
              </g>
              <rect x="150" y="256" width="180" height="24" fill="#f7f6f2" stroke="#22231f" />
              <text x="240" y="272" text-anchor="middle">remaining −= pay</text>
              <text x="144" y="272" class="small-label value-label" text-anchor="end">= {{ amount(row.after) }}</text>
              <path d="M330 268h108V130h-98" stroke="#686960" fill="none" />
              <path d="m345 126-5 4 5 4" stroke="#686960" fill="none" />
              <text x="432" y="198" class="small-label" text-anchor="end">NEXT ROW</text>
              <path d="M240 280v30" stroke="#22231f" />
              <text x="250" y="300" class="small-label">AFTER THE LAST ROW</text>
              <circle cx="240" cy="312" r="3" fill="#22231f" />
              <text x="254" y="315" class="small-label">DONE</text>
            </g>

            <!-- 5: note the balance, make the call, then check what it spent. -->
            <g v-else>
              <circle cx="240" cy="26" r="3" fill="#22231f" />
              <text x="254" y="29" class="small-label">START</text>
              <path d="M240 29v11" stroke="#22231f" />
              <rect x="140" y="40" width="200" height="26" fill="#f7f6f2" stroke="#22231f" />
              <text x="240" y="57" text-anchor="middle">note the balance</text>
              <path d="M240 66v14" stroke="#22231f" />
              <rect x="140" y="80" width="200" height="30" fill="#f4ebe5" stroke="#be3b23" />
              <text x="240" y="99" text-anchor="middle" fill="#be3b23">call the protocol</text>
              <path d="M240 110v18" stroke="#22231f" />
              <path d="M240 128l76 36-76 36-76-36z" stroke="#22231f" fill="#f7f6f2" />
              <text x="240" y="168" text-anchor="middle">spent ≤ limit?</text>
              <g class="branch" :class="{ 'is-active': branch === 'left' }">
                <path d="M164 164h-52v112h128" stroke="currentColor" fill="none" :stroke-dasharray="dash('left')" />
                <text x="132" y="155" class="small-label">YES</text>
              </g>
              <g class="branch" :class="{ 'is-active': branch === 'right' }">
                <path d="M316 164h52v50" stroke="currentColor" fill="none" :stroke-dasharray="dash('right')" />
                <path d="m364 209 4 5 4-5" stroke="currentColor" fill="none" />
                <text x="334" y="155" class="small-label">NO</text>
                <rect x="288" y="215" width="160" height="40" />
                <text x="368" y="232" text-anchor="middle">check fails</text>
                <text x="368" y="247" text-anchor="middle" class="small-label">THE CALL IS UNDONE</text>
                <path d="M368 255v20" stroke="currentColor" fill="none" :stroke-dasharray="dash('right')" />
                <path d="M362 281l12 12m0-12l-12 12" stroke="currentColor" fill="none" />
                <text x="382" y="291" class="small-label">FAILS</text>
              </g>
              <path d="M240 276v20" stroke="#22231f" />
              <circle cx="240" cy="298" r="3" fill="#22231f" />
              <text x="254" y="301" class="small-label">DONE</text>
            </g>

            <!-- Over the drawing: boxes fill as the path reaches them, the line draws in and its
                 dashes march, and the ends ping. -->
            <g :key="`path-${routeKey}`" class="route" :style="route.style" aria-hidden="true">
              <path v-for="(node, index) in route.nodes" :key="`node-${index}`" class="route-node" :d="node.d" :style="{ '--at': node.at }" />
              <path
                v-for="(leg, index) in route.legs"
                :key="`leg-${index}`"
                class="route-leg"
                :d="leg.d"
                pathLength="100"
                :style="{ '--at': leg.at, '--span': leg.span }"
              />
              <path v-for="(leg, index) in route.legs" :key="`march-${index}`" class="route-march" :d="leg.d" />
              <circle v-if="route.end === 'done'" class="route-end" :cx="route.mark[0]" :cy="route.mark[1]" r="3.4" />
              <circle class="route-ping at-start" :cx="route.start[0]" :cy="route.start[1]" r="3" />
              <circle class="route-ping at-end" :cx="route.mark[0]" :cy="route.mark[1]" r="3" />
            </g>
          </svg>

          <div class="plate-readout" aria-live="polite" aria-atomic="true">
            <span :key="`outcome-${routeKey}`" class="readout-line">{{ active.outcome }}</span>
            <strong :key="`result-${routeKey}`" class="readout-line">{{ active.result }}</strong>
          </div>
        </div>
      </div>
    </div>
    <div class="plate-caption gallery-caption">
      <a :href="withBase(current.href)">Read this example ↗</a>
      <a :href="withBase('/examples/')">All {{ facts.examples }} examples ↗</a>
    </div>
  </section>
</template>
