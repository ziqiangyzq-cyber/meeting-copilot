import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

export interface TranscriptEvent {
  source: 'system' | 'mic';
  text: string;
  is_final: boolean;
  begin_ms: number;
  end_ms: number;
}

export async function startMeeting(): Promise<void> {
  await invoke('start_meeting');
}

export async function stopMeeting(): Promise<void> {
  await invoke('stop_meeting');
}

export async function onTranscript(
  callback: (evt: TranscriptEvent) => void
): Promise<UnlistenFn> {
  return listen<TranscriptEvent>('transcript', (e) => callback(e.payload));
}

// --- Plan 2 additions ---

export interface MeetingDraft {
  name: string;
  project_ref?: string;
  purpose?: string;
  participants?: string;
  focus_points?: string;
  template_id?: string;
}

export interface MeetingTemplate {
  id: string;
  display_name: string;
  default_purpose: string;
  focus_placeholder: string;
  minutes_schema: string;
}

export async function listTemplates(): Promise<MeetingTemplate[]> {
  return await invoke<MeetingTemplate[]>('list_templates');
}

export interface MaterialProgressEvent {
  file_path: string;
  status: 'started' | 'completed' | 'failed';
  material_id?: string;
  error?: string;
}

export async function createMeeting(draft: MeetingDraft): Promise<string> {
  return await invoke<string>('create_meeting', {
    name: draft.name,
    projectRef: draft.project_ref,
    purpose: draft.purpose,
    participants: draft.participants,
    focusPoints: draft.focus_points,
    templateId: draft.template_id,
  });
}

export async function updateFocusPoints(meetingId: string, focusPoints: string): Promise<void> {
  await invoke('update_focus_points', { meetingId, focusPoints });
}

export async function updateMeetingNotes(meetingId: string, notes: string): Promise<void> {
  await invoke('update_meeting_notes', { meetingId, notes });
}

export async function exportMinutesDocx(markdown: string, savePath: string): Promise<void> {
  await invoke('export_minutes_docx', { markdown, savePath });
}

export async function ingestMaterial(meetingId: string, filePath: string): Promise<string> {
  return await invoke<string>('ingest_material', {
    meetingId,
    filePath,
  });
}

export async function startMeetingWithId(meetingId: string): Promise<void> {
  await invoke('start_meeting', { meetingId });
}

export async function onMaterialProgress(
  callback: (evt: MaterialProgressEvent) => void
): Promise<UnlistenFn> {
  return listen<MaterialProgressEvent>('material_progress', (e) => callback(e.payload));
}

// --- Suggestion events (T11) ---

export async function triggerSuggestion(): Promise<void> {
  await invoke('trigger_suggestion');
}

export async function setSuggestionsEnabled(enabled: boolean): Promise<void> {
  await invoke('set_suggestions_enabled', { enabled });
}

export async function restartMic(): Promise<void> {
  await invoke('restart_mic');
}

export async function translateText(text: string): Promise<string> {
  return await invoke<string>('translate_text', { text });
}

export async function onSuggestionToken(cb: (token: string) => void): Promise<UnlistenFn> {
  return listen<string>('suggestion_token', (e) => cb(e.payload));
}

export async function onSuggestionComplete(cb: () => void): Promise<UnlistenFn> {
  return listen<void>('suggestion_complete', () => cb());
}

export async function onSuggestionError(cb: (err: string) => void): Promise<UnlistenFn> {
  return listen<string>('suggestion_error', (e) => cb(e.payload));
}

export async function listSupportedFiles(folder: string): Promise<string[]> {
  return await invoke<string[]>('list_supported_files', { folder });
}

// --- Minutes events (T2) ---

export async function generateMinutes(meetingId: string): Promise<string> {
  return await invoke<string>('generate_minutes', { meetingId });
}

export async function onMinutesToken(cb: (token: string) => void): Promise<UnlistenFn> {
  return listen<string>('minutes_token', (e) => cb(e.payload));
}

export async function onMinutesComplete(cb: (markdown: string) => void): Promise<UnlistenFn> {
  return listen<string>('minutes_complete', (e) => cb(e.payload));
}

export async function onMinutesError(cb: (err: string) => void): Promise<UnlistenFn> {
  return listen<string>('minutes_error', (e) => cb(e.payload));
}

// --- History (T3 + T4) ---

export interface MeetingSummary {
  id: string;
  name: string;
  project_ref: string | null;
  purpose: string | null;
  started_at: number;
  ended_at: number | null;
  duration_ms: number | null;
  transcript_count: number;
  suggestion_count: number;
  has_minutes: boolean;
  template_id: string | null;
}

export interface MeetingDetail {
  meeting: {
    id: string;
    name: string;
    project_ref: string | null;
    purpose: string | null;
    participants: string | null;
    started_at: number;
    ended_at: number | null;
    audio_path: string | null;
    metadata: string | null;
    focus_points: string | null;
    notes: string | null;
    template_id: string | null;
  };
  transcripts: {
    id: number;
    meeting_id: string;
    speaker: string | null;
    text: string;
    start_ms: number;
    end_ms: number;
    is_final: boolean;
  }[];
  suggestions: {
    id: number;
    meeting_id: string;
    triggered_at: number;
    trigger_type: string | null;
    style: string | null;
    content: string;
    user_action: string | null;
  }[];
  latest_minutes_md: string | null;
  latest_minutes_version: number | null;
}

export async function listMeetings(): Promise<MeetingSummary[]> {
  return await invoke<MeetingSummary[]>('list_meetings');
}

export async function getMeetingDetail(meetingId: string): Promise<MeetingDetail> {
  return await invoke<MeetingDetail>('get_meeting_detail', { meetingId });
}

export async function deleteMeeting(meetingId: string): Promise<void> {
  await invoke('delete_meeting', { meetingId });
}

// --- API Key management (Plan 4 Phase A) ---

export interface KeyStatus {
  aliyun_set: boolean;
  minimax_set: boolean;
}

export async function getApiKeyStatus(): Promise<KeyStatus> {
  return await invoke<KeyStatus>('get_api_key_status');
}

export async function saveApiKeys(aliyun: string, minimax: string): Promise<void> {
  await invoke('save_api_keys', { aliyun, minimax });
}

export async function testAliyunKey(key: string): Promise<void> {
  return await invoke('test_aliyun_key', { key });
}

export async function testMinimaxKey(key: string): Promise<void> {
  return await invoke('test_minimax_key', { key });
}

// --- LLM provider selection (MiniMax / OpenAI-compat) ---

export type LlmProvider = 'minimax' | 'openai_compat';

export interface LlmStatus {
  provider: LlmProvider;
  minimax_set: boolean;
  openai_compat_set: boolean;
  current_base_url: string;
  current_model: string;
}

export async function getLlmStatus(): Promise<LlmStatus> {
  return await invoke<LlmStatus>('get_llm_status');
}

export async function saveAliyunOnly(aliyun: string): Promise<void> {
  await invoke('save_aliyun_only', { aliyun });
}

export async function saveMinimaxOnly(minimax: string): Promise<void> {
  await invoke('save_minimax_only', { minimax });
}

export async function saveOpenaiCompat(
  baseUrl: string,
  model: string,
  apiKey: string
): Promise<void> {
  await invoke('save_openai_compat', { baseUrl, model, apiKey });
}

export async function testOpenaiCompat(
  baseUrl: string,
  model: string,
  apiKey: string
): Promise<void> {
  await invoke('test_openai_compat', { baseUrl, model, apiKey });
}

// --- Mic voice processing toggle (echo cancel + noise suppress + AGC) ---

export async function setVoiceProcessing(enabled: boolean): Promise<void> {
  await invoke('set_voice_processing', { enabled });
}

export async function getVoiceProcessing(): Promise<boolean> {
  return await invoke<boolean>('get_voice_processing');
}
