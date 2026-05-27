import { useState } from 'react';
import {
  MeetingDraft,
  createMeeting,
  startMeetingWithId,
} from '../lib/tauri-bridge';
import { MeetingForm } from '../components/MeetingForm';
import { MaterialFolderPicker } from '../components/MaterialFolderPicker';
import { MeetingView } from './MeetingView';
import { MinutesView } from './MinutesView';

type Stage = 'form' | 'materials' | 'starting' | 'started' | 'minutes';

export function Setup() {
  const [stage, setStage] = useState<Stage>('form');
  const [meetingId, setMeetingId] = useState<string | null>(null);
  const [meetingName, setMeetingName] = useState<string>('');
  const [error, setError] = useState<string | null>(null);
  const [materialsReady, setMaterialsReady] = useState(false);

  const handleCreate = async (draft: MeetingDraft) => {
    setError(null);
    try {
      const id = await createMeeting(draft);
      setMeetingId(id);
      setMeetingName(draft.name);
      setMaterialsReady(true); // ready immediately — no files yet
      setStage('materials');
    } catch (e) {
      setError(String(e));
    }
  };

  const handleStart = async () => {
    if (!meetingId) return;
    setStage('starting');
    setError(null);
    try {
      console.log('[Setup] calling startMeetingWithId', meetingId);
      await startMeetingWithId(meetingId);
      console.log('[Setup] start_meeting OK');
      setStage('started');
    } catch (e) {
      console.error('[Setup] handleStart error:', e);
      setError(String(e));
      setStage('materials');
    }
  };

  if (stage === 'minutes' && meetingId) {
    return (
      <MinutesView
        meetingId={meetingId}
        meetingName={meetingName}
        onBack={() => {
          setStage('form');
          setMeetingId(null);
          setMeetingName('');
          setMaterialsReady(false);
        }}
      />
    );
  }

  if (stage === 'started') {
    return <MeetingView onEnd={() => {
      setStage('minutes');
    }} />;
  }

  return (
    <div className="min-h-screen bg-white p-8 max-w-2xl mx-auto">
      <h1 className="text-2xl font-bold mb-2">会议助理</h1>
      <p className="text-gray-600 mb-6 text-sm">EFC 会议 AI 助理 — 实时转写 + 智能建议</p>

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 text-red-800 rounded text-sm">
          错误:{error}
        </div>
      )}

      {stage === 'form' && (
        <section>
          <h2 className="text-lg font-semibold mb-4">1. 新建会议</h2>
          <MeetingForm onSubmit={handleCreate} />
        </section>
      )}

      {stage === 'materials' && meetingId && (
        <section className="space-y-6">
          <div className="p-3 bg-blue-50 border border-blue-200 rounded text-sm text-blue-800">
            会议已创建:{meetingId.slice(0, 8)}...
          </div>

          <div>
            <h2 className="text-lg font-semibold mb-4">2. 上传会议资料(可选)</h2>
            <MaterialFolderPicker
              meetingId={meetingId}
              onAllReady={(anyIngested) => {
                // anyIngested is informational; always allow proceeding once files settle
                setMaterialsReady(true);
                void anyIngested;
              }}
            />
          </div>

          <div className="pt-4 border-t">
            <button
              onClick={handleStart}
              disabled={!materialsReady}
              className="px-6 py-2 bg-green-600 text-white rounded hover:bg-green-700 disabled:bg-gray-400 transition"
            >
              开始会议 ▶
            </button>
            <p className="text-xs text-gray-500 mt-2">
              {!materialsReady
                ? '等待资料索引完成...'
                : '可以直接开始,或继续拖入更多资料'}
            </p>
          </div>
        </section>
      )}

      {stage === 'starting' && (
        <section className="text-center py-12 text-gray-600">
          🔄 正在启动会议(请求屏幕录制 + 麦克风权限)...
        </section>
      )}

    </div>
  );
}
