import DefaultTheme from 'vitepress/theme';
import { h } from 'vue';
import FieldManual from './FieldManual.vue';
import './style.css';

export default {
  extends: DefaultTheme,
  Layout: () => h(DefaultTheme.Layout, null, {
    'home-hero-before': () => h(FieldManual),
  }),
};
