import { Setup } from './pages/Setup';
import { Floating } from './pages/Floating';
import './App.css';

function getWindowMode(): 'main' | 'floating' {
  const params = new URLSearchParams(window.location.search);
  return params.get('window') === 'floating' ? 'floating' : 'main';
}

export default function App() {
  const mode = getWindowMode();
  if (mode === 'floating') return <Floating />;
  return <Setup />;
}
