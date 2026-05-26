import { useEffect, useState } from 'react';
import {
  TranscriptEvent,
  onTranscript,
  stopMeeting,
  hideFloating,
} from '../lib/tauri-bridge';

export function Floating() {
  const [items, setItems] = useState<TranscriptEvent[]>([]);
  const [isAsrOk] = useState(true);

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
    return () => unlisten?.();
  }, []);

  const handleStop = async () => {
    try {
      await stopMeeting();
    } catch (e) {
      console.error('stop_meeting failed', e);
    }
    await hideFloating();
    // T11 will navigate main back to setup form; for now, just hide floating
  };

  return (
    <div className="h-screen w-screen flex flex-col bg-black/80 backdrop-blur text-white text-xs font-mono select-none">
      {/* Top bar */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-white/10">
        <span className={isAsrOk ? 'text-green-400' : 'text-orange-400'}>●</span>
        <span className="text-white/70">ASR</span>
        <div className="flex-1" />
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

      {/* Bottom suggestion placeholder (T11 fills) */}
      <div className="border-t border-white/10 px-3 py-2 text-white/50 text-[10px]">
        建议区(T11)
      </div>
    </div>
  );
}
