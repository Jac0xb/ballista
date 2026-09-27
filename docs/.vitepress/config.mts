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
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'Examples', link: '/examples/' },
      { text: 'Reference', link: '/reference/typescript' },
      {
        text: 'More',
        items: [
          { text: 'Scope and limits', link: '/scope' },
          { text: 'What a run costs', link: '/benchmarks' },
          { text: 'Where the compute goes', link: '/cu-profile' },
        ],
      },
    ],
    sidebar: {
      '/guide/': [
        {
          text: 'Start here',
          items: [
            { text: 'Why Ballista?', link: '/guide/why-ballista' },
            { text: 'Getting started', link: '/guide/getting-started' },
            { text: 'Mental model', link: '/guide/mental-model' },
          ],
        },
        {
          text: 'What templates can do',
          items: [
            { text: 'Amounts read at run time', link: '/guide/runtime-values' },
            { text: 'Conditional calls', link: '/guide/conditional' },
            { text: 'Loops that decide per row', link: '/guide/loops' },
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
            { text: 'Trust model', link: '/guide/trust-model' },
            { text: 'Errors and events', link: '/guide/errors-and-events' },
            { text: 'Formal verification', link: '/guide/formal-verification' },
          ],
        },
        {
          text: 'Deploy',
          items: [
            { text: 'Transaction v1', link: '/guide/transaction-v1' },
            { text: 'Devnet workflow', link: '/guide/devnet' },
          ],
        },
      ],
      '/examples/': [
        {
          text: 'Examples',
          items: [
            { text: 'All examples', link: '/examples/' },
          ],
        },
        {
          text: 'Protocol templates',
          collapsed: true,
          items: [
            { text: 'Overview', link: '/examples/protocols/' },
            { text: 'Jupiter · deposit swap output', link: '/examples/protocols/jupiter-deposit' },
            { text: 'Jupiter · oracle-checked swap', link: '/examples/protocols/jupiter-oracle-swap' },
            { text: 'Jupiter · sell whole balance', link: '/examples/protocols/token-sweep' },
            { text: 'Kamino · repay from swap', link: '/examples/protocols/kamino-repay' },
            { text: 'Kamino · liquidate', link: '/examples/protocols/kamino-liquidate' },
            { text: 'marginfi · withdraw', link: '/examples/protocols/marginfi-withdraw' },
            { text: 'Drift · rebalance', link: '/examples/protocols/drift-rebalance' },
            { text: 'Drift · settle and withdraw', link: '/examples/protocols/drift-settle' },
            { text: 'Orca · compound fees', link: '/examples/protocols/orca-compound' },
            { text: 'Orca · harvest', link: '/examples/protocols/orca-harvest' },
            { text: 'Pyth · price gate', link: '/examples/protocols/pyth-gate' },
            { text: 'Jito · conditional tip', link: '/examples/protocols/jito-tip' },
          ],
        },
        {
          text: 'Composition',
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
          text: 'Payments',
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
          text: 'Token accounts',
          collapsed: true,
          items: [
            { text: 'Create then transfer', link: '/examples/token-accounts#assert-create-then-transfer' },
            { text: 'Token payroll', link: '/examples/token-accounts#existing-account-token-payroll' },
            { text: 'Conditional ATA setup', link: '/examples/token-accounts#conditional-ata-setup' },
            { text: 'Close empty accounts', link: '/examples/token-accounts#close-empty-token-accounts' },
            { text: 'Exact token debit', link: '/examples/token-accounts#exact-token-debit' },
          ],
        },
      ],
      '/reference/': [
        {
          text: 'Reference',
          items: [
            { text: 'TypeScript SDK', link: '/reference/typescript' },
            { text: 'Rust SDK', link: '/reference/rust' },
            { text: 'Template language', link: '/reference/language' },
            { text: 'Wire format', link: '/reference/wire-format' },
            { text: 'Limits', link: '/reference/limits' },
          ],
        },
      ],
    },
    socialLinks: [{ icon: 'github', link: 'https://github.com/Jac0xb/ballista' }],
    footer: {
      message: 'BALLISTA / A SMALL MACHINE FOR COMPLEX TRANSACTIONS',
      copyright: `OPEN SOURCE · MIT LICENSE · v${release}`,
    },
  },
});
