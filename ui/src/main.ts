// The tokens and base rules of appkit-ui first: the look every app of appkit shares.
import 'appkit-ui/tokens.css';
import 'appkit-ui/base.css';
import { mount } from 'svelte';
import App from './App.svelte';

const target = document.getElementById('app');
if (target === null) throw new Error('#app is missing');
mount(App, { target });
