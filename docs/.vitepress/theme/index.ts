import DefaultTheme from 'vitepress/theme';
import { onContentUpdated } from 'vitepress';
import { defineAsyncComponent, defineComponent, h, onMounted } from 'vue';
import AuditFund from './AuditFund.vue';
import CircuitBackground from './CircuitBackground.vue';
import FieldManual from './FieldManual.vue';
import FinalizationGates from './FinalizationGates.vue';
import { enhanceCodeGroups, listenForCodeLang } from './codeLang';
import { useThemeBurst } from './themeBurst';
import './style.css';

// Highlight-and-comment review, on the dev server only: production builds drop this import.
const ReviewComments = import.meta.env.DEV ? defineAsyncComponent(() => import('./ReviewComments.vue')) : null;

const Layout = defineComponent({
  setup() {
    useThemeBurst();
    // Code groups labelled "<Language> · <Part>" get part tabs and a remembered language (codeLang.ts).
    onContentUpdated(enhanceCodeGroups);
    onMounted(() => {
      listenForCodeLang();
      enhanceCodeGroups();
    });
    return () =>
      h(DefaultTheme.Layout, null, {
        'layout-top': () => h(CircuitBackground),
        'home-hero-before': () => h(FieldManual),
        ...(ReviewComments ? { 'layout-bottom': () => h(ReviewComments) } : {}),
      });
  },
});

export default {
  extends: DefaultTheme,
  Layout,
  enhanceApp({ app }: { app: import('vue').App }) {
    // <AuditFund /> in markdown shows the audit donation address when BALLISTA_AUDIT_FUND is set.
    app.component('AuditFund', AuditFund);
    // <FinalizationGates /> draws the finalization checks on the Trust model page.
    app.component('FinalizationGates', FinalizationGates);
  },
};
