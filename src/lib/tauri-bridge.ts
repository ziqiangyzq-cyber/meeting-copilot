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
  });
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
