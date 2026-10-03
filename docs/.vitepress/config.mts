import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitepress';
import { reviewComments } from './review-comments';

const base = process.env.BALLISTA_DOCS_BASE ?? '/ballista/';
const origin = process.env.BALLISTA_DOCS_ORIGIN ?? 'https://jac0xb.github.io';
const { version } = JSON.parse(readFileSync(new URL('../../clients/js/package.json', import.meta.url), 'utf8'));
const release = version.split('.').slice(0, 2).join('.');
const title = 'Ballista — execution, composed';
const summary =
  'Ballista stores a sequence of Solana program calls, and the checks between them, as a template on chain. Anyone can run it later with new inputs.';

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
      { text: 'Account groups', link: '/guide/account-groups' },
      { text: 'Assertions and snapshots', link: '/guide/assertions' },
      { text: 'PDA and ATA assertions', link: '/guide/pda-assertions' },
      { text: 'Errors and events', link: '/guide/errors-and-events' },
      { text: 'Inspecting a template', link: '/guide/inspecting-templates' },
    ],
  },
  {
    text: 'Security',
    items: [
      { text: 'Trust model', link: '/guide/trust-model' },
      { text: 'Security posture', link: '/guide/security' },
      { text: 'Failure modes and recovery', link: '/guide/failure-modes' },
      { text: 'Formal verification', link: '/guide/formal-verification' },
    ],
  },
  {
    text: 'Deploy',
    items: [{ text: 'Devnet workflow', link: '/guide/devnet' }],
  },
];

const examplesSidebar = [
  {
    text: 'Examples',
    items: [{ text: 'All examples', link: '/examples/' }],
  },
  {
    text: 'Protocol templates',
    link: '/examples/protocols/',
    collapsed: true,
    items: [
      { text: 'Jupiter · Deposit what a swap made', link: '/examples/protocols/jupiter-deposit' },
      { text: 'Jupiter · Oracle-checked swap', link: '/examples/protocols/jupiter-oracle-swap' },
      { text: 'Jupiter · Sell a whole balance', link: '/examples/protocols/token-sweep' },
      { text: 'Kamino · Repay what a swap made', link: '/examples/protocols/kamino-repay' },
      { text: 'Kamino · Liquidate, minimum payout', link: '/examples/protocols/kamino-liquidate' },
      { text: 'marginfi · Withdraw everything', link: '/examples/protocols/marginfi-withdraw' },
      { text: 'Kamino · Move funds from marginfi', link: '/examples/protocols/marginfi-to-kamino' },
      { text: 'Orca · Compound collected fees', link: '/examples/protocols/orca-compound' },
      { text: 'Orca · Harvest positions that earned', link: '/examples/protocols/orca-harvest' },
      { text: 'Pyth · Act on a fresh price', link: '/examples/protocols/pyth-gate' },
      { text: 'Jito · Tip only from profit', link: '/examples/protocols/jito-tip' },
      { text: 'Ed25519 · Settle at a signed quote', link: '/examples/protocols/signed-quote' },
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
  {
    text: 'Payment patterns',
    link: '/examples/payments',
    collapsed: true,
    items: [
      { text: 'SOL payroll', link: '/examples/payments#bounded-sol-payroll' },
      { text: 'Revenue split', link: '/examples/payments#basis-point-revenue-split' },
      { text: 'Weighted rewards', link: '/examples/payments#index-weighted-rewards' },
      { text: 'Deadline refund', link: '/examples/payments#deadline-refund' },
      { text: 'Reserve-preserving sweep', link: '/examples/payments#reserve-preserving-sweep' },
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
      { text: 'Glossary', link: '/reference/glossary' },
    ],
  },
  {
    text: 'Limits and scope',
    items: [
      { text: 'Limits', link: '/reference/limits' },
      { text: 'Transaction v1', link: '/guide/transaction-v1' },
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
  },
  themeConfig: {
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
      { text: 'Guide', link: '/guide/getting-started', activeMatch: '^/guide/(?!transaction-v1)' },
      { text: 'Examples', link: '/examples/' },
      { text: 'Reference', link: '/reference/typescript', activeMatch: '^/(reference/|scope|guide/transaction-v1)' },
    ],
    sidebar: {
      // These two live outside /reference/ but belong with it. They come first: VitePress picks the
      // deepest matching key, and '/guide/transaction-v1' ties with '/guide/' on depth.
      '/guide/transaction-v1': referenceSidebar,
      '/scope': referenceSidebar,
      '/guide/': guideSidebar,
      '/examples/': examplesSidebar,
      '/reference/': referenceSidebar,
    },
    socialLinks: [{ icon: 'github', link: 'https://github.com/Jac0xb/ballista' }],
    footer: {
      message: 'BALLISTA / A SMALL MACHINE FOR COMPLEX TRANSACTIONS',
      copyright: `OPEN SOURCE · MIT LICENSE · v${release}`,
    },
  },
});
