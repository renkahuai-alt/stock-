import { writable } from 'svelte/store';
import type { GetTargetNoteRequest, TargetNotePayload } from '../types/contracts';
import { getTargetNote, saveTargetNote } from '../services/commands';

export const noteStore = writable<TargetNotePayload>({
  targetType: 'index',
  targetId: '',
  content: '',
  updatedAt: null,
});

export function setTargetNote(note: TargetNotePayload): void {
  noteStore.set(note);
}

export async function loadTargetNote(payload: GetTargetNoteRequest): Promise<void> {
  noteStore.set(await getTargetNote(payload));
}

export async function persistNote(note: TargetNotePayload): Promise<void> {
  noteStore.set(await saveTargetNote(note));
}
