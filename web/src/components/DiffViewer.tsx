import React, { useState } from 'react';
import { GitDiffResponse } from '../types';

interface DiffViewerProps {
  diffData?: GitDiffResponse | null;
  diff?: GitDiffResponse | null;
  rawDiff?: string | null;
  loading?: boolean;
  onRefresh?: () => void;
  onCommit?: (message: string) => Promise<void>;
}

export const DiffViewer: React.FC<DiffViewerProps> = ({
  diffData: propDiffData,
  diff: aliasDiff,
  rawDiff,
  loading = false,
  onRefresh,
  onCommit,
}) => {
  const diffData = propDiffData || aliasDiff;
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [commitMessage, setCommitMessage] = useState('');
  const [committing, setCommitting] = useState(false);
  const [commitError, setCommitError] = useState<string | null>(null);
  const [commitSuccess, setCommitSuccess] = useState(false);

  const diffText = diffData?.diff || rawDiff || '';
  const filesChanged = diffData?.files_changed || [];
  const insertions = diffData?.insertions ?? 0;
  const deletions = diffData?.deletions ?? 0;

  // Split diff into per-file chunks
  const fileChunks: { filename: string; content: string[] }[] = [];
  const lines = diffText.split('\n');
  let currentFile = '';
  let currentLines: string[] = [];

  for (const line of lines) {
    if (line.startsWith('diff --git')) {
      if (currentFile && currentLines.length > 0) {
        fileChunks.push({ filename: currentFile, content: currentLines });
      }
      currentLines = [line];
      const match = line.match(/b\/(.+)$/);
      currentFile = match ? match[1] : 'unknown';
    } else {
      currentLines.push(line);
    }
  }
  if (currentFile && currentLines.length > 0) {
    fileChunks.push({ filename: currentFile, content: currentLines });
  }

  // Filter if file selected
  const activeChunks = selectedFile
    ? fileChunks.filter((c) => c.filename === selectedFile)
    : fileChunks;

  const handleCommitSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!commitMessage.trim() || !onCommit) return;

    try {
      setCommitting(true);
      setCommitError(null);
      await onCommit(commitMessage.trim());
      setCommitSuccess(true);
      setCommitMessage('');
      setTimeout(() => setCommitSuccess(false), 3000);
    } catch (err: any) {
      setCommitError(err.message || 'Failed to commit changes');
    } finally {
      setCommitting(false);
    }
  };

  return (
    <div className="bg-slate-900 border border-slate-800 rounded-xl overflow-hidden flex flex-col h-full">
      {/* Diff Toolbar */}
      <div className="flex flex-wrap items-center justify-between px-4 py-3 border-b border-slate-800 bg-slate-950/60 gap-2">
        <div className="flex items-center space-x-3">
          <div className="flex items-center space-x-1.5 text-xs font-semibold">
            <span className="text-slate-400">Changed Files:</span>
            <span className="px-2 py-0.5 bg-slate-800 text-slate-200 rounded">
              {filesChanged.length || fileChunks.length}
            </span>
          </div>
          {(insertions > 0 || deletions > 0) && (
            <div className="flex items-center space-x-2 text-xs font-mono">
              <span className="text-emerald-400 font-semibold">+{insertions}</span>
              <span className="text-red-400 font-semibold">-{deletions}</span>
            </div>
          )}
        </div>

        <div className="flex items-center space-x-2">
          {onRefresh && (
            <button
              onClick={onRefresh}
              disabled={loading}
              className="p-1.5 text-xs text-slate-400 hover:text-slate-200 hover:bg-slate-800 rounded transition"
              title="Refresh Diff"
            >
              <svg className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
            </button>
          )}
        </div>
      </div>

      {/* File filter tabs if multiple files */}
      {fileChunks.length > 1 && (
        <div className="flex items-center space-x-1 px-4 py-2 border-b border-slate-800 bg-slate-900/80 overflow-x-auto text-xs">
          <button
            onClick={() => setSelectedFile(null)}
            className={`px-2.5 py-1 rounded font-medium transition ${
              selectedFile === null
                ? 'bg-indigo-600/30 text-indigo-300 border border-indigo-500/40'
                : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800'
            }`}
          >
            All Files ({fileChunks.length})
          </button>
          {fileChunks.map((chunk) => (
            <button
              key={chunk.filename}
              onClick={() => setSelectedFile(chunk.filename)}
              className={`px-2.5 py-1 rounded font-mono font-medium truncate max-w-[200px] transition ${
                selectedFile === chunk.filename
                  ? 'bg-indigo-600/30 text-indigo-300 border border-indigo-500/40'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800'
              }`}
            >
              {chunk.filename}
            </button>
          ))}
        </div>
      )}

      {/* Diff Content Body */}
      <div className="flex-1 overflow-y-auto font-mono text-xs p-4 space-y-4">
        {loading ? (
          <div className="py-12 text-center text-slate-500 font-sans">
            <svg className="w-6 h-6 animate-spin mx-auto mb-2 text-slate-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
            Loading git diff inspection...
          </div>
        ) : !diffText.trim() ? (
          <div className="py-12 text-center text-slate-500 font-sans">
            <svg className="w-8 h-8 text-emerald-500/60 mx-auto mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <p className="text-slate-300 font-medium">Working tree is clean</p>
            <p className="text-xs text-slate-500 mt-1">No unstaged or staged modifications in this workspace.</p>
          </div>
        ) : (
          activeChunks.map((chunk) => (
            <div key={chunk.filename} className="border border-slate-800 rounded-lg overflow-hidden bg-slate-950">
              <div className="px-3 py-2 bg-slate-900 border-b border-slate-800 flex items-center justify-between">
                <span className="font-semibold text-slate-200 flex items-center space-x-2">
                  <svg className="w-3.5 h-3.5 text-slate-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                  </svg>
                  <span>{chunk.filename}</span>
                </span>
              </div>
              <div className="overflow-x-auto p-2">
                {chunk.content.map((line, idx) => {
                  let lineClass = 'text-slate-400';
                  let bgClass = '';

                  if (line.startsWith('+') && !line.startsWith('+++')) {
                    lineClass = 'text-emerald-300';
                    bgClass = 'bg-emerald-950/30';
                  } else if (line.startsWith('-') && !line.startsWith('---')) {
                    lineClass = 'text-red-300';
                    bgClass = 'bg-red-950/30';
                  } else if (line.startsWith('@@')) {
                    lineClass = 'text-sky-400 font-bold';
                    bgClass = 'bg-sky-950/20';
                  } else if (line.startsWith('diff --git') || line.startsWith('index')) {
                    lineClass = 'text-slate-500 font-semibold';
                  }

                  return (
                    <div
                      key={idx}
                      className={`whitespace-pre font-mono px-2 py-0.5 rounded-sm ${bgClass} ${lineClass}`}
                    >
                      {line || ' '}
                    </div>
                  );
                })}
              </div>
            </div>
          ))
        )}
      </div>

      {/* Commit Bar if commit action enabled and diff exists */}
      {onCommit && diffText.trim().length > 0 && (
        <form
          onSubmit={handleCommitSubmit}
          className="p-3 border-t border-slate-800 bg-slate-950/80 flex items-center space-x-2"
        >
          <input
            type="text"
            placeholder="Commit message (e.g. feat: implement rate limiting algorithm)"
            value={commitMessage}
            onChange={(e) => setCommitMessage(e.target.value)}
            disabled={committing}
            className="flex-1 bg-slate-900 border border-slate-700 rounded-lg px-3 py-1.5 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-indigo-500 font-sans"
          />
          <button
            type="submit"
            disabled={committing || !commitMessage.trim()}
            className="px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 disabled:opacity-50 text-white text-xs font-semibold rounded-lg transition flex items-center space-x-1 shadow-sm font-sans"
          >
            {committing ? 'Committing...' : 'Commit Changes'}
          </button>
          {commitSuccess && <span className="text-xs text-emerald-400 font-sans">Committed!</span>}
          {commitError && <span className="text-xs text-red-400 font-sans">{commitError}</span>}
        </form>
      )}
    </div>
  );
};
