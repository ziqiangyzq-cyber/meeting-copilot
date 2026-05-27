import { useEffect, useState } from 'react';
import { Setup } from './pages/Setup';
import { Settings } from './pages/Settings';
import { getApiKeyStatus } from './lib/tauri-bridge';
import './App.css';

export default function App() {
  const [keysReady, setKeysReady] = useState<boolean | null>(null);
  const [showSettings, setShowSettings] = useState(false);

  useEffect(() => {
    getApiKeyStatus()
      .then((s) => {
        setKeysReady(s.aliyun_set && s.minimax_set);
      })
      .catch(() => setKeysReady(false));
  }, []);

  if (keysReady === null) {
    return (
      <div className="min-h-screen flex items-center justify-center text-gray-400">
        加载中...
      </div>
    );
  }

  if (!keysReady || showSettings) {
    return (
      <Settings
        isFirstLaunch={!keysReady}
        onBack={() => setShowSettings(false)}
        onSaved={() => {
          setKeysReady(true);
          setShowSettings(false);
        }}
      />
    );
  }

  return <Setup onOpenSettings={() => setShowSettings(true)} />;
}
