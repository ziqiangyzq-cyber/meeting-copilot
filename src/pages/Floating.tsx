import { useEffect, useState } from 'react';
import { register, unregister } from '@tauri-apps/plugin-global-shortcut';
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';
import {
  TranscriptEvent,
  onTranscript,
  onSuggestionToken,
  onSuggestionComplete,
  stopMeeting,
  hideFloating,
  triggerSuggestion,
} from '../lib/tauri-bridge';
import { SuggestionCard } from '../components/SuggestionCard';

const SHORTCUT = 'CommandOrControl+Shift+M';

export function Floating() {
  console.log('[Floating] component mounting, location:', window.location.href);
  const [items, setItems] = useState<TranscriptEvent[]>([]);
  const [isAsrOk] = useState(true);
  const [notifyEnabled, setNotifyEnabled] = useState<boolean>(() => {
    const stored = localStorage.getItem('notifyEnabled');
    return stored === null ? true : stored === 'true';
  });

  useEffect(() => {
    localStorage.setItem('notifyEnabled', String(notifyEnabled));
  }, [notifyEnabled]);

  // Request macOS notification permission when enabled
  useEffect(() => {
    if (!notifyEnabled) return;
    (async () => {
      try {
        let granted = await isPermissionGranted();
        if (!granted) {
          const perm = await requestPermission();
          granted = perm === 'granted';
        }
        if (!granted) {
          setNotifyEnabled(false);
        }
      } catch (e) {
        console.error('notification permission check failed', e);
      }
    })();
  }, [notifyEnabled]);

  // Listen for suggestion stream and fire notification on completion
  useEffect(() => {
    const accumRef = { current: '' };
    let unlistenToken: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;

    onSuggestionToken((token) => {
      accumRef.current += token;
    }).then((fn) => {
      unlistenToken = fn;
    });

    onSuggestionComplete(() => {
      const text = accumRef.current.trim();
      if (text) {
        // Read from localStorage to avoid stale closure
        const enabled = localStorage.getItem('notifyEnabled') !== 'false';
        if (enabled) {
          try {
            sendNotification({
              title: '会议助理 — 新建议',
              body: text.slice(0, 120),
            });
          } catch (e) {
            console.error('sendNotification failed', e);
          }
        }
      }
      accumRef.current = '';
    }).then((fn) => {
      unlistenDone = fn;
    });

    return () => {
      unlistenToken?.();
      unlistenDone?.();
    };
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    onTranscript((evt) => {
      setItems((prev) => {
        // Replace last partial from same source, or append
        const last = prev[prev.length - 1];
        if (last && !last.is_final && last.source === evt.source) {
          const next = prev.slice(0, -1);
          next.push(evt);
          // Keep only last 8 items in floating window
          return next.slice(-8);
        }
        return [...prev, evt].slice(-8);
      });
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      console.log('[Floating] component unmounting');
      unlisten?.();
    };
  }, []);

  // Register global shortcut Cmd+Shift+M → trigger_suggestion
  useEffect(() => {
    let registered = false;

    (async () => {
      try {
        await register(SHORTCUT, async (event) => {
          if (event.state === 'Pressed') {
            try {
              await triggerSuggestion();
            } catch (e) {
              console.error('trigger_suggestion via shortcut failed', e);
            }
          }
        });
        registered = true;
      } catch (e) {
        console.error('register shortcut failed', e);
      }
    })();

    return () => {
      if (registered) {
        unregister(SHORTCUT).catch((e) =>
          console.error('unregister shortcut failed', e)
        );
      }
    };
  }, []);

  const handleStop = async () => {
    try {
      await stopMeeting();
    } catch (e) {
      console.error('stop_meeting failed', e);
    }
    await hideFloating();
  };

  return (
    <div className="h-screen w-screen flex flex-col bg-black/80 backdrop-blur text-white text-xs font-mono select-none">
      {/* Top bar */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-white/10">
        <span className={isAsrOk ? 'text-green-400' : 'text-orange-400'}>●</span>
        <span className="text-white/70">ASR</span>
        <div className="flex-1" />
        <button
          onClick={() => setNotifyEnabled(!notifyEnabled)}
          className="text-base hover:opacity-80"
          title={notifyEnabled ? '通知开 (点击关闭)' : '通知关 (点击开启)'}
        >
          {notifyEnabled ? '🔔' : '🔕'}
        </button>
        <button
          onClick={handleStop}
          className="px-2 py-0.5 bg-red-600/80 hover:bg-red-600 rounded text-[10px]"
        >
          结束
        </button>
      </div>

      {/* Transcript scroll */}
      <div className="flex-1 overflow-y-auto px-3 py-2 space-y-1">
        {items.length === 0 && (
          <div className="text-white/40 italic">等待转写...</div>
        )}
        {items.map((item, i) => (
          <div
            key={i}
            className={`flex gap-1.5 ${
              item.source === 'system' ? 'text-blue-300' : 'text-green-300'
            }`}
          >
            <span className="font-bold shrink-0">
              {item.source === 'system' ? '对方' : '我'}
            </span>
            <span className="break-words">
              {item.text}
              {!item.is_final && <span className="text-white/40">…</span>}
            </span>
          </div>
        ))}
      </div>

      {/* Suggestion card (T11) */}
      <div className="border-t border-white/10 max-h-[180px] overflow-y-auto">
        <SuggestionCard />
      </div>
    </div>
  );
}
