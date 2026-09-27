import DefaultTheme from 'vitepress/theme';
import { defineAsyncComponent, defineComponent, h } from 'vue';
import CircuitBackground from './CircuitBackground.vue';
import FieldManual from './FieldManual.vue';
import { useThemeBurst } from './themeBurst';
import './style.css';

// Highlight-and-comment review, on the dev server only: production builds drop this import.
const ReviewComments = import.meta.env.DEV ? defineAsyncComponent(() => import('./ReviewComments.vue')) : null;

const Layout = defineComponent({
  setup() {
    useThemeBurst();
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
};
