import { useEffect, useRef } from 'react';
import { TranscriptEvent } from '../lib/tauri-bridge';

interface Props {
  items: TranscriptEvent[];
}

export function TranscriptView({ items }: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [items.length]);

  return (
    <div className="h-96 overflow-y-auto bg-gray-50 rounded p-4 font-mono text-sm space-y-2 border border-gray-200">
      {items.length === 0 && (
        <div className="text-gray-400 italic">等待会议开始...</div>
      )}
      {items.map((item, i) => (
        <div
          key={i}
          className={`flex gap-2 ${
            item.source === 'system' ? 'text-blue-700' : 'text-green-700'
          }`}
        >
          <span className="font-bold shrink-0">
            {item.source === 'system' ? '对方' : '我'}
          </span>
          <span>{item.text}</span>
          {!item.is_final && <span className="text-gray-400">…</span>}
        </div>
      ))}
      <div ref={bottomRef} />
    </div>
  );
}
