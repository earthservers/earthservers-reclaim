import { useState, useEffect, useCallback, useRef } from 'react';
import { RightDockPanel, RIGHT_DOCK_WIDTH_WIDE } from '../lib/rightDock';

interface Note {
  id: string;
  title: string;
  content: string;
  createdAt: string;
  updatedAt: string;
}

interface NotesPluginProps {
  isOpen: boolean;
  onClose: () => void;
}

// Simple localStorage-based notes (could be extended to use Tauri fs)
const NOTES_KEY = 'reclaim_notes';

function loadNotes(): Note[] {
  try {
    const stored = localStorage.getItem(NOTES_KEY);
    return stored ? JSON.parse(stored) : [];
  } catch {
    return [];
  }
}

function saveNotes(notes: Note[]) {
  localStorage.setItem(NOTES_KEY, JSON.stringify(notes));
}

// Convert markdown to HTML for display
function markdownToHtml(text: string): string {
  if (!text) return '';

  let html = text
    // Escape HTML first
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    // Headers
    .replace(/^### (.+)$/gm, '<h3 class="text-lg font-semibold mt-3 mb-1">$1</h3>')
    .replace(/^## (.+)$/gm, '<h2 class="text-xl font-semibold mt-4 mb-2">$1</h2>')
    .replace(/^# (.+)$/gm, '<h1 class="text-2xl font-bold mt-4 mb-2">$1</h1>')
    // Bold **text** or __text__
    .replace(/\*\*(.+?)\*\*/g, '<strong class="font-bold">$1</strong>')
    .replace(/__(.+?)__/g, '<strong class="font-bold">$1</strong>')
    // Italic *text* or _text_
    .replace(/(?<!\*)\*([^*\n]+)\*(?!\*)/g, '<em class="italic">$1</em>')
    .replace(/(?<!_)_([^_\n]+)_(?!_)/g, '<em class="italic">$1</em>')
    // Strikethrough ~~text~~
    .replace(/~~(.+?)~~/g, '<del class="line-through text-gray-500">$1</del>')
    // Code `text`
    .replace(/`([^`]+)`/g, '<code class="px-1 py-0.5 bg-gray-700 rounded text-[var(--primary-color)] font-mono text-sm">$1</code>')
    // Blockquotes > text
    .replace(/^&gt; (.+)$/gm, '<div class="border-l-4 border-[var(--primary-color)] pl-3 py-1 my-2 text-gray-400 italic">$1</div>')
    // Unordered lists - item
    .replace(/^- (.+)$/gm, '<div class="ml-4 flex gap-2"><span class="text-[var(--primary-color)]">•</span><span>$1</span></div>')
    // Ordered lists 1. item
    .replace(/^(\d+)\. (.+)$/gm, '<div class="ml-4 flex gap-2"><span class="text-[var(--primary-color)]">$1.</span><span>$2</span></div>')
    // Horizontal rule ---
    .replace(/^---$/gm, '<hr class="border-gray-700 my-4" />')
    // Links [text](url)
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" class="text-[var(--primary-color)] hover:underline" target="_blank" rel="noopener">$1</a>')
    // Line breaks
    .replace(/\n/g, '<br />');

  return html;
}

export function NotesPlugin({ isOpen, onClose }: NotesPluginProps) {
  const [notes, setNotes] = useState<Note[]>([]);
  const [activeNoteId, setActiveNoteId] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState(false);
  const [viewMode, setViewMode] = useState<'view' | 'edit'>('view');
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const editorRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Load notes on mount
  useEffect(() => {
    setNotes(loadNotes());
  }, []);

  const activeNote = notes.find(n => n.id === activeNoteId);

  // Sync contenteditable with note content when switching notes or modes
  useEffect(() => {
    if (editorRef.current && activeNote && viewMode === 'view') {
      editorRef.current.innerHTML = markdownToHtml(activeNote.content);
    }
  }, [activeNoteId, viewMode]);

  // Create new note
  const createNote = useCallback(() => {
    const newNote: Note = {
      id: Date.now().toString(),
      title: 'Untitled Note',
      content: '',
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
    const updated = [newNote, ...notes];
    setNotes(updated);
    saveNotes(updated);
    setActiveNoteId(newNote.id);
    setViewMode('edit'); // Start in edit mode for new notes
  }, [notes]);

  // Update note content
  const updateNoteContent = useCallback((content: string) => {
    if (!activeNoteId) return;
    const updated = notes.map(n =>
      n.id === activeNoteId
        ? { ...n, content, updatedAt: new Date().toISOString() }
        : n
    );
    setNotes(updated);
    saveNotes(updated);
  }, [activeNoteId, notes]);

  // Update note title
  const updateNoteTitle = useCallback((title: string) => {
    if (!activeNoteId) return;
    const updated = notes.map(n =>
      n.id === activeNoteId
        ? { ...n, title, updatedAt: new Date().toISOString() }
        : n
    );
    setNotes(updated);
    saveNotes(updated);
  }, [activeNoteId, notes]);

  // Delete note
  const deleteNote = useCallback((id: string) => {
    const updated = notes.filter(n => n.id !== id);
    setNotes(updated);
    saveNotes(updated);
    if (activeNoteId === id) {
      setActiveNoteId(updated[0]?.id || null);
    }
  }, [notes, activeNoteId]);

  // Export note as .txt (with raw markdown)
  const exportNote = useCallback(() => {
    if (!activeNote) return;

    // Format content with title - keeps raw markdown
    const content = `${activeNote.title}\n${'='.repeat(activeNote.title.length)}\n\n${activeNote.content}`;

    // Create blob and download
    const blob = new Blob([content], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${activeNote.title.replace(/[^a-z0-9]/gi, '_').toLowerCase()}.txt`;
    a.click();
    URL.revokeObjectURL(url);
  }, [activeNote]);

  // Apply formatting to selected text
  const applyFormat = useCallback((format: 'bold' | 'italic' | 'heading' | 'list' | 'quote') => {
    if (viewMode === 'edit' && textareaRef.current && activeNote) {
      // Edit mode - modify markdown directly
      const textarea = textareaRef.current;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;
      const selected = activeNote.content.substring(start, end);

      let replacement = '';
      let cursorOffset = 0;
      switch (format) {
        case 'bold':
          replacement = `**${selected}**`;
          cursorOffset = selected ? 0 : 2;
          break;
        case 'italic':
          replacement = `_${selected}_`;
          cursorOffset = selected ? 0 : 1;
          break;
        case 'heading':
          replacement = `# ${selected}`;
          cursorOffset = 0;
          break;
        case 'list':
          replacement = selected ? selected.split('\n').map(line => `- ${line}`).join('\n') : '- ';
          cursorOffset = 0;
          break;
        case 'quote':
          replacement = selected ? selected.split('\n').map(line => `> ${line}`).join('\n') : '> ';
          cursorOffset = 0;
          break;
      }

      const newContent = activeNote.content.substring(0, start) + replacement + activeNote.content.substring(end);
      updateNoteContent(newContent);

      // Restore cursor position
      setTimeout(() => {
        textarea.focus();
        const newPos = start + replacement.length - cursorOffset;
        textarea.setSelectionRange(newPos, newPos);
      }, 0);
    } else if (viewMode === 'view' && activeNote) {
      // View mode - apply formatting to selected text
      const selection = window.getSelection();
      if (!selection || selection.rangeCount === 0) return;

      const selectedText = selection.toString();
      const content = activeNote.content;
      let newContent = content;

      if (selectedText) {
        const formatWrap = {
          bold: `**${selectedText}**`,
          italic: `_${selectedText}_`,
          heading: `\n# ${selectedText}\n`,
          list: `\n- ${selectedText}\n`,
          quote: `\n> ${selectedText}\n`,
        };

        const index = content.lastIndexOf(selectedText);
        if (index !== -1) {
          newContent = content.substring(0, index) + formatWrap[format] + content.substring(index + selectedText.length);
        }
      } else {
        const formatAdd = {
          bold: '****',
          italic: '__',
          heading: '\n# ',
          list: '\n- ',
          quote: '\n> ',
        };
        newContent = content + formatAdd[format];
      }

      updateNoteContent(newContent);

      if (editorRef.current) {
        editorRef.current.innerHTML = markdownToHtml(newContent);
        const range = document.createRange();
        range.selectNodeContents(editorRef.current);
        range.collapse(false);
        selection.removeAllRanges();
        selection.addRange(range);
      }
    }
  }, [viewMode, activeNote, updateNoteContent]);

  // Handle input in view mode
  const handleViewInput = useCallback(() => {
    if (!editorRef.current || !activeNote) return;
    const text = editorRef.current.innerText || '';
    if (text !== activeNote.content) {
      updateNoteContent(text);
    }
  }, [activeNote, updateNoteContent]);

  // Handle keyboard shortcuts
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey) {
      switch (e.key.toLowerCase()) {
        case 'b':
          e.preventDefault();
          applyFormat('bold');
          break;
        case 'i':
          e.preventDefault();
          applyFormat('italic');
          break;
      }
    }
  }, [applyFormat]);

  return (
    <RightDockPanel id="notes" open={isOpen} width={RIGHT_DOCK_WIDTH_WIDE} title="Notes" onClose={onClose}>
      <div className="flex h-full gap-2">
        {/* Collapsible Notes List */}
        <div className={`flex-shrink-0 flex flex-col gap-2 transition-all duration-200 ${sidebarCollapsed ? 'w-8' : 'w-36'}`}>
          {/* Collapse/Expand Toggle */}
          <button
            onClick={() => setSidebarCollapsed(!sidebarCollapsed)}
            className="flex items-center justify-center p-1.5 hover:bg-gray-700/50 rounded transition-colors text-gray-400 hover:text-white"
            title={sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
          >
            <svg
              className={`w-4 h-4 transition-transform duration-200 ${sidebarCollapsed ? 'rotate-180' : ''}`}
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 19l-7-7 7-7m8 14l-7-7 7-7" />
            </svg>
          </button>

          {!sidebarCollapsed && (
            <>
              <button
                onClick={createNote}
                className="w-full px-2 py-1.5 bg-[var(--primary-color)] text-white rounded-lg text-xs font-medium hover:opacity-90 transition-opacity flex items-center justify-center gap-1"
              >
                <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                </svg>
                New
              </button>

              <div className="flex-1 overflow-y-auto space-y-1">
                {notes.map(note => (
                  <button
                    key={note.id}
                    onClick={() => {
                      setActiveNoteId(note.id);
                      setViewMode('view');
                    }}
                    className={`w-full text-left px-2 py-1.5 rounded text-xs transition-colors group ${
                      activeNoteId === note.id
                        ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]'
                        : 'hover:bg-gray-700/50 text-[var(--text-muted-color)]'
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="truncate flex-1">{note.title}</span>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          deleteNote(note.id);
                        }}
                        className="opacity-0 group-hover:opacity-100 p-0.5 hover:text-red-400 transition-all"
                      >
                        <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                        </svg>
                      </button>
                    </div>
                    <span className="text-[10px] text-gray-500 block truncate">
                      {new Date(note.updatedAt).toLocaleDateString()}
                    </span>
                  </button>
                ))}

                {notes.length === 0 && (
                  <p className="text-[10px] text-gray-500 text-center py-4">
                    No notes
                  </p>
                )}
              </div>
            </>
          )}

          {sidebarCollapsed && (
            <button
              onClick={createNote}
              className="p-1.5 bg-[var(--primary-color)] text-white rounded hover:opacity-90 transition-opacity"
              title="New Note"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
              </svg>
            </button>
          )}
        </div>

        {/* Editor */}
        <div className="flex-1 flex flex-col min-w-0 border-l border-gray-700/50 pl-2">
          {activeNote ? (
            <>
              {/* Title */}
              <div className="mb-2">
                {editingTitle ? (
                  <input
                    type="text"
                    value={activeNote.title}
                    onChange={(e) => updateNoteTitle(e.target.value)}
                    onBlur={() => setEditingTitle(false)}
                    onKeyDown={(e) => e.key === 'Enter' && setEditingTitle(false)}
                    autoFocus
                    className="w-full px-2 py-1 bg-transparent border-b border-[var(--primary-color)] text-[var(--text-color)] font-semibold text-sm focus:outline-none"
                  />
                ) : (
                  <h4
                    onClick={() => setEditingTitle(true)}
                    className="font-semibold text-[var(--text-color)] text-sm cursor-pointer hover:text-[var(--primary-color)] transition-colors truncate"
                  >
                    {activeNote.title}
                  </h4>
                )}
              </div>

              {/* Toolbar */}
              <div className="flex items-center gap-1 mb-2 pb-2 border-b border-gray-700/50">
                {/* View/Edit Toggle Button */}
                <button
                  onClick={() => setViewMode(viewMode === 'view' ? 'edit' : 'view')}
                  className="px-2 py-1 rounded text-xs font-medium bg-gray-700 hover:bg-gray-600 text-gray-200 transition-colors flex items-center gap-1"
                  title={viewMode === 'view' ? 'Switch to Edit mode' : 'Switch to View mode'}
                >
                  {viewMode === 'view' ? (
                    <>
                      <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                      </svg>
                      Edit
                    </>
                  ) : (
                    <>
                      <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                      </svg>
                      View
                    </>
                  )}
                </button>

                <div className="w-px h-5 bg-gray-700 mx-1" />

                {/* Formatting buttons */}
                <button
                  onClick={() => applyFormat('bold')}
                  className="p-1 hover:bg-gray-700/50 rounded text-[var(--text-muted-color)] hover:text-[var(--text-color)] transition-colors"
                  title="Bold (Ctrl+B)"
                >
                  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" strokeWidth={3} viewBox="0 0 24 24">
                    <path d="M6 4h8a4 4 0 014 4 4 4 0 01-4 4H6z" />
                    <path d="M6 12h9a4 4 0 014 4 4 4 0 01-4 4H6z" />
                  </svg>
                </button>
                <button
                  onClick={() => applyFormat('italic')}
                  className="p-1 hover:bg-gray-700/50 rounded text-[var(--text-muted-color)] hover:text-[var(--text-color)] transition-colors"
                  title="Italic (Ctrl+I)"
                >
                  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
                    <path d="M19 4h-9M14 20H5M15 4L9 20" />
                  </svg>
                </button>
                <button
                  onClick={() => applyFormat('heading')}
                  className="p-1 hover:bg-gray-700/50 rounded text-[var(--text-muted-color)] hover:text-[var(--text-color)] transition-colors"
                  title="Heading"
                >
                  <span className="text-xs font-bold">H</span>
                </button>
                <button
                  onClick={() => applyFormat('list')}
                  className="p-1 hover:bg-gray-700/50 rounded text-[var(--text-muted-color)] hover:text-[var(--text-color)] transition-colors"
                  title="List"
                >
                  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
                    <path strokeLinecap="round" d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" />
                  </svg>
                </button>
                <button
                  onClick={() => applyFormat('quote')}
                  className="p-1 hover:bg-gray-700/50 rounded text-[var(--text-muted-color)] hover:text-[var(--text-color)] transition-colors"
                  title="Quote"
                >
                  <svg className="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M6 17h3l2-4V7H5v6h3zm8 0h3l2-4V7h-6v6h3z" />
                  </svg>
                </button>

                <div className="flex-1" />
                <button
                  onClick={exportNote}
                  className="p-1 hover:bg-gray-700/50 rounded text-[var(--text-muted-color)] hover:text-[var(--text-color)] transition-colors"
                  title="Export as .txt"
                >
                  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                  </svg>
                </button>
              </div>

              {/* Content Area */}
              {viewMode === 'edit' ? (
                <textarea
                  ref={textareaRef}
                  className="flex-1 w-full p-2 bg-[var(--bg-color)] border border-gray-700 rounded-lg text-[var(--text-color)] placeholder-gray-500 focus:outline-none focus:border-[var(--primary-color)] resize-none font-mono text-xs"
                  value={activeNote.content}
                  onChange={(e) => updateNoteContent(e.target.value)}
                  onKeyDown={handleKeyDown}
                  placeholder="Write with markdown:
# Heading
**bold** _italic_
- list item
> quote"
                />
              ) : (
                <div
                  ref={editorRef}
                  contentEditable
                  suppressContentEditableWarning
                  onInput={handleViewInput}
                  onKeyDown={handleKeyDown}
                  className="flex-1 w-full p-2 bg-[var(--bg-color)] border border-gray-700 rounded-lg text-[var(--text-color)] focus:outline-none focus:border-[var(--primary-color)] overflow-y-auto text-sm leading-relaxed"
                  style={{ minHeight: '100px' }}
                  dangerouslySetInnerHTML={{ __html: markdownToHtml(activeNote.content) || '<span class="text-gray-500">Start writing...</span>' }}
                />
              )}

              {/* Footer */}
              <div className="mt-1 text-[10px] text-gray-500 flex justify-between">
                <span>{activeNote.content.length} chars</span>
                <span>{new Date(activeNote.updatedAt).toLocaleTimeString()}</span>
              </div>
            </>
          ) : (
            <div className="flex-1 flex items-center justify-center text-[var(--text-muted-color)]">
              <div className="text-center">
                <svg className="w-10 h-10 mx-auto mb-2 opacity-50" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                </svg>
                <p className="text-xs">Select or create a note</p>
              </div>
            </div>
          )}
        </div>
      </div>
    </RightDockPanel>
  );
}

export default NotesPlugin;
