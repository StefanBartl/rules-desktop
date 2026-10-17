// The tokens and base rules of appkit-ui first: the look every app of appkit shares.
import 'appkit-ui/tokens.css';
import 'appkit-ui/base.css';
import { takePairingCode } from 'appkit-ui/pair';
import { installTooltips } from 'appkit-ui/tooltip';
import { mount } from 'svelte';
import App from './App.svelte';
import { Hub } from './lib/api';
import { App as AppState, type Env } from './lib/app.svelte';

const target = document.getElementById('app');
if (target === null) throw new Error('#app is missing');

// The code leaves the address bar before anything else happens.
const code = takePairingCode(window.location, window.history);

const fetchFn: typeof fetch = (input, init) => window.fetch(input, init);
const env: Env = {
  hub: new Hub(fetchFn),
  session: window.sessionStorage,
  prefs: window.localStorage,
  preferredLanguages: navigator.languages,
  now: () => Date.now(),
  fetchFn,
  sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
};

const state = new AppState(env);
installTooltips(document, window, { regionLabel: () => 'hints' });
mount(App, { target, props: { app: state, code } });

// A link of `rules-hub open` pasted into a tab that is already open changes only the fragment: no reload. The
// page starts again with the new code; what ran before is stopped first.
window.addEventListener('hashchange', () => {
  const later = takePairingCode(window.location, window.history);
  if (later) {
    state.stop();
    void state.start(later);
  }
});
