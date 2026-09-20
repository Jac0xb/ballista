import { defineConfig } from 'vitepress';

export default defineConfig({
  lang: 'en-US',
  title: 'Ballista',
  description: 'The bounded orchestration VM for reusable Solana transaction templates.',
  base: process.env.BALLISTA_DOCS_BASE ?? '/ballista/',
  cleanUrls: true,
  lastUpdated: true,
  appearance: false,
  head: [
    ['meta', { name: 'theme-color', content: '#f7f6f2' }],
    ['link', { rel: 'icon', type: 'image/svg+xml', href: `${process.env.BALLISTA_DOCS_BASE ?? '/ballista/'}ballista-mark.svg` }],
    ['meta', { property: 'og:title', content: 'Ballista · Bounded on-chain orchestration' }],
    [
      'meta',
      {
        property: 'og:description',
        content: 'Compile complex Solana workflows once, store them on-chain, and execute them with fresh inputs.',
      },
    ],
  ],
  markdown: {
    lineNumbers: true,
    theme: { light: 'github-light', dark: 'github-light' },
  },
  themeConfig: {
    logo: '/ballista-mark.svg',
    siteTitle: 'Ballista',
    search: { provider: 'local' },
    outline: { level: [2, 3], label: 'On this page' },
    editLink: {
      pattern: 'https://github.com/Jac0xb/ballista/edit/main/docs/:path',
      text: 'Edit this page on GitHub',
    },
    nav: [
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'Examples', link: '/examples/' },
      { text: 'Use cases', link: '/use-cases' },
      { text: 'Reference', link: '/reference/typescript' },
      {
        text: '1.0',
        items: [
          { text: 'Scope and limits', link: '/scope' },
          { text: 'Benchmarks', link: '/benchmarks' },
          { text: 'Implementation status', link: '/implementation-plan' },
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
          text: 'Ship',
          items: [
            { text: 'Transaction v1', link: '/guide/transaction-v1' },
            { text: 'Devnet workflow', link: '/guide/devnet' },
          ],
        },
      ],
      '/examples/': [
        {
          text: 'Example cookbook',
          items: [
            { text: 'All examples', link: '/examples/' },
            { text: 'Author and run in both languages', link: '/examples/end-to-end' },
            { text: 'Payments', link: '/examples/payments' },
            { text: 'Token accounts', link: '/examples/token-accounts' },
            { text: 'Safety guardrails', link: '/examples/guardrails' },
            { text: 'Protocol composition', link: '/examples/composition' },
            { text: '25-use-case matrix', link: '/use-cases' },
          ],
        },
      ],
      '/reference/': [
        {
          text: 'SDK reference',
          items: [
            { text: 'TypeScript', link: '/reference/typescript' },
            { text: 'Rust', link: '/reference/rust' },
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
      copyright: 'OPEN SOURCE · MIT LICENSE · v0.3',
    },
  },
});
