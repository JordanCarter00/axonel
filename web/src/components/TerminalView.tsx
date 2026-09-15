import React, { useState, useEffect, useRef } from 'react';
import { TaskTerminal } from '../types';
import { api } from '../services/api';

interface TerminalViewProps {
  taskId: string;
  isTaskActive?: boolean;
}

export const TerminalView: React.FC<TerminalViewProps> = ({ taskId, isTaskActive = false }) => {
  const [terminal, setTerminal] = useState<TaskTerminal | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  const [autoScroll, setAutoScroll] = useState<boolean>(true);
  const [copied, setCopied] = useState<boolean>(false);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  const fetchTerminal = async () => {
    try {
      const data = await api.getTaskTerminal(taskId);
      setTerminal(data);
      setError(null);
    } catch (err: any) {
      setError(err.message || 'Failed to fetch terminal output');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchTerminal();

    // Poll while active or until completed
    let interval: any = null;
    if (isTaskActive || (terminal && !terminal.is_completed)) {
      interval = setInterval(fetchTerminal, 1500);
    }

    return () => {
      if (interval) clearInterval(interval);
    };
  }, [taskId, isTaskActive, terminal?.is_completed]);

  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [terminal?.lines, autoScroll]);

  const handleCopy = () => {
    if (!terminal?.lines) return;
    const text = terminal.lines.map((l) => `[${l.stream.toUpperCase()}] ${l.line}`).join('\n');
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="bg-slate-950 border border-slate-800 rounded-xl overflow-hidden flex flex-col h-full shadow-lg font-mono">
      {/* Terminal Bar */}
      <div className="flex items-center justify-between px-4 py-2.5 bg-slate-900 border-b border-slate-800 text-xs select-none">
        <div className="flex items-center space-x-3">
          <div className="flex space-x-1.5">
            <div className="w-3 h-3 rounded-full bg-red-500/70"></div>
            <div className="w-3 h-3 rounded-full bg-amber-500/70"></div>
            <div className="w-3 h-3 rounded-full bg-emerald-500/70"></div>
          </div>
          <span className="text-slate-300 font-semibold flex items-center space-x-1.5">
            <svg className="w-3.5 h-3.5 text-indigo-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
            </svg>
            <span>Live Task Terminal</span>
          </span>
          <span className="px-2 py-0.5 text-[10px] bg-indigo-950/60 text-indigo-300 border border-indigo-800/60 rounded">
            Redaction Active
          </span>
        </div>

        <div className="flex items-center space-x-3 text-[11px] font-sans">
          {terminal && (
            <div>
              {terminal.is_completed ? (
                terminal.exit_code === 0 ? (
                  <span className="px-2 py-0.5 bg-emerald-950/80 text-emerald-300 border border-emerald-800 rounded font-semibold flex items-center space-x-1">
                    <span className="w-1.5 h-1.5 rounded-full bg-emerald-400"></span>
                    <span>Exit 0 (Success)</span>
                  </span>
                ) : (
                  <span className="px-2 py-0.5 bg-red-950/80 text-red-300 border border-red-800 rounded font-semibold flex items-center space-x-1">
                    <span className="w-1.5 h-1.5 rounded-full bg-red-400"></span>
                    <span>Exit {terminal.exit_code ?? 1} (Failed)</span>
                  </span>
                )
              ) : (
                <span className="px-2 py-0.5 bg-amber-950/80 text-amber-300 border border-amber-800 rounded font-semibold flex items-center space-x-1 animate-pulse">
                  <span className="w-1.5 h-1.5 rounded-full bg-amber-400"></span>
                  <span>Executing...</span>
                </span>
              )}
            </div>
          )}

          <label className="flex items-center space-x-1 text-slate-400 cursor-pointer hover:text-slate-200">
            <input
              type="checkbox"
              checked={autoScroll}
              onChange={(e) => setAutoScroll(e.target.checked)}
              className="rounded bg-slate-800 border-slate-700 text-indigo-600 focus:ring-0 text-xs"
            />
            <span className="text-[10px]">Auto-scroll</span>
          </label>

          <button
            onClick={handleCopy}
            className="p-1 text-slate-400 hover:text-slate-200 hover:bg-slate-800 rounded transition"
            title="Copy Output"
          >
            {copied ? (
              <span className="text-emerald-400 text-[10px]">Copied!</span>
            ) : (
              <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M8 5H6a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2v-1M8 5a2 2 0 002 2h2a2 2 0 002-2M8 5a2 2 0 012-2h2a2 2 0 012 2m0 0h2a2 2 0 012 2v3m2 4H10m0 0l3-3m-3 3l3 3" />
              </svg>
            )}
          </button>
        </div>
      </div>

      {/* Terminal Buffer Content */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto p-4 space-y-1 text-xs leading-relaxed select-text"
      >
        {loading && !terminal ? (
          <div className="text-slate-500 py-6 text-center font-sans">
            Connecting to task execution stream...
          </div>
        ) : error ? (
          <div className="text-red-400 py-4 font-sans text-center">
            {error}
          </div>
        ) : !terminal?.lines || terminal.lines.length === 0 ? (
          <div className="text-slate-500 py-6 text-center font-sans">
            No terminal output recorded for this task yet.
          </div>
        ) : (
          terminal.lines.map((line, idx) => {
            const isStderr = line.stream === 'stderr';
            const isSystem = line.stream === 'system';

            let tagColor = 'text-blue-400 bg-blue-950/40 border-blue-800/40';
            let textColor = 'text-slate-200';

            if (isStderr) {
              tagColor = 'text-red-400 bg-red-950/40 border-red-800/40';
              textColor = 'text-red-300';
            } else if (isSystem) {
              tagColor = 'text-purple-400 bg-purple-950/40 border-purple-800/40';
              textColor = 'text-slate-400';
            }

            const timeStr = new Date(line.timestamp).toLocaleTimeString();

            return (
              <div key={idx} className="flex items-start space-x-2.5 hover:bg-slate-900/60 py-0.5 px-1.5 rounded">
                <span className="text-slate-600 text-[10px] select-none font-mono shrink-0">
                  {timeStr}
                </span>
                <span className={`px-1.5 py-0.2 text-[9px] uppercase font-semibold rounded border shrink-0 select-none ${tagColor}`}>
                  {line.stream}
                </span>
                <span className={`flex-1 break-all whitespace-pre-wrap ${textColor}`}>
                  {line.line}
                </span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};
