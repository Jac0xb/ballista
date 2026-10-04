import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitepress';
import { reviewComments } from './review-comments';

const base = process.env.BALLISTA_DOCS_BASE ?? '/ballista/';
const origin = process.env.BALLISTA_DOCS_ORIGIN ?? 'https://jac0xb.github.io';
const { version } = JSON.parse(readFileSync(new URL('../../clients/js/package.json', import.meta.url), 'utf8'));
const release = version.split('.').slice(0, 2).join('.');
// A Solana address that takes donations toward an audit. Unset, the docs show no address.
const auditFund = process.env.BALLISTA_AUDIT_FUND?.trim() || undefined;
if (auditFund && !/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(auditFund)) {
  throw new Error(`BALLISTA_AUDIT_FUND is not a base58 Solana address: ${auditFund}`);
}
const title = 'Ballista — execution, composed';

// Numbers in prose and tables render as inline code chips (`.num` in style.css): a standalone
// number such as 1,014, 4,096 or 0.5, and inline code that is only a number. A currency symbol or
// unit next to the number goes in the same chip: $5, 1 SOL, 5,080 lamports, 80-byte, 64 KiB.
// Headings, code blocks and numbers inside words (v1, ed25519) are left alone.
const UNIT = String.raw`(?:%|-?bytes?\b|\s(?:SOL|USDC|USDT|lamports?|bytes?|KiB|MiB|seconds?|minutes?|hours?|days?|ms|CU|bps)\b)`;
const NUMBER = new RegExp(String.raw`(?<![\w.#/-])\$?\d[\d,]*(?:\.\d+)?${UNIT}?(?![\w])`, 'g');
const NUMBER_ONLY = new RegExp(String.raw`^\$?[\d,]+(?:\.\d+)?${UNIT}?$`);
type MdToken = { type: string; content: string; children: MdToken[] | null; attrJoin?: (n: string, v: string) => void };
function numberChips(state: { tokens: MdToken[]; Token: new (type: string, tag: string, nesting: number) => MdToken }) {
  let inHeading = false;
  for (const token of state.tokens) {
    if (token.type === 'heading_open') inHeading = true;
    if (token.type === 'heading_close') inHeading = false;
    if (token.type !== 'inline' || inHeading || !token.children) continue;
    const out: MdToken[] = [];
    let inLink = 0;
    for (const child of token.children) {
      if (child.type === 'link_open') inLink++;
      if (child.type === 'link_close') inLink--;
      if (child.type === 'code_inline' && NUMBER_ONLY.test(child.content)) {
        child.attrJoin?.('class', 'num');
        out.push(child);
        continue;
      }
      if (child.type !== 'text' || inLink) {
        out.push(child);
        continue;
      }
      let last = 0;
      for (const match of child.content.matchAll(NUMBER)) {
        const index = match.index ?? 0;
        if (index > last) {
          const text = new state.Token('text', '', 0);
          text.content = child.content.slice(last, index);
          out.push(text);
        }
        const code = new state.Token('code_inline', 'code', 0);
        code.content = match[0];
        code.attrJoin?.('class', 'num');
        out.push(code);
        last = index + match[0].length;
      }
      if (last === 0) {
        out.push(child);
        continue;
      }
      if (last < child.content.length) {
        const text = new state.Token('text', '', 0);
        text.content = child.content.slice(last);
        out.push(text);
      }
    }
    token.children = out;
  }
}
const summary =
  'Ballista stores a sequence of Solana program calls, and the checks between them, as a template on chain. Anyone can run it later with new inputs.';

// Examples live in the guide's sidebar: patterns first, then each protocol's templates in a group
// of their own.
const examplesSidebar = [
  {
    text: 'Examples',
    items: [
      { text: 'All examples', link: '/examples/' },
      {
        text: 'Payment patterns',
        link: '/examples/payments',
        collapsed: true,
        items: [
          { text: 'Revenue split', link: '/examples/payments#basis-point-revenue-split' },
          { text: 'Weighted rewards', link: '/examples/payments#index-weighted-rewards' },
          { text: 'Deadline refund', link: '/examples/payments#deadline-refund' },
          { text: 'Reserve-preserving sweep', link: '/examples/payments#reserve-preserving-sweep' },
          { text: 'Payment agent', link: '/examples/payments#payment-agent' },
        ],
      },
      {
        text: 'Token-account patterns',
        link: '/examples/token-accounts',
        collapsed: true,
        items: [
          { text: 'Create then transfer', link: '/examples/token-accounts#assert-create-then-transfer' },
          { text: 'Token payroll', link: '/examples/token-accounts#existing-account-token-payroll' },
          { text: 'Conditional ATA setup', link: '/examples/token-accounts#conditional-ata-setup' },
          { text: 'Close empty accounts', link: '/examples/token-accounts#close-empty-token-accounts' },
          { text: 'Exact token debit', link: '/examples/token-accounts#exact-token-debit' },
        ],
      },
      {
        text: 'Protocol composition',
        link: '/examples/composition',
        collapsed: true,
        items: [
          { text: 'Swap then deposit', link: '/examples/composition#swap-then-deposit' },
          { text: 'Claim then distribute', link: '/examples/composition#claim-then-distribute' },
          { text: 'Fallback route', link: '/examples/composition#primary-or-fallback-route' },
          { text: 'Time-gated governance', link: '/examples/composition#time-gated-governance-execution' },
          { text: 'Keeper crank', link: '/examples/composition#bounded-keeper-crank' },
        ],
      },
    ],
  },
  {
    text: 'Protocol templates',
    items: [
      { text: 'Overview', link: '/examples/protocols/' },
      {
        text: 'Jupiter',
        collapsed: true,
        items: [
          { text: 'Deposit what a swap produced', link: '/examples/protocols/jupiter-deposit' },
          { text: 'Swap checked against an oracle', link: '/examples/protocols/jupiter-oracle-swap' },
          { text: 'Sell a whole balance', link: '/examples/protocols/token-sweep' },
          { text: "Cap a caller's daily swaps", link: '/examples/protocols/daily-cap' },
        ],
      },
      {
        text: 'Kamino',
        collapsed: true,
        items: [
          { text: 'Repay what a swap produced', link: '/examples/protocols/kamino-repay' },
          { text: 'Liquidate with a minimum payout', link: '/examples/protocols/kamino-liquidate' },
        ],
      },
      {
        text: 'Orca',
        collapsed: true,
        items: [
          { text: 'Compound collected fees', link: '/examples/protocols/orca-compound' },
          { text: 'Harvest positions that earned', link: '/examples/protocols/orca-harvest' },
        ],
      },
      {
        text: 'pump.fun',
        collapsed: true,
        items: [
          { text: 'Buy a basket within a budget', link: '/examples/protocols/pump-buy-basket' },
          { text: 'Sell all of a coin above a floor', link: '/examples/protocols/pump-sell-all' },
        ],
      },
      { text: 'Pyth', collapsed: true, items: [{ text: 'Act only on a fresh price', link: '/examples/protocols/pyth-gate' }] },
      { text: 'Ed25519', collapsed: true, items: [{ text: 'Settle at a signed quote', link: '/examples/protocols/signed-quote' }] },
    ],
  },
];

const guideSidebar = [
  {
    text: 'Start here',
    items: [
      { text: 'Why Ballista?', link: '/guide/why-ballista' },
      { text: 'Getting started', link: '/guide/getting-started' },
      { text: 'How it works', link: '/guide/mental-model' },
    ],
  },
  {
    text: 'What templates can do',
    items: [
      { text: 'Amounts read at run time', link: '/guide/runtime-values' },
      { text: 'Conditional calls', link: '/guide/conditional' },
      { text: 'Loops over rows and counts', link: '/guide/loops' },
      { text: 'Remember state between runs', link: '/guide/registries' },
      { text: 'Safety guardrails', link: '/guide/guardrails' },
    ],
  },
  {
    text: 'Build templates',
    items: [
      { text: 'Template lifecycle', link: '/guide/template-lifecycle' },
      { text: 'Inputs and expressions', link: '/guide/expressions' },
      { text: 'Accounts and CPIs', link: '/guide/accounts-and-cpis' },
      { text: 'Batch execution', link: '/guide/batching' },
      { text: 'Assertions and snapshots', link: '/guide/assertions' },
      { text: 'PDA and ATA assertions', link: '/guide/pda-assertions' },
      { text: 'Errors and events', link: '/guide/errors-and-events' },
    ],
  },
  ...examplesSidebar,
  {
    text: 'Security',
    items: [
      { text: 'Trust model', link: '/guide/trust-model' },
      { text: 'Security posture', link: '/guide/security' },
      { text: 'Failure modes and recovery', link: '/guide/failure-modes' },
    ],
  },
];


const referenceSidebar = [
  {
    text: 'SDKs',
    items: [
      { text: 'TypeScript SDK', link: '/reference/typescript' },
      { text: 'Rust SDK', link: '/reference/rust' },
    ],
  },
  {
    text: 'Templates',
    items: [
      { text: 'Template language', link: '/reference/language' },
      { text: 'Wire format', link: '/reference/wire-format' },
      { text: 'Error codes', link: '/reference/errors' },
      { text: 'Glossary', link: '/reference/glossary' },
    ],
  },
  {
    text: 'Limits and scope',
    items: [
      { text: 'Limits', link: '/reference/limits' },
      { text: 'Scope and design choices', link: '/scope' },
    ],
  },
];

export default defineConfig({
  lang: 'en-US',
  title: 'Ballista',
  description: summary,
  base,
  cleanUrls: true,
  // Working notes for contributors, not pages for readers.
  srcExclude: ['README.md', 'implementation-plan.md', 'superpowers/**'],
  // Highlight-and-comment review on the dev server; see review-comments.ts.
  vite: {
    plugins: [reviewComments()],
    resolve: {
      // The default search box is replaced by SearchPalette, which reads the same index.
      alias: [{ find: /^.*\/VPLocalSearchBox\.vue$/, replacement: fileURLToPath(new URL('./theme/SearchPalette.vue', import.meta.url)) }],
    },
  },
  lastUpdated: true,
  appearance: true,
  head: [
    ['meta', { name: 'theme-color', content: '#f7f6f2' }],
    ['link', { rel: 'icon', type: 'image/svg+xml', href: `${base}ballista-mark.svg` }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:url', content: `${origin}${base}` }],
    ['meta', { property: 'og:title', content: title }],
    ['meta', { property: 'og:description', content: summary }],
    ['meta', { property: 'og:image', content: `${origin}${base}og.png` }],
    ['meta', { property: 'og:image:width', content: '1200' }],
    ['meta', { property: 'og:image:height', content: '630' }],
    ['meta', { property: 'og:image:alt', content: 'Execution, composed. A check decides whether a Solana program is called.' }],
    ['meta', { name: 'twitter:card', content: 'summary_large_image' }],
    ['meta', { name: 'twitter:image', content: `${origin}${base}og.png` }],
  ],
  markdown: {
    lineNumbers: true,
    theme: { light: 'github-light', dark: 'github-dark' },
    config: (md) => md.core.ruler.push('number-chips', numberChips),
  },
  themeConfig: {
    auditFund,
    logo: { light: '/ballista-mark.svg', dark: '/ballista-mark-dark.svg' },
    siteTitle: 'Ballista',
    search: {
      provider: 'local',
      options: {
        // Each section keeps its text in the index, so results can show where the words matched.
        miniSearch: { options: { storeFields: ['title', 'titles', 'text'] } },
        // The generated cost tables repeat the same words under every example; they stay out of the index.
        _render(src, env, md) {
          const html = md.render(src.replace(/<!-- benchmark:[\s\S]*?<!-- \/benchmark -->/g, ''), env);
          return env.frontmatter?.search === false ? '' : html;
        },
      },
    },
    outline: { level: [2, 3], label: 'On this page' },
    editLink: {
      pattern: 'https://github.com/Jac0xb/ballista/edit/main/docs/:path',
      text: 'Edit this page on GitHub',
    },
    nav: [
      { text: 'Guide', link: '/guide/why-ballista', activeMatch: '^/(guide|examples)/' },
      { text: 'Reference', link: '/reference/typescript', activeMatch: '^/(reference/|scope)' },
    ],
    sidebar: {
      // Scope lives outside /reference/ but belongs with it.
      '/scope': referenceSidebar,
      '/guide/': guideSidebar,
      '/examples/': guideSidebar,
      '/reference/': referenceSidebar,
    },
    socialLinks: [{ icon: 'github', link: 'https://github.com/Jac0xb/ballista' }],
    footer: {
      message: 'BALLISTA / A SMALL MACHINE FOR COMPLEX TRANSACTIONS',
      // The version is the SDK's; the program itself is not yet released.
      copyright: `OPEN SOURCE · MIT LICENSE · SDK v${release} · PROGRAM PRE-RELEASE`,
    },
  },
});
