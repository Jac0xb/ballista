import { nextTick, provide } from 'vue';
import { useData } from 'vitepress';

// Switching between light and dark: the new theme is revealed through a circle that grows from the
// switch until it covers the whole screen. Without View Transitions, or with reduced motion, the
// theme simply switches.

const DURATION = 650;

export function useThemeBurst() {
  const { isDark } = useData();
  provide('toggle-appearance', async (event?: MouseEvent) => {
    const root = document.documentElement;
    // The circle comes out of where the pointer clicked. A keyboard toggle has no pointer position
    // (`detail` is 0), so it comes out of the switch's knob: the one that was clicked, else the
    // visible one.
    let x: number;
    let y: number;
    if (event && event.detail > 0) {
      x = event.clientX;
      y = event.clientY;
    } else {
      const clicked = (event?.currentTarget ?? event?.target) as Element | null | undefined;
      const knob =
        clicked?.closest?.('.VPSwitchAppearance')?.querySelector('.check') ??
        [...document.querySelectorAll('.VPSwitchAppearance .check')].find((element) => element.getBoundingClientRect().width > 0);
      const box = knob?.getBoundingClientRect();
      x = box ? box.left + box.width / 2 : window.innerWidth - 60;
      y = box ? box.top + box.height / 2 : 38;
    }
    const toDark = !isDark.value;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches || typeof document.startViewTransition !== 'function') {
      isDark.value = toDark;
      return;
    }
    // Far enough to pass the farthest corner, with a margin so the edge never lingers on screen.
    const reach = Math.hypot(Math.max(x, window.innerWidth - x), Math.max(y, window.innerHeight - y)) + 40;
    root.classList.add('theme-burst');
    const transition = document.startViewTransition(async () => {
      isDark.value = toDark;
      await nextTick();
    });
    try {
      await transition.ready;
    } catch {
      root.classList.remove('theme-burst');
      return;
    }
    root.animate(
      { clipPath: [`circle(0px at ${x}px ${y}px)`, `circle(${reach}px at ${x}px ${y}px)`] },
      { duration: DURATION, easing: 'cubic-bezier(0.45, 0, 0.25, 1)', fill: 'forwards', pseudoElement: '::view-transition-new(root)' },
    );
    transition.finished.finally(() => root.classList.remove('theme-burst'));
  });
}
