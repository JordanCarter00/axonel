import React, { useState, useEffect, useRef } from 'react';
import { EventRecord } from '../types';
import { api } from '../services/api';
import { eventStream, ConnectionState } from '../services/sse';
import {
  Radio,
  Search,
  Filter,
  ArrowDownCircle,
  Clock,
  ChevronDown,
  ChevronRight,
  Layers,
  Bot,
  Zap,
  MessageSquare,
} from 'lucide-react';

interface LiveTimelineViewProps {
  onSelectWorkflow: (id: string) => void;
}

export const LiveTimelineView: React.FC<LiveTimelineViewProps> = ({ onSelectWorkflow }) => {
  const [events, setEvents] = useState<EventRecord[]>([]);
  const [connectionState, setConnectionState] = useState<ConnectionState>(
    eventStream.getConnectionState()
  );
  const [cursor, setCursor] = useState<number>(eventStream.getCursor());
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedType, setSelectedType] = useState<string>('ALL');
  const [autoScroll, setAutoScroll] = useState<boolean>(true);
  const [expandedEvents, setExpandedEvents] = useState<Set<string>>(new Set());

  const endRef = useRef<HTMLDivElement>(null);

  // Load initial historical events
  useEffect(() => {
    api
      .listEvents({ limit: 100 })
      .then((initialEvents) => {
        setEvents(initialEvents);
      })
      .catch((err) => console.error('Failed to load initial events:', err));
  }, []);

  // Listen to live SSE events and connection changes
  useEffect(() => {
    const unsubState = eventStream.onStateChange((state, cur) => {
      setConnectionState(state);
      setCursor(cur);
    });

    const unsubEvents = eventStream.subscribeAll((event) => {
      setEvents((prev) => {
        // Prevent duplicate sequences
        if (prev.some((e) => e.sequence === event.sequence)) {
          return prev;
        }
        return [...prev, event];
      });
    });

    return () => {
      unsubState();
      unsubEvents();
    };
  }, []);

  // Auto scroll to bottom
  useEffect(() => {
    if (autoScroll && endRef.current) {
      endRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [events, autoScroll]);

  const toggleExpand = (id: string) => {
    setExpandedEvents((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const getEventBadge = (type: string) => {
    if (type.includes('Workflow')) {
      return 'bg-indigo-500/10 text-indigo-400 border-indigo-500/30';
    }
    if (type.includes('message') || type.includes('Message')) {
      return 'bg-cyan-500/10 text-cyan-400 border-cyan-500/30';
    }
    if (type.includes('Task')) {
      return 'bg-sky-500/10 text-sky-400 border-sky-500/30';
    }
    if (type.includes('Agent') || type.includes('agent')) {
      return 'bg-purple-500/10 text-purple-400 border-purple-500/30';
    }
    if (type.includes('tool') || type.includes('Tool')) {
      return 'bg-amber-500/10 text-amber-300 border-amber-500/30';
    }
    if (type.includes('Approval')) {
      return 'bg-amber-500/10 text-amber-400 border-amber-500/30';
    }
    if (type.includes('Verification')) {
      return 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30';
    }
    if (type.includes('Recovery') || type.includes('Failed')) {
      return 'bg-rose-500/10 text-rose-400 border-rose-500/30';
    }
    return 'bg-slate-800 text-slate-400 border-slate-700';
  };

  const eventTypes = ['ALL', ...Array.from(new Set(events.map((e) => e.event_type)))];

  const filteredEvents = events.filter((e) => {
    const matchesType = selectedType === 'ALL' || e.event_type === selectedType;
    const matchesSearch =
      searchQuery === '' ||
      e.event_type.toLowerCase().includes(searchQuery.toLowerCase()) ||
      JSON.stringify(e.payload).toLowerCase().includes(searchQuery.toLowerCase()) ||
      (e.workflow_id && e.workflow_id.includes(searchQuery)) ||
      (e.task_id && e.task_id.includes(searchQuery));
    return matchesType && matchesSearch;
  });

  return (
    <div className="space-y-4 pb-12">
      {/* Header Controls */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold text-slate-100 flex items-center space-x-2">
            <Radio className="w-5 h-5 text-emerald-400 animate-pulse" />
            <span>Live Audit & Execution Timeline</span>
          </h1>
          <p className="text-xs text-slate-400">
            Immutable SSE event stream with sequence cursor tracking and durable catch-up
          </p>
        </div>

        <div className="flex items-center space-x-3">
          <div className="flex items-center space-x-2 bg-surface px-3 py-1.5 rounded border border-surface-border text-xs font-mono">
            <span
              className={`w-2 h-2 rounded-full ${
                connectionState === 'connected' ? 'bg-emerald-400' : 'bg-amber-400'
              }`}
            />
            <span className="text-slate-300">{connectionState}</span>
            <span className="text-slate-500">|</span>
            <span className="text-slate-400">seq #{cursor}</span>
          </div>

          <button
            onClick={() => setAutoScroll(!autoScroll)}
            className={`flex items-center space-x-1.5 px-3 py-1.5 rounded text-xs font-mono border transition-colors ${
              autoScroll
                ? 'bg-primary-600/20 text-indigo-300 border-primary-500/40'
                : 'bg-surface text-slate-400 border-surface-border'
            }`}
          >
            <ArrowDownCircle className="w-3.5 h-3.5" />
            <span>Auto-Scroll: {autoScroll ? 'ON' : 'OFF'}</span>
          </button>
        </div>
      </div>

      {/* Filter and Search */}
      <div className="flex flex-col sm:flex-row items-center justify-between gap-3 bg-surface border border-surface-border p-3 rounded-lg">
        <div className="relative w-full sm:w-72">
          <Search className="w-4 h-4 text-slate-400 absolute left-3 top-2.5" />
          <input
            type="text"
            placeholder="Search payload, task, workflow..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full bg-[#0a0d14] border border-surface-border rounded-md pl-9 pr-3 py-1.5 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-indigo-500"
          />
        </div>

        <div className="flex items-center space-x-2 w-full sm:w-auto">
          <Filter className="w-3.5 h-3.5 text-slate-400" />
          <select
            value={selectedType}
            onChange={(e) => setSelectedType(e.target.value)}
            className="bg-[#0a0d14] border border-surface-border rounded px-2.5 py-1 text-xs text-slate-200"
          >
            {eventTypes.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
          <span className="text-xs text-slate-500 font-mono">({filteredEvents.length} events)</span>
        </div>
      </div>

      {/* Event Stream Log Box */}
      <div className="bg-[#0a0d14] border border-surface-border rounded-lg overflow-hidden font-mono divide-y divide-[#151d2f]">
        {filteredEvents.length === 0 ? (
          <div className="p-12 text-center text-slate-500 text-xs">
            No events match current filter or waiting for new events...
          </div>
        ) : (
          filteredEvents.map((evt) => {
            const isExpanded = expandedEvents.has(evt.event_id);
            return (
              <div key={evt.sequence} className="p-3 hover:bg-[#111726]/60 transition-colors">
                <div className="flex items-center justify-between text-xs cursor-pointer" onClick={() => toggleExpand(evt.event_id)}>
                  <div className="flex items-center space-x-3">
                    <span className="text-slate-500 text-[11px] w-12 text-right">
                      #{evt.sequence}
                    </span>

                    <span
                      className={`text-[10px] uppercase font-bold px-2 py-0.5 rounded border ${getEventBadge(
                        evt.event_type
                      )}`}
                    >
                      {evt.event_type}
                    </span>

                    {evt.workflow_id && (
                      <span
                        onClick={(e) => {
                          e.stopPropagation();
                          onSelectWorkflow(evt.workflow_id!);
                        }}
                        className="text-[11px] text-indigo-400 hover:underline flex items-center space-x-1"
                      >
                        <Layers className="w-3 h-3" />
                        <span>wf:{evt.workflow_id.slice(0, 6)}</span>
                      </span>
                    )}

                    {evt.agent_id && (
                      <span className="text-[11px] text-purple-400 flex items-center space-x-1">
                        <Bot className="w-3 h-3" />
                        <span>agent:{evt.agent_id.slice(0, 6)}</span>
                      </span>
                    )}

                    {evt.task_id && (
                      <span className="text-[11px] text-sky-400 flex items-center space-x-1">
                        <Zap className="w-3 h-3" />
                        <span>task:{evt.task_id.slice(0, 6)}</span>
                      </span>
                    )}

                    {evt.event_type === 'message_sent' && evt.payload && (
                      <span className="text-[11px] text-cyan-300 font-sans italic truncate max-w-sm flex items-center space-x-1">
                        <MessageSquare className="w-3 h-3 text-cyan-400 shrink-0" />
                        <span className="truncate">
                          {(evt.payload as any).role ? `[${String((evt.payload as any).role)}] ` : ''}
                          {String((evt.payload as any).content || 'Collaboration message')}
                        </span>
                      </span>
                    )}
                  </div>

                  <div className="flex items-center space-x-3 text-slate-500 text-[11px]">
                    <div className="flex items-center space-x-1">
                      <Clock className="w-3 h-3" />
                      <span>{new Date(evt.timestamp).toLocaleTimeString()}</span>
                    </div>
                    {isExpanded ? (
                      <ChevronDown className="w-4 h-4 text-slate-400" />
                    ) : (
                      <ChevronRight className="w-4 h-4 text-slate-400" />
                    )}
                  </div>
                </div>

                {/* Expanded JSON payload view */}
                {isExpanded && (
                  <div className="mt-2.5 ml-14 p-3 bg-[#06080d] rounded border border-surface-border text-[11px] text-slate-300 overflow-x-auto">
                    <pre>{JSON.stringify(evt.payload, null, 2)}</pre>
                  </div>
                )}
              </div>
            );
          })
        )}
        <div ref={endRef} />
      </div>
    </div>
  );
};
